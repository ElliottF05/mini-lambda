use std::net::SocketAddr;

use axum::{body::Bytes, extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router, ServiceExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info};
use uuid::Uuid;
use clap::Parser;
use axum::extract::ConnectInfo;
use tower_http::trace::TraceLayer;

mod registry;
use registry::WorkerRegistry;
use mini_lambda_proto::{RegisterWorkerRequest, RegisterWorkerResponse, UnregisterWorkerRequest};

// proto types aren't required by this handler (no-body request_worker). Keep proto in the workspace for other crates.

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("no workers registered")]
    NoWorkers,

    #[error("worker not found")]
    WorkerNotFound,
}

impl IntoResponse for OrchestratorError {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match &self {
            OrchestratorError::NoWorkers => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            OrchestratorError::WorkerNotFound => (StatusCode::NOT_FOUND, self.to_string()),
        };
        (status, Json(serde_json::json!({ "error": body }))).into_response()
    }
}


// Reuse proto types for register request/response. We expect workers to send { port: u16 }.

#[derive(Deserialize, Debug)]
struct UpdateQueueRequest {
    worker_id: Uuid,
    queue_len: usize,
}

#[derive(Serialize, Debug)]
struct OrchestratorSubmitResponse {
    job_id: Uuid,
    worker_endpoint: String,
}

async fn register_worker(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<RegisterWorkerRequest>,
) -> (StatusCode, Json<RegisterWorkerResponse>) {

    info!("registering worker: {:?}", req);
    // build endpoint from peer.ip() and the port the worker reports
    let endpoint = format!("http://{}:{}", peer.ip(), req.port);
    let id = registry.register(endpoint).await;
    (
        StatusCode::CREATED,
        Json(RegisterWorkerResponse { worker_id: id }),
    )
}

async fn unregister_worker(
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

async fn update_queue(
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<UpdateQueueRequest>,
) -> Result<StatusCode, OrchestratorError> {

    info!("updating queue for worker: {:?}", req);
    if registry.update_queue(req.worker_id, req.queue_len).await {
        Ok(StatusCode::OK)
    } else {
        Err(OrchestratorError::WorkerNotFound)
    }
}

async fn request_worker(
    Extension(registry): Extension<WorkerRegistry>,
) -> Result<(StatusCode, Json<OrchestratorSubmitResponse>), OrchestratorError> {

    info!("requesting worker");
    if let Some((_id, endpoint)) = registry.pick_and_increment().await {
        let resp = OrchestratorSubmitResponse {
            job_id: Uuid::new_v4(),
            worker_endpoint: endpoint,
        };

        Ok((StatusCode::CREATED, Json(resp)))
    } else {
        Err(OrchestratorError::NoWorkers)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    #[derive(Parser)]
    struct Opts {
        /// address to bind, e.g. 127.0.0.1:8080
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
    }

    let opts = Opts::parse();

    let registry = WorkerRegistry::new();

    // bind to configured address
    let listener = tokio::net::TcpListener::bind(&opts.bind).await.unwrap();
    let app = Router::new()
        .route("/register_worker", post(register_worker))
        .route("/unregister_worker", post(unregister_worker))
        .route("/update_queue", post(update_queue))
        .route("/request_worker", post(request_worker))
        .layer(TraceLayer::new_for_http()) // add request tracing
        .layer(Extension(registry)); // inject registry

    info!("orchestrator listening on {}", opts.bind);

    let server = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>());
    let graceful = server.with_graceful_shutdown(async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }
}
