use crate::executor::WorkerJob;
use crate::proto::*;
use crate::sandbox::{Sandbox, SandboxResult};
use crate::{new_local_channel, ChannelReceiver, ChannelSender, RawSandboxResult};
use failure::{Error, Fail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};
use std::thread;
use tabox::configuration::SandboxConfiguration;
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
    missing_deps: HashMap<FileStoreKey, Vec<FileUuid>>,
}

/// The worker is the component that receives the work from the server and sends the results back.
/// It computes the results by executing a process inside a sandbox, limiting the available
/// resources and measuring the used ones.
pub struct Worker {
    /// The identifier of this worker.
    uuid: WorkerUuid,
    /// The name of this worker.
    name: String,
    /// The channel that sends messages to the server.
    sender: ChannelSender<WorkerClientMessage>,
    /// The channel that receives messages from the server.
    receiver: ChannelReceiver<WorkerServerMessage>,
    /// A reference to the [`FileStore`](../task_maker_store/struct.FileStore.html).
    file_store: Arc<FileStore>,
    /// Job the worker is currently working on.
    current_job: Arc<Mutex<WorkerCurrentJob>>,
    /// Where to put the sandboxes.
    sandbox_path: PathBuf,
    /// The function that spawns an actual sandbox.
    sandbox_runner:
        Arc<dyn Fn(SandboxConfiguration, Arc<AtomicU32>) -> RawSandboxResult + Send + Sync>,
}

/// An handle of the connection to the worker.
pub struct WorkerConn {
    /// The identifier of the worker.
    pub uuid: WorkerUuid,
    /// The name of the worker.
    pub name: String,
    /// The channel that sends messages to the worker.
    pub sender: ChannelSender<WorkerServerMessage>,
    /// The channel that receives messages from the server.
    pub receiver: ChannelReceiver<WorkerClientMessage>,
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
    pub fn new<S: Into<String>, P: Into<PathBuf>, F>(
        name: S,
        file_store: Arc<FileStore>,
        sandbox_path: P,
        runner: F,
    ) -> (Worker, WorkerConn)
    where
        F: Fn(SandboxConfiguration, Arc<AtomicU32>) -> RawSandboxResult + Send + Sync + 'static,
    {
        let (tx, rx_worker) = new_local_channel();
        let (tx_worker, rx) = new_local_channel();
        let uuid = Uuid::new_v4();
        let name = name.into();
        (
            Worker::new_with_channel(
                name.clone(),
                file_store,
                sandbox_path.into(),
                tx_worker,
                rx_worker,
                runner,
            ),
            WorkerConn {
                uuid,
                name,
                sender: tx,
                receiver: rx,
            },
        )
    }

    /// Make a new worker with an already connected channel.
    pub fn new_with_channel<S: Into<String>, P: Into<PathBuf>, F>(
        name: S,
        file_store: Arc<FileStore>,
        sandbox_path: P,
        sender: ChannelSender<WorkerClientMessage>,
        receiver: ChannelReceiver<WorkerServerMessage>,
        runner: F,
    ) -> Worker
    where
        F: Fn(SandboxConfiguration, Arc<AtomicU32>) -> RawSandboxResult + Send + Sync + 'static,
    {
        let uuid = Uuid::new_v4();
        let name = name.into();
        let sandbox_path = sandbox_path.into();
        Worker {
            uuid,
            name,
            sender,
            receiver,
            file_store,
            current_job: Arc::new(Mutex::new(WorkerCurrentJob::new())),
            sandbox_path,
            sandbox_runner: Arc::new(runner),
        }
    }

    /// The worker body, this function will block until the worker disconnects.
    pub fn work(self) -> Result<(), Error> {
        trace!("Worker {} ready, asking for work", self);
        self.sender.send(WorkerClientMessage::GetWork)?;

        let start_job = || -> Result<(), Error> {
            let sandbox = execute_job(
                self.current_job.clone(),
                &self.sender,
                &self.sandbox_path,
                self.sandbox_runner.clone(),
            )?;
            self.current_job.lock().unwrap().current_sandbox = Some(sandbox);
            Ok(())
        };

        loop {
            match self.receiver.recv() {
                Ok(WorkerServerMessage::Work(job)) => {
                    trace!("Worker {} got job: {:?}", self, job);
                    assert!(self.current_job.lock().unwrap().current_job.is_none());
                    let mut missing_deps: HashMap<FileStoreKey, Vec<FileUuid>> = HashMap::new();
                    let mut handles = HashMap::new();
                    for input in job.execution.dependencies().iter() {
                        let key = job
                            .dep_keys
                            .get(&input)
                            .ok_or(WorkerError::MissingDependencyKey { uuid: *input })?;
                        match self.file_store.get(&key) {
                            None => {
                                // ask the file only once
                                if !missing_deps.contains_key(key) {
                                    self.sender
                                        .send(WorkerClientMessage::AskFile(key.clone()))?;
                                }
                                missing_deps.entry(key.clone()).or_default().push(*input);
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
                    let should_start = {
                        let mut job = self.current_job.lock().unwrap();
                        let uuids = job
                            .missing_deps
                            .remove(&key)
                            .expect("Server sent a not required dependency");
                        for uuid in uuids {
                            job.current_job
                                .as_mut()
                                .expect("Received file while doing nothing")
                                .1
                                .insert(uuid, handle.clone());
                        }
                        job.missing_deps.is_empty()
                    };
                    if should_start {
                        start_job()?;
                    }
                }
                Ok(WorkerServerMessage::Exit) => {
                    info!("Worker {} ({}) is asked to exit", self.name, self.uuid);
                    break;
                }
                Ok(WorkerServerMessage::KillJob(job)) => {
                    let current_job = self.current_job.lock().unwrap();
                    if let Some((worker_job, _)) = current_job.current_job.as_ref() {
                        // check that the job is the same
                        if worker_job.execution.uuid == job {
                            if let Some(sandbox) = current_job.current_sandbox.as_ref() {
                                // ask the sandbox to kill the process
                                sandbox.kill();
                            }
                        }
                    }
                }
                Err(e) => {
                    let cause = e.find_root_cause().to_string();
                    if cause == "receiving on an empty and disconnected channel" {
                        trace!("Connection closed: {}", cause);
                    } else {
                        error!("Connection error: {}", cause);
                    }
                    if let Some(sandbox) = self.current_job.lock().unwrap().current_sandbox.as_ref()
                    {
                        sandbox.kill();
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Spawn a new thread that will start the sandbox and will send the results back to the server.
fn execute_job(
    current_job: Arc<Mutex<WorkerCurrentJob>>,
    sender: &ChannelSender<WorkerClientMessage>,
    sandbox_path: &Path,
    runner: Arc<
        dyn Fn(SandboxConfiguration, Arc<AtomicU32>) -> RawSandboxResult + Send + Sync + 'static,
    >,
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
                sender.send(WorkerClientMessage::GetWork).unwrap();
            }}

            let result = match sandbox.run(move |config, pid| runner(config, pid)) {
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
                    sender
                        .send(WorkerClientMessage::WorkerDone(result, Default::default()))
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

            sender
                .send(WorkerClientMessage::WorkerDone(result, outputs.clone()))
                .unwrap();

            for (uuid, key) in outputs.into_iter() {
                sender
                    .send(WorkerClientMessage::ProvideFile(uuid, key))
                    .unwrap();
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
            status: ExecutionStatus::InternalError(error),
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
