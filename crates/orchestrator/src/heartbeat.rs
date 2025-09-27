// ...existing code...
use axum::{extract::Extension, Json};
use axum::http::StatusCode;
use tracing::info;

use crate::registry::WorkerRegistry;
use crate::errors::OrchestratorError;
use mini_lambda_proto::HeartbeatUpdate;


/// Handler that accepts the worker heartbeat payload and updates the registry.
pub async fn handle_heartbeat_received(
    Extension(registry): Extension<WorkerRegistry>,
    Json(heartbeat): Json<HeartbeatUpdate>,
) -> Result<StatusCode, OrchestratorError> {
    info!("heartbeat received for: worker={} seq={} credits={}", heartbeat.worker_id, heartbeat.seq, heartbeat.credits);

    // update the registry with the new credits
    let ok = registry.update_credits(heartbeat.worker_id, heartbeat.credits).await;

    if ok {
        Ok(StatusCode::OK)
    } else {
        Err(OrchestratorError::WorkerNotFound)
    }
}