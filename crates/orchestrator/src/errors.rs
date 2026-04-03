/// Enum for all recoverable errors that can occur within the Orchestrator.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("job cancelled before worker assigned")]
    JobCancelled,

    #[error("tried to cancel a job which couldn't be found or was already cancelled")]
    JobNotFound,

    #[error("received malformed job id: {0}")]
    MalformedJobId(String),
}

impl From<OrchestratorError> for tonic::Status {
    fn from(e: OrchestratorError) -> Self {
        match e {
            OrchestratorError::JobCancelled => tonic::Status::cancelled(e.to_string()),
            OrchestratorError::JobNotFound => tonic::Status::not_found(e.to_string()),
            OrchestratorError::MalformedJobId(_) => tonic::Status::invalid_argument(e.to_string()),
        }
    }
}