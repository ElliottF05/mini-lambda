use std::net::SocketAddr;

use axum::{extract::ConnectInfo, Extension, Json};
use mini_lambda_proto::{RegisterWorkerRequest, RegisterWorkerResponse, UnregisterWorkerRequest};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::{errors::OrchestratorError, registry::WorkerRegistry};

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
) -> Result<(StatusCode, Json<OrchestratorSubmitResponse>), OrchestratorError> {

    info!("requesting worker");
    if let Some((_id, endpoint)) = registry.pick_and_decrement().await {
        let resp = OrchestratorSubmitResponse {
            job_id: Uuid::new_v4(),
            worker_endpoint: endpoint,
        };

        Ok((StatusCode::CREATED, Json(resp)))
    } else {
        Err(OrchestratorError::NoWorkers)
    }
}