use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tonic::{Request, Status, Response};
use wasmer::Module;
use wasmer_wasix::runners::wasi::{RuntimeOrEngine, WasiRunner};
use wasmer_wasix::Pipe;

use shared::executor_server::Executor;
use shared::{JobRequest, JobResponse};

use crate::worker::Worker;
use crate::errors::ExecutorError;

/// Implementation of the Executor service for Worker.
#[tonic::async_trait]
impl Executor for Worker {

    /// The function exposed by the Worker that the Client calls to execute
    /// the submitted WASM job.
    async fn execute_job(
        &self, 
        request: Request<JobRequest>
    ) -> Result<Response<JobResponse>, Status> {
        println!("Received job to execute...");

        // Decrement credits when beginning job execution
        self.credits.fetch_sub(1, Ordering::Relaxed);

        let request = request.into_inner();
        let wasm_bytes = request.wasm_bytes;
        let args = request.args;
        let runtime = self.wasm_runtime.clone();

        // Run the compilation and wasm execution on a separate blocking task
        let result = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), ExecutorError> {
            // Compile the wasm module
            let wasm_module = Module::new(&runtime.engine, wasm_bytes)?;

            // Create pipes to capture stdout and stderr, and buffers for their contents
            let (stdout_tx, mut stdout_rx) = Pipe::channel();
            let (stderr_tx, mut stderr_rx) = Pipe::channel();
            let mut stdout_buf = vec![];
            let mut stderr_buf = vec![];
            
            let run_result = {
                // Configure the runtime environment and run it
                let mut runner = WasiRunner::new();
                runner
                    .with_stdout(Box::new(stdout_tx))
                    .with_stderr(Box::new(stderr_tx))
                    .with_args(args);
                // TODO: env, file system, etc

                // TODO: add program name (job id?)
                runner.run_wasm(
                    RuntimeOrEngine::Runtime(Arc::new(runtime)), 
                    "unnamed", 
                    wasm_module, 
                    wasmer_types::ModuleHash::random()
                )
            };
            
            if let Err(e) = stderr_rx.read_to_end(&mut stderr_buf) {
                eprintln!("Encountered an error capturing stderr: {e}");
                if !stderr_buf.is_empty() {
                    stderr_buf.extend_from_slice(b" | ");
                }
                stderr_buf.extend_from_slice(format!("Encountered an error capturing stderr: {e}").as_bytes());
            }

            run_result.map_err(|e| ExecutorError::ExecutionFailed(format!("{}\nstderr: {}", e, String::from_utf8_lossy(&stderr_buf))))?;

            stdout_rx.read_to_end(&mut stdout_buf)
                .map_err(|e| ExecutorError::OutputCaptureFailed(e.to_string()))?;
            Ok((stdout_buf, stderr_buf))
        })
        .await
        .map_err(ExecutorError::WorkerPanicked);

        // Increment credits on job completion and send update to Orchestrator
        self.credits.fetch_add(1, Ordering::Relaxed);
        self.send_credit_update();

        let (stdout_buf, stderr_buf) = result??;
        let response = JobResponse {
            stdout: stdout_buf,
            stderr: stderr_buf
        };

        Ok(Response::new(response))
    }
}