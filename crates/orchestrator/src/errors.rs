use axum::{response::IntoResponse, Json};
use reqwest::StatusCode;
use thiserror::Error;

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