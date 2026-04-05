use std::sync::Arc;

use jsonwebtoken::{DecodingKey, Validation};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;
use tonic::metadata::MetadataMap;
use tonic::{Request, Status, Response};
use uuid::Uuid;

use shared::executor_server::Executor;
use shared::{CancelJobRequest, CancelJobResponse, JobClaims, JobRequest, JobResponse};

use wasmtime::Store;
use wasmtime::component::{Component, ResourceTable};
use wasmtime_wasi::p2::bindings::Command;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::job_guard::JobGuard;
use crate::worker::Worker;
use crate::errors::ExecutorError;

/// Required by wasmtime
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
        let (metadata, _extensions, request) = request.into_parts();
        let job_id = Uuid::from_slice(&request.job_id)
            .unwrap_or_else(|e| {
                eprintln!("ERROR: worker received a malformed job id from the client, this should never happen: {}", e);
                std::process::exit(1);
            });
        let wasm_bytes = request.wasm_bytes;
        let mut wasi_args = vec![job_id.to_string()];
        wasi_args.extend(request.args);

        // Check authentication
        self.check_client_auth(&metadata, job_id)?;

        // RAII credit guard to send credit update back to Orchestrator when dropped
        // and removes cancellation token
        let cancellation_token = CancellationToken::new();
        self.cancellation_tokens.insert(job_id, cancellation_token.clone());

        let worker = self.clone();
        let execute_task = tokio::spawn(async move {
            let _job_guard = JobGuard::new(
                worker.orchestrator_tx.clone(), 
                worker.cancellation_tokens.clone(),
                job_id
            );

            let wasm_hash = blake3::hash(&wasm_bytes);

            let cell = worker.component_cache.lock().await
                .get_or_insert(wasm_hash, || Arc::new(OnceCell::new()))
                .clone();

            let component = cell.get_or_try_init(|| async {
                let engine = worker.wasm_engine.clone();
                tokio::task::spawn_blocking(move || {
                    Component::from_binary(&engine, &wasm_bytes)
                        .map_err(ExecutorError::CompilationFailed)
                })
                .await
                .unwrap_or_else(|e| {
                    eprintln!("ERROR: wasm compilation task panicked, this should never happen: {e}");
                    std::process::exit(1);
                })
            })
            .await?;

            let stdout_pipe = MemoryOutputPipe::new(10 * 1024 * 1024); // 10 MB
            let stderr_pipe = MemoryOutputPipe::new(10 * 1024 * 1024); // 10 MB

            let wasi_ctx = WasiCtx::builder()
                .args(&wasi_args)
                .stdout(stdout_pipe.clone())
                .stderr(stderr_pipe.clone())
                .build();

            // TODO: add env and file system
            let state = ComponentRunStates::new(wasi_ctx);
            let mut store = Store::new(&worker.wasm_engine, state);

            store.epoch_deadline_async_yield_and_update(1);
            store.set_epoch_deadline(1);

            let command = Command::instantiate_async(&mut store, component, &worker.wasm_linker).await
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
        });

        execute_task.await.unwrap_or_else(|e| Err(ExecutorError::ExecutionTaskFailed(e.to_string()).into()))
    }

    /// A function exposed by the Worker for the Client to call
    /// to cancel a job that is currently being run by this Worker. 
    /// Returns an error on invalid job id.
    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>
    ) -> Result<Response<CancelJobResponse>, Status> {
        let (metadata, _extensions, request) = request.into_parts();
        let job_id = Uuid::from_slice(&request.job_id)
            .unwrap_or_else(|e| {
                eprintln!("ERROR: worker received a malformed job id from the client, this should never happen: {}", e);
                std::process::exit(1);
            });

        // Check authentication
        self.check_client_auth(&metadata, job_id)?;
        
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

impl Worker {
    /// Verifies the JWT token in the request metadata matches the given job_id.
    /// Returns Unauthenticated if the token is missing, invalid, or bound to a different job.
    fn check_client_auth(&self, metadata: &MetadataMap, job_id: Uuid) -> Result<(), ExecutorError> {
        let jwt_token = metadata.get("authorization")
            .ok_or(ExecutorError::Unauthenticated)?;

        let secret = self.jwt_secret.get()
            .unwrap_or_else(|| {
                eprintln!("ERROR: worker should always have received jwt secret before running a job, this should never happen");
                std::process::exit(1);
            });

        let token_data = jsonwebtoken::decode(
            jwt_token, 
            &DecodingKey::from_secret(secret), 
            &Validation::default()
        ).map_err(|_| ExecutorError::Unauthenticated)?;

        let job_claims: JobClaims = token_data.claims;

        if job_claims.sub != job_id {
            Err(ExecutorError::Unauthenticated)
        } else {
            Ok(())
        }
    }
}