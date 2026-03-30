/// Enum for all recoverable errors that can occur with the ClientApi
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("invalid job id: {0}")]
    InvalidJobId(#[from] uuid::Error),

    #[error("job cancelled before worker assigned")]
    JobCancelled,
}

impl From<ClientError> for tonic::Status {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::InvalidJobId(_) => tonic::Status::invalid_argument(e.to_string()),
            ClientError::JobCancelled => tonic::Status::cancelled(e.to_string()),
        }
    }
}