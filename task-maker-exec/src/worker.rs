use crate::proto::*;
use crate::*;
use failure::{Error, Fail};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use task_maker_dag::*;
use task_maker_store::*;
use uuid::Uuid;

/// The information about the current job the worker is doing.
struct WorkerCurrentJob {
    /// Job currently waiting for, when there is a job running this should be `None`
    current_job: Option<(Box<WorkerJob>, HashMap<FileUuid, FileStoreHandle>)>,
    /// The currently running sandbox.
    current_sandbox: Option<Sandbox>,
    /// The dependencies that are missing and required for the execution start.
    missing_deps: HashMap<FileStoreKey, FileUuid>,
}

/// The worker is the component that receives the work from the server and sends the results back.
/// It computes the results by executing a process inside a sandbox, limiting the available
/// resources and measuring the used ones.
pub(crate) struct Worker {
    /// The identifier of this worker.
    uuid: WorkerUuid,
    /// The name of this worker.
    name: String,
    /// The channel that sends messages to the server.
    sender: ChannelSender,
    /// The channel that receives messages from the server.
    receiver: ChannelReceiver,
    /// A reference to the [`FileStore`](../task_maker_store/struct.FileStore.html).
    file_store: Arc<FileStore>,
    /// Job the worker is currently working on.
    current_job: Arc<Mutex<WorkerCurrentJob>>,
    /// Where to put the sandboxes.
    sandbox_path: PathBuf,
}

/// An handle of the connection to the worker.
pub(crate) struct WorkerConn {
    /// The identifier of the worker.
    pub uuid: WorkerUuid,
    /// The name of the worker.
    pub name: String,
    /// The channel that sends messages to the worker.
    pub sender: ChannelSender,
    /// The channel that receives messages from the server.
    pub receiver: ChannelReceiver,
}

/// An error generated by the worker.
#[derive(Debug, Fail)]
enum WorkerError {
    /// A dependency key is missing from the list of file dependencies.
    #[fail(display = "missing key for dependency {}", uuid)]
    MissingDependencyKey { uuid: Uuid },
}

impl WorkerCurrentJob {
    /// Make a new [`WorkerCurrentJob`](struct.WorkerCurrentJob.html).
    fn new() -> WorkerCurrentJob {
        WorkerCurrentJob {
            current_job: None,
            current_sandbox: None,
            missing_deps: HashMap::new(),
        }
    }
}

impl Worker {
    /// Make a new worker attached to a [`FileStore`](../task_maker_store/struct.FileStore.html),
    /// will return a pair with the actual `Worker` and an handle with the channels to connect to
    /// communicate with the worker.
    pub fn new<S: Into<String>, P: Into<PathBuf>>(
        name: S,
        file_store: Arc<FileStore>,
        sandbox_path: P,
    ) -> (Worker, WorkerConn) {
        let (tx, rx_worker) = new_local_channel();
        let (tx_worker, rx) = new_local_channel();
        let uuid = Uuid::new_v4();
        let name = name.into();
        let sandbox_path = sandbox_path.into();
        (
            Worker {
                uuid,
                name: name.clone(),
                sender: tx_worker,
                receiver: rx_worker,
                file_store,
                current_job: Arc::new(Mutex::new(WorkerCurrentJob::new())),
                sandbox_path,
            },
            WorkerConn {
                uuid,
                name,
                sender: tx,
                receiver: rx,
            },
        )
    }

