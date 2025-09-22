use mini_lambda_proto::UnregisterWorkerRequest;
use reqwest::Client;
use tracing::{error, info};
use uuid::Uuid;

use crate::errors::WorkerError;

/// Unregister the worker from the orchestrator (called on shutdown).
pub async fn unregister_worker(client: &Client, base: &str, worker_id: Uuid) -> Result<(), WorkerError> {
    let url = format!("{}/unregister_worker", base);
    let req = UnregisterWorkerRequest { worker_id };

    match client.post(&url).json(&req).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                info!("successfully unregistered worker {} at {}", worker_id, url);
                Ok(())
            } else {
                error!("unregister failed: {} -> status {}", url, resp.status());
                Err(WorkerError::Unregistration(format!("unregister returned {}", resp.status())))
            }
        }
        Err(e) => {
            error!("failed to unregister with orchestrator {}: {}", url, e);
            Err(WorkerError::Unregistration(format!("unregister network error: {}", e)))
        }
    }
}