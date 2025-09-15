use axum::{extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::error;
use uuid::Uuid;
mod registry;
use registry::WorkerRegistry;

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


#[derive(Deserialize)]
struct RegisterWorkerRequest {
    endpoint: String,
}

#[derive(Serialize)]
struct RegisterWorkerResponse {
    worker_id: Uuid,
}

#[derive(Deserialize)]
struct UpdateQueueRequest {
    worker_id: Uuid,
    queue_len: usize,
}

#[derive(Serialize)]
struct OrchestratorSubmitResponse {
    job_id: Uuid,
    worker_endpoint: String,
}

async fn register_worker(
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<RegisterWorkerRequest>,
) -> (StatusCode, Json<RegisterWorkerResponse>) {
    let id = registry.register(req.endpoint).await;
    (
        StatusCode::CREATED,
        Json(RegisterWorkerResponse { worker_id: id }),
    )
}

async fn update_queue(
    Extension(registry): Extension<WorkerRegistry>,
    Json(req): Json<UpdateQueueRequest>,
) -> Result<StatusCode, OrchestratorError> {
    if registry.update_queue(req.worker_id, req.queue_len).await {
        Ok(StatusCode::OK)
    } else {
        Err(OrchestratorError::WorkerNotFound)
    }
}

async fn request_worker(
    Extension(registry): Extension<WorkerRegistry>,
) -> Result<(StatusCode, Json<OrchestratorSubmitResponse>), OrchestratorError> {
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

    let registry = WorkerRegistry::new();

    // hard-coded port for now
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    let app = Router::new()
        .route("/register_worker", post(register_worker))
        .route("/update_queue", post(update_queue))
        .route("/request_worker", post(request_worker))
        .layer(Extension(registry)); // inject registry

    let server = axum::serve(listener, app);
    let graceful = server.with_graceful_shutdown(async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }
}
