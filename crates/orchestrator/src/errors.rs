use axum::{response::IntoResponse, Json};
use reqwest::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("no workers registered")]
    NoWorkers,

    #[error("worker not found")]
    WorkerNotFound,

    #[error("job queue is full")]
    QueueFull,

    #[error("internal server error")]
    Internal,

    #[error("request timed out")]
    RequestTimeout,
}

impl IntoResponse for OrchestratorError {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match &self {
            OrchestratorError::NoWorkers => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            OrchestratorError::WorkerNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            OrchestratorError::QueueFull => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            OrchestratorError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            OrchestratorError::RequestTimeout => (StatusCode::REQUEST_TIMEOUT, self.to_string()),
        };
        (status, Json(serde_json::json!({ "error": body }))).into_response()
    }
}