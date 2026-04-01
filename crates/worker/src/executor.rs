use std::usize;

use tokio_util::sync::CancellationToken;
use tonic::{Request, Status, Response};
use uuid::Uuid;

use shared::executor_server::Executor;
use shared::{CancelJobRequest, CancelJobResponse, JobRequest, JobResponse};

use wasmtime::Store;
use wasmtime::component::{Component, ResourceTable};
use wasmtime_wasi::p2::bindings::Command;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::credit_guard::JobGuard;
use crate::worker::Worker;
use crate::errors::ExecutorError;

// Required by wasmtime
pub struct ComponentRunStates {
    pub wasi_ctx: WasiCtx,
    pub resource_table: ResourceTable,
}

/// Exposes the WASI context and resource table to wasmtime-wasi's host function implementations.
impl WasiView for ComponentRunStates {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

impl ComponentRunStates {
    /// An easy way to create a ComponentRunStates with a default ResourceTable and the
    /// given WasiCtx
    pub fn new(wasi_ctx: WasiCtx) -> Self {
        Self { wasi_ctx, resource_table: ResourceTable::new() }
    }
}

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

        // Extract request info
        let request = request.into_inner();
        let job_id = Uuid::from_slice(&request.job_id)
            .unwrap_or_else(|e| panic!("received malformed uuid: {}", e));
        let wasm_bytes = request.wasm_bytes;
        let mut wasi_args = vec![job_id.to_string()];
        wasi_args.extend(request.args);


        // RAII credit guard to send credit update back to Orchestrator when dropped
        // and removes cancellation token
        let cancellation_token = CancellationToken::new();
        self.cancellation_tokens.insert(job_id, cancellation_token.clone());
        let _credit_guard = JobGuard::new(
            self.orchestrator_tx.clone(), 
            self.cancellation_tokens.clone(),
            job_id
        );
        
        
        // TODO: cache component once compiled
        let engine = self.wasm_engine.clone();
        let component = tokio::task::spawn_blocking(move || {
            Component::from_binary(&engine, &wasm_bytes)
                .map_err(ExecutorError::CompilationFailed)
        })
        .await
        .unwrap_or_else(|e| panic!("compilation task panicked: {e}"))?;

        let stdout_pipe = MemoryOutputPipe::new(usize::MAX);
        let stderr_pipe = MemoryOutputPipe::new(usize::MAX);

        let wasi_ctx = WasiCtx::builder()
            .args(&wasi_args)
            .stdout(stdout_pipe.clone())
            .stderr(stderr_pipe.clone())
            // .inherit_network() // TODO: check if i should use this
            .build();

        // TODO: add env and file system
        let state = ComponentRunStates::new(wasi_ctx);
        let mut store = Store::new(&self.wasm_engine, state);

        store.epoch_deadline_async_yield_and_update(1);
        store.set_epoch_deadline(1);

        let command = Command::instantiate_async(&mut store, &component, &self.wasm_linker).await
            .map_err(ExecutorError::InstantiationFailed)?;

        let run_result = tokio::select! {
            result = command.wasi_cli_run().call_run(&mut store) => result,
            _ = cancellation_token.cancelled() => {
                return Err(ExecutorError::JobCancelled.into())
            }
        };

        let stdout = stdout_pipe.contents().to_vec();
        let stderr = stderr_pipe.contents().to_vec();

        match run_result {
            Ok(Ok(())) => Ok(Response::new(JobResponse { stdout, stderr })),
            Ok(Err(())) => Err(ExecutorError::ExecutionFailed(format!("stderr: {}", String::from_utf8_lossy(&stderr))).into()),
            Err(e) => {
                if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                    match exit.0 {
                        0 => Ok(Response::new(JobResponse { stdout, stderr })),
                        code => Err(ExecutorError::ExecutionFailed(format!("exited with code {code}, stderr: {}: {}", code, String::from_utf8_lossy(&stderr))).into()),
                    }
                } else {
                    Err(ExecutorError::Unknown(e.to_string()).into())
                }
            }
        }
    }

    /// A function exposed by the Worker for the Client to call
    /// to cancel a job that is currently being run by this Worker. 
    /// Returns an error on invalid job id.
    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>
    ) -> Result<Response<CancelJobResponse>, Status> {
        let job_id = Uuid::from_slice(&request.into_inner().job_id)
            .unwrap_or_else(|e| panic!("received malformed job id: {}", e));
        
        // Cancel the job via the cancellation token
        match self.cancellation_tokens.get(&job_id) {
            Some(cancellation_token) => {
                cancellation_token.cancel();
                self.wasm_engine.increment_epoch(); // increment the epoch immediately so control is yielded back
                Ok(Response::new(CancelJobResponse {}))
            },
            None => {
                Err(ExecutorError::JobNotFound.into())
            }
        }
    }
}