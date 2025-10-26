use std::{net::SocketAddr, time::Duration};

use axum::{extract::ConnectInfo, Extension, Json};
use mini_lambda_proto::{MonitoringInfo, RegisterWorkerRequest, RegisterWorkerResponse, UnregisterWorkerRequest, WorkerInfo};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::info;
use uuid::Uuid;

use crate::{errors::OrchestratorError, queue::{Job, PendingQueue}, registry::{WorkerRegistry}};

const QUEUE_TIMEOUT_SECS: u64 = 30;

#[derive(Deserialize, Debug)]
pub struct UpdateCreditsRequest {
    worker_id: Uuid,
    credits: usize,
}

#[derive(Serialize, Debug)]
pub struct OrchestratorSubmitResponse {
    job_id: Uuid,
    worker_endpoint: String,
}

pub async fn register_worker(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<RegisterWorkerRequest>,
) -> (StatusCode, Json<RegisterWorkerResponse>) {

    info!("registering worker: {:?}", req);
    // build endpoint from peer.ip() and the port the worker reports
    let endpoint = format!("http://{}:{}", peer.ip(), req.port);
    let id = registry.register(endpoint, req.initial_credits).await;
    (
        StatusCode::CREATED,
        Json(RegisterWorkerResponse { worker_id: id }),
    )
}

pub async fn unregister_worker(
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<UnregisterWorkerRequest>,
) -> Result<StatusCode, OrchestratorError> {

    info!("unregistering worker with id: {:?}", req.worker_id);
    if registry.unregister(req.worker_id).await {
        Ok(StatusCode::OK)
    } else {
        Err(OrchestratorError::WorkerNotFound)
    }
}

pub async fn update_credits(
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<UpdateCreditsRequest>,
) -> Result<StatusCode, OrchestratorError> {

    info!("updating credits for worker: {:?}", req);
    if registry.update_credits(req.worker_id, req.credits).await {
        Ok(StatusCode::OK)
    } else {
        Err(OrchestratorError::WorkerNotFound)
    }
}

pub async fn request_worker(
    Extension(registry): Extension<WorkerRegistry>,
    Extension(pending_queue): Extension<PendingQueue>,
) -> Result<(StatusCode, Json<OrchestratorSubmitResponse>), OrchestratorError> {

    info!("requesting worker");
    if registry.len().await == 0 {
        return Err(OrchestratorError::NoWorkers);
    }

    if let Some((_id, endpoint)) = registry.pick_and_decrement().await {
        let resp = OrchestratorSubmitResponse {
            job_id: Uuid::new_v4(),
            worker_endpoint: endpoint,
        };
        Ok((StatusCode::CREATED, Json(resp)))

    } else {
        info!("no available workers, enqueueing job");
        // no available workers, enqueue the job and add a timeout
        let (tx, rx ) = oneshot::channel();
        let job_id = Uuid::new_v4();
        let job = Job {
            job_id,
            submitted_at: std::time::SystemTime::now(),
            responder: tx,
        };

        // enqueue the job (or return 429 error if queue is full)
        pending_queue.enqueue(job).await.map_err(|_| OrchestratorError::QueueFull)?;

        // wait for a worker to become available or timeout
        let timeout = Duration::from_secs(QUEUE_TIMEOUT_SECS);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(endpoint)) => {
                info!("job {} dequeued from queue and assigned to worker at {}", job_id, endpoint);
                let resp = OrchestratorSubmitResponse {
                    job_id,
                    worker_endpoint: endpoint,
                };
                Ok((StatusCode::CREATED, Json(resp)))
            }
            Ok(Err(_cancelled)) => {
                // sender dropped, treat as server error
                Err(OrchestratorError::Internal)
            }
            Err(_elapsed) => {
                // timeout elapsed, remove job from queue
                info!("job {} timed out waiting for worker", job_id);
                let _ = pending_queue.remove_job_by_id(job_id).await;
                Err(OrchestratorError::RequestTimeout)
            }
        }
    }
}

pub async fn get_monitoring_info(
    Extension(registry): Extension<WorkerRegistry>,
    Extension(pending): Extension<PendingQueue>,
) -> (StatusCode, Json<MonitoringInfo>) {
    // snapshot registry
    let workers_map = registry.snapshot().await;
    let workers: Vec<WorkerInfo> = workers_map.into_values().collect();

    // snapshot pending queue (non-destructive)
    let pending = pending.snapshot().await;

    (
        StatusCode::OK,
        Json(MonitoringInfo { workers, pending }),
    )
}