use axum::{response::IntoResponse, Json};
use reqwest::StatusCode;
use thiserror::Error;

/// Errors that can occur in the worker.
#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("WASM compilation error: {0}")]
    Compile(String),

    #[error("WASM execution error during runtime: {0}")]
    Execution(String),

    #[error("module not found in cache: {0}")]
    ModuleNotFound(String),

    #[error("WASM I/O error, failed to read captured stdout: {0}")]
    Io(#[from] std::io::Error),

    #[error("WASM thread failed to join (wasm thread panicked): {0}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("failed to register with orchestrator: {0}")]
    Registration(String),

    #[error("failed to unregister from orchestrator: {0}")]
    Unregistration(String),

    #[error("failed to send heartbeat: {0}")]
    Heartbeat(String),
}

impl WorkerError {
    pub fn to_http_response(&self) -> (StatusCode, String) {
        match self {
            WorkerError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            WorkerError::Compile(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            WorkerError::Execution(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            WorkerError::ModuleNotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            WorkerError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal I/O error".to_string()),
            WorkerError::JoinError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "wasm thread panicked".to_string()),
            WorkerError::Registration(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            WorkerError::Unregistration(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            WorkerError::Heartbeat(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        }
    }
}

impl IntoResponse for WorkerError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = self.to_http_response();
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}