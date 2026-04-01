/// Enum for all recoverable errors that can occur in the Executor.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("wasm compilation failed: {0}")]
    CompilationFailed(wasmtime::Error),

    #[error("wasm is not a valid wasi command component: {0}")]
    InstantiationFailed(wasmtime::Error),

    #[error("wasm execution failed: {0}")]
    ExecutionFailed(String),

    #[error("invalid job id: {0}")]
    InvalidJobId(#[from] uuid::Error),

    #[error("unknown error: {0}")]
    Unknown(String),
}

impl From<ExecutorError> for tonic::Status {
    fn from(e: ExecutorError) -> Self {
        match e {
            ExecutorError::CompilationFailed(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::InstantiationFailed(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::ExecutionFailed(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::InvalidJobId(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::Unknown(_) => tonic::Status::unknown(e.to_string()),
        }
    }
}