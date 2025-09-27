use mini_lambda_proto::{RegisterWorkerRequest, RegisterWorkerResponse, UnregisterWorkerRequest};
use reqwest::Client;
use tracing::{error, info};
use uuid::Uuid;

use crate::{errors::WorkerError, INITIAL_CREDITS};

/// Register this worker with the orchestrator, returning the assigned worker_id.
pub async fn register_with_orchestrator(client: &Client, base: &str, port: u16) -> Result<Uuid, WorkerError> {
    let url = format!("{}/register_worker", base);
    info!("registering with orchestrator at {}", url);
    let req = RegisterWorkerRequest { port, initial_credits: INITIAL_CREDITS };

    let resp = client.post(&url).json(&req).send().await.map_err(|e| {
        WorkerError::Registration(format!("failed to send register request: {}", e))
    })?;

    let resp = resp.error_for_status().map_err(|e| WorkerError::Registration(format!("register returned error: {}", e)))?;
    let body: RegisterWorkerResponse = resp.json().await.map_err(|e| WorkerError::Registration(format!("failed to parse response: {}", e)))?;

    info!("successfully registered worker {} at {}", body.worker_id, url);
    Ok(body.worker_id)
}

/// Unregister the worker from the orchestrator (called on shutdown).
pub async fn unregister_with_orchestrator(client: &Client, base: &str, worker_id: Uuid) -> Result<(), WorkerError> {
    let url = format!("{}/unregister_worker", base);
    info!("unregistering worker {} with orchestrator at {}", worker_id, url);
    let req = UnregisterWorkerRequest { worker_id };

    let resp = client.post(&url).json(&req).send().await.map_err(|e| WorkerError::Unregistration(format!("failed to send unregister: {}", e)))?;
    if resp.status().is_success() {
        info!("successfully unregistered worker {} at {}", worker_id, url);
        Ok(())
    } else {
        Err(WorkerError::Unregistration(format!("unregister returned {}", resp.status())))
    }
}