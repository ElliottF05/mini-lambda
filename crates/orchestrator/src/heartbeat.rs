use std::f32::consts::E;

// ...existing code...
use axum::{extract::Extension, Json};
use axum::http::StatusCode;
use tracing::info;

use crate::queue::PendingQueue;
use crate::registry::WorkerRegistry;
use crate::errors::OrchestratorError;
use mini_lambda_proto::HeartbeatUpdate;


/// Handler that accepts the worker heartbeat payload and updates the registry.
pub async fn handle_heartbeat_received(
    Extension(registry): Extension<WorkerRegistry>,
    Extension(pending_queue): Extension<PendingQueue>,
    Json(heartbeat): Json<HeartbeatUpdate>,
) -> Result<StatusCode, OrchestratorError> {
    // info!("heartbeat received for: worker={} seq={} credits={}", heartbeat.worker_id, heartbeat.seq, heartbeat.credits);

    // update the registry with the new credits
    let ok = registry.update_credits(heartbeat.worker_id, heartbeat.credits).await;
    if !ok {
        return Err(OrchestratorError::WorkerNotFound);
    }

    if heartbeat.credits == 0 {
        // no available credits, nothing to do
        return Ok(StatusCode::OK);
    }

    let worker_endpoint = match registry.get_worker_info(heartbeat.worker_id).await {
        Some(info) => info.endpoint,
        None => return Err(OrchestratorError::WorkerNotFound),
    };

    let mut assigned = 0;
    while assigned < heartbeat.credits {
        // try to dequeue a job
        if let Some(job) = pending_queue.dequeue().await {
            // send the worker endpoint to the job responder
            if let Err(_err) = job.responder.send(worker_endpoint.clone()) {
                // receiver dropped, likely due to timeout - just log and continue
                info!("job {} responder dropped, likely timed out", job.job_id);
            } else {
                assigned += 1;
            }
        } else {
            // no more pending jobs
            break;
        }
    }

    let remaining_credits = heartbeat.credits.saturating_sub(assigned);
    let _ = registry.update_credits(heartbeat.worker_id, remaining_credits).await;
    
    Ok(StatusCode::OK)
}