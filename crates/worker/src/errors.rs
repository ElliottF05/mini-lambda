/// Enum for all recoverable errors that can occur in the Executor.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("wasm compilation failed: {0}")]
    CompilationFailed(wasmtime::Error),

    #[error("wasm is not a valid wasi command component: {0}")]
    InstantiationFailed(wasmtime::Error),

    #[error("wasm execution failed: {0}")]
    ExecutionFailed(String),

    #[error("malformed job id, job failed")] 
    MalformedJobId, // note that a malformed job id in cancel_job just returns a JobNotFound error,
    // since this makes more logical sense

    #[error("job not found")]
    JobNotFound,

    #[error("job cancelled by client")]
    JobCancelled,

    #[error("received unathenticated jwt token")]
    Unauthenticated,

    #[error("worker was accessed before it was ready: {0}")]
    NotReady(String),

    #[error("an unknown error occurred: {0}")]
    Unknown(String)
}

impl From<ExecutorError> for tonic::Status {
    fn from(e: ExecutorError) -> Self {
        match e {
            ExecutorError::CompilationFailed(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::InstantiationFailed(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::ExecutionFailed(_) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::MalformedJobId => tonic::Status::internal(e.to_string()),
            ExecutorError::JobNotFound => tonic::Status::not_found(e.to_string()),
            ExecutorError::JobCancelled => tonic::Status::cancelled(e.to_string()),
            ExecutorError::Unauthenticated => tonic::Status::unauthenticated(e.to_string()),
            ExecutorError::NotReady(_) => tonic::Status::internal(e.to_string()),
            ExecutorError::Unknown(_) => tonic::Status::unknown(e.to_string()),
        }
    }
}