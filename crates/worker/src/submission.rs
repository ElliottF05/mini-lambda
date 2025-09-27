use std::sync::{atomic::AtomicUsize, Arc};

use axum::{Extension, Json};
use mini_lambda_proto::{hash_wasm_module, JobSubmissionHash, JobSubmissionWasm, SubmitResponse};
use reqwest::StatusCode;
use tracing::info;
use uuid::Uuid;
use wasmer::{Engine, Module};

use crate::{errors::WorkerError, module_cache::ModuleCache, job_ticket::JobTicket, runner::run_wasm_module};

pub async fn handle_submit_wasm(
    Extension(engine): Extension<Arc<Engine>>,
    Extension(module_cache): Extension<Arc<tokio::sync::Mutex<ModuleCache>>>,
    Extension(num_jobs): Extension<Arc<AtomicUsize>>,
    Json(data): Json<JobSubmissionWasm>,
) -> Result<(StatusCode, Json<SubmitResponse>), WorkerError> {

    info!("received wasm submission with manifest: {:?}", data.manifest);

    if data.module_bytes.is_empty() {
        return Err(WorkerError::Validation("empty wasm module".into()));
    }

    let module = Module::new(&engine, &data.module_bytes).map_err(|e| WorkerError::Compile(e.to_string()))?;
    let module = Arc::new(module);

    let module_hash = hash_wasm_module(&data.module_bytes);

    let mut module_cache = module_cache.lock().await;
    module_cache.put(module_hash, module.clone());

    // acquire a job ticket to increase the active job count
    let ticket = JobTicket::acquire(num_jobs.clone());

    let buf = run_wasm_module((*engine).clone(), (*module).clone(), data.manifest.call_args, ticket).await?;

    let resp = SubmitResponse {
        job_id: Uuid::new_v4(),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

pub async fn handle_submit_hash(
    Extension(engine): Extension<Arc<Engine>>,
    Extension(module_cache): Extension<Arc<tokio::sync::Mutex<ModuleCache>>>,
    Extension(num_jobs): Extension<Arc<AtomicUsize>>,
    Json(data): Json<JobSubmissionHash>,
) -> Result<(StatusCode, Json<SubmitResponse>), WorkerError> {

    info!("received hash submission with manifest: {:?}", data.manifest);
    
    if data.module_hash.is_empty() {
        return Err(WorkerError::Validation("empty module hash".into()));
    }

    let mut module_cache = module_cache.lock().await;
    let module = module_cache.get(&data.module_hash).ok_or_else(|| WorkerError::ModuleNotFound(data.module_hash.clone()))?;

    // acquire a job ticket to increase the active job count
    let ticket = JobTicket::acquire(num_jobs.clone());

    let buf = run_wasm_module((*engine).clone(), (*module).clone(), data.manifest.call_args, ticket).await?;

    let resp = SubmitResponse {
        job_id: Uuid::new_v4(),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    Ok((StatusCode::CREATED, Json(resp)))
}