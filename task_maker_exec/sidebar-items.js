initSidebarItems({"enum":[["RawSandboxResult","Response of the internal implementation of the sandbox."]],"fn":[["derive_key_from_password","Derive the encryption key from a password string."],["eval_dag_locally","Evaluate a DAG locally spawning a new `LocalExecutor` with the specified number of workers."]],"mod":[["executors","The supported executors."],["proto","The protocol related structs and enums."]],"struct":[["ClientInfo","Information about a client of the scheduler."],["ErrorSandboxRunner","A fake sandbox that don’t actually spawn anything and always return an error."],["ExecutorClient","This is a client of the `Executor`, the client is who sends a DAG for an evaluation, provides some files and receives the callbacks from the server. When the server notifies a callback function is called by the client."],["ExecutorStatus","The current status of the `Executor`, this is sent to the user when the server status is asked."],["ExecutorWorkerStatus","Status of a worker of an `Executor`."],["SuccessSandboxRunner","A fake sandbox that don’t actually spawn anything and always return successfully with exit code 0."],["UnsafeSandboxRunner","A fake sandbox that simply spawns the process and does not measure anything. No actual sandboxing is performed, so the process may do bad things."],["Worker","The worker is the component that receives the work from the server and sends the results back. It computes the results by executing a process inside a sandbox, limiting the available resources and measuring the used ones."],["WorkerConn","An handle of the connection to the worker."],["WorkerCurrentJobStatus","Information about the job the worker is currently doing."]],"trait":[["SandboxRunner","Something able to spawn a sandbox, wait for it to exit and return the results."]]});