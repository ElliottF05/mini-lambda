use tonic::{Code, Status};

/// Enum for all recoverable errors that can occur in the Executor.
#[derive(Debug, thiserror::Error, Clone)]
pub enum JobError {
    #[error("the submitted wasm contained an error when compiled or when run: {0}")]
    WasmError(String),

    #[error("job cancelled by client")]
    JobCancelled,

    #[error("intenral error: {0}")]
    Internal(String),
}

impl From<Status> for JobError {
    fn from(status: Status) -> Self {
        let message = status.message().to_string();
        match status.code() {
            Code::InvalidArgument => JobError::WasmError(message),
            Code::Cancelled => JobError::JobCancelled,
            _ => JobError::Internal(format!("code: {}, message: {}", status.code(), message))
        }
    }
}