/// Enum for all recoverable errors that can occur in the Executor.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("wasm compilation failed: {0}")]
    CompilationFailed(#[from] wasmer::CompileError),

    #[error("wasm execution failed: {0}")]
    ExecutionFailed(String),

    #[error("failed to capture job output: {0}")]
    OutputCaptureFailed(String),

    #[error("executor task panicked: {0}")]
    WorkerPanicked(#[from] tokio::task::JoinError)
}

impl From<ExecutorError> for tonic::Status {
    fn from(value: ExecutorError) -> Self {
        match value {
            ExecutorError::CompilationFailed(e) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::ExecutionFailed(e) => tonic::Status::invalid_argument(e.to_string()),
            ExecutorError::OutputCaptureFailed(e) => tonic::Status::internal(e.to_string()),
            ExecutorError::WorkerPanicked(e) => tonic::Status::internal(e.to_string()),
        }
    }
}