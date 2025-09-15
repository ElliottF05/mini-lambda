use axum::{extract::Extension, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::error;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use uuid::Uuid;

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


#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkerInfo {
    endpoint: String,
    queue_len: usize,
}

type Registry = Arc<Mutex<HashMap<Uuid, WorkerInfo>>>;

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
    Extension(registry): Extension<Registry>,
    Json(req): Json<RegisterWorkerRequest>,
) -> (StatusCode, Json<RegisterWorkerResponse>) {
    // (StatusCode, Json<RegisterWorkerResponse>)
    let id = Uuid::new_v4();
    let info = WorkerInfo {
        endpoint: req.endpoint,
        queue_len: 0,
    };

    registry.lock().await.insert(id, info);

    (
        StatusCode::CREATED,
        Json(RegisterWorkerResponse { worker_id: id }),
    )
}

async fn update_queue(
    Extension(registry): Extension<Registry>,
    Json(req): Json<UpdateQueueRequest>,
) -> Result<StatusCode, OrchestratorError> {
    let mut reg = registry.lock().await;
    if let Some(w) = reg.get_mut(&req.worker_id) {
        w.queue_len = req.queue_len;
        return Ok(StatusCode::OK);
    }

    Err(OrchestratorError::WorkerNotFound)
}

async fn request_worker(
    Extension(registry): Extension<Registry>,
) -> Result<(StatusCode, Json<OrchestratorSubmitResponse>), OrchestratorError> {
    let mut reg = registry.lock().await;
    if reg.is_empty() {
        return Err(OrchestratorError::NoWorkers);
    }

    // choose worker with smallest queue_len
    let mut chosen_id: Option<Uuid> = None;
    let mut smallest: Option<usize> = None;
    for (id, info) in reg.iter() {
        if smallest.is_none() || info.queue_len < smallest.unwrap() {
            smallest = Some(info.queue_len);
            chosen_id = Some(*id);
        }
    }

    let chosen_id = chosen_id.expect("non-empty checked");
    let endpoint = reg.get_mut(&chosen_id).unwrap().endpoint.clone();
    // increment approximate queue length
    reg.get_mut(&chosen_id).unwrap().queue_len += 1;

    let resp = OrchestratorSubmitResponse {
        job_id: Uuid::new_v4(),
        worker_endpoint: endpoint,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));

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