    /// The worker body, this function will block until the worker disconnects.
    pub fn work(self) -> Result<(), Error> {
        trace!("Worker {} ready, asking for work", self);
        serialize_into(&WorkerClientMessage::GetWork, &self.sender)?;

        let start_job = || -> Result<(), Error> {
            let sandbox = execute_job(self.current_job.clone(), &self.sender, &self.sandbox_path)?;
            self.current_job.lock().unwrap().current_sandbox = Some(sandbox);
            Ok(())
        };

        loop {
            let message = deserialize_from::<WorkerServerMessage>(&self.receiver);
            match message {
                Ok(WorkerServerMessage::Work(job)) => {
                    trace!("Worker {} got job: {:?}", self, job);
                    assert!(self.current_job.lock().unwrap().current_job.is_none());
                    let mut missing_deps = HashMap::new();
                    let mut handles = HashMap::new();
                    for input in job.execution.dependencies().iter() {
                        let key = job
                            .dep_keys
                            .get(&input)
                            .ok_or(WorkerError::MissingDependencyKey { uuid: *input })?;
                        match self.file_store.get(&key) {
                            None => {
                                serialize_into(
                                    &WorkerClientMessage::AskFile(key.clone()),
                                    &self.sender,
                                )?;
                                missing_deps.insert(key.clone(), *input);
                            }
                            Some(handle) => {
                                handles.insert(*input, handle);
                            }
                        }
                    }
                    let job_ready = missing_deps.is_empty();
                    {
                        let mut current_job = self.current_job.lock().unwrap();
                        current_job.missing_deps = missing_deps;
                        current_job.current_job = Some((job, handles));
                    }
                    if job_ready {
                        start_job()?;
                    }
                }
                Ok(WorkerServerMessage::ProvideFile(key)) => {
                    info!("Server sent file {:?}", key);
                    let reader = ChannelFileIterator::new(&self.receiver);
                    let handle = self.file_store.store(&key, reader)?;
                    let mut job = self.current_job.lock().unwrap();
                    let uuid = job
                        .missing_deps
                        .remove(&key)
                        .expect("Server sent a not required dependency");
                    job.current_job
                        .as_mut()
                        .expect("Received file while doing nothing")
                        .1
                        .insert(uuid, handle);
                    if job.missing_deps.is_empty() {
                        start_job()?;
                    }
                }
                Ok(WorkerServerMessage::Exit) => {
                    info!("Worker {} ({}) is asked to exit", self.name, self.uuid);
                    break;
                }
                Err(e) => {
                    let cause = e.find_root_cause().to_string();
                    if cause == "receiving on a closed channel" {
                        trace!("Connection closed: {}", cause);
                        if let Some(sandbox) =
                            self.current_job.lock().unwrap().current_sandbox.as_ref()
                        {
                            sandbox.kill();
                        }
                        break;
                    } else {
                        error!("Connection error: {}", cause);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Spawn a new thread that will start the sandbox and will send the results back to the server.
fn execute_job(
    current_job: Arc<Mutex<WorkerCurrentJob>>,
    sender: &ChannelSender,
    sandbox_path: &Path,
) -> Result<Sandbox, Error> {
    let (job, mut sandbox) = {
        let current_job = current_job.lock().unwrap();
        let job = current_job
            .current_job
            .as_ref()
            .expect("Worker job is gone");
        (
            job.0.clone(),
            Sandbox::new(sandbox_path, &job.0.execution, &job.1)?,
        )
    };
    if job.execution.config().keep_sandboxes {
        sandbox.keep();
    }
    let thread_sender = sender.clone();
    let thread_sandbox = sandbox.clone();
    let thread_job = job.clone();
    // FIXME: if the sandbox fails badly this may deadlock
    thread::Builder::new()
        .name(format!("Sandbox of {}", job.execution.description))
        .spawn(move || {
            let sender = thread_sender;
            let sandbox = thread_sandbox;
            let job = thread_job;

            defer! {{
                let mut job = current_job.lock().unwrap();
                job.current_job = None;
                job.current_sandbox = None;
                serialize_into(&WorkerClientMessage::GetWork, &sender).unwrap();
            }}

            let result = match sandbox.run() {
                Ok(res) => res,
                Err(e) => {
                    let result = ExecutionResult {
                        status: ExecutionStatus::InternalError(format!(
                            "Sandbox failed: {}",
                            e.to_string()
                        )),
                        was_killed: false,
                        was_cached: false,
                        resources: ExecutionResourcesUsage::default(),
                    };
                    serialize_into(
                        &WorkerClientMessage::WorkerDone(result, Default::default()),
                        &sender,
                    )
                    .unwrap();
                    return;
                }
            };
            let result = compute_execution_result(&job.execution, result);

            let mut outputs = HashMap::new();
            let mut output_paths = HashMap::new();
            if let Some(stdout) = job.execution.stdout {
                let path = sandbox.stdout_path();
                outputs.insert(stdout.uuid, FileStoreKey::from_file(&path).unwrap());
                output_paths.insert(stdout.uuid, path);
            }
            if let Some(stderr) = job.execution.stderr {
                let path = sandbox.stderr_path();
                outputs.insert(stderr.uuid, FileStoreKey::from_file(&path).unwrap());
                output_paths.insert(stderr.uuid, path);
            }
            for (path, file) in job.execution.outputs.iter() {
                let path = sandbox.output_path(path);
                // the sandbox process may want to remove a file, consider missing files as empty
                if path.exists() {
                    outputs.insert(file.uuid, FileStoreKey::from_file(&path).unwrap());
                    output_paths.insert(file.uuid, path.clone());
                } else {
                    // FIXME: /dev/null may not be used
                    outputs.insert(file.uuid, FileStoreKey::from_file("/dev/null").unwrap());
                    output_paths.insert(file.uuid, "/dev/null".into());
                }
            }

            serialize_into(
                &WorkerClientMessage::WorkerDone(result, outputs.clone()),
                &sender,
            )
            .unwrap();

            for (uuid, key) in outputs.into_iter() {
                serialize_into(&WorkerClientMessage::ProvideFile(uuid, key), &sender).unwrap();
                ChannelFileSender::send(&output_paths[&uuid], &sender).unwrap();
            }
        })?;
    Ok(sandbox)
}

/// Compute the [`ExecutionResult`](../task_maker_dag/struct.ExecutionResult.html) based on the
/// result of the sandbox.
fn compute_execution_result(execution: &Execution, result: SandboxResult) -> ExecutionResult {
    match result {
        SandboxResult::Success {
            exit_status,
            signal,
            resources,
            was_killed,
        } => ExecutionResult {
            status: execution.status(exit_status, signal, &resources),
            resources,
            was_killed,
            was_cached: false,
        },
        SandboxResult::Failed { error } => ExecutionResult {
            status: ExecutionStatus::InternalError(error.to_string()),
            resources: ExecutionResourcesUsage::default(),
            was_killed: false,
            was_cached: false,
        },
    }
}

impl std::fmt::Display for WorkerConn {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "'{}' ({})", self.name, self.uuid)
    }
}

impl std::fmt::Display for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "'{}' ({})", self.name, self.uuid)
    }
}
