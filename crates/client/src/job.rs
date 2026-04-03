use std::{ops::Deref, path::Path, time::Duration};
use std::fmt::Display;

use tokio::{sync::watch, task::AbortHandle};
use tonic::{Code, Status};
use uuid::Uuid;

use crate::client::Client;

pub struct Job {
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) args: Vec<String>,
    pub(crate) timeout: Option<Duration>,
}

impl Job {
    pub fn from_bytes(wasm_bytes: Vec<u8>) -> Self {
        Self { 
            wasm_bytes,
            args: vec![],
            timeout: None
        }
    }
    pub fn from_path(wasm_path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        match std::fs::read(wasm_path) {
            Ok(wasm_bytes) => Ok(Self::from_bytes(wasm_bytes)),
            Err(e) => Err(e)
        }
    }
    pub fn arg(mut self, arg: impl AsRef<str>) -> Self {
        self.args.push(arg.as_ref().to_string());
        self
    }
    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        args.into_iter().for_each(|arg| self.args.push(arg.as_ref().to_string()));
        self
    }
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
}

pub enum JobState {
    Queued,
    Executing { worker_address: String },
    Completed(Result<JobOutput, JobError>),
    Cancelled,
}

#[derive(Clone)]
pub struct RunningJob {
    pub(crate) job_id: Uuid,
    pub(crate) state_rx: watch::Receiver<JobState>,
    pub(crate) state_tx: watch::Sender<JobState>,
    pub(crate) abort: AbortHandle,
    pub(crate) client: Client,
}

impl RunningJob {
    pub async fn wait(mut self) -> Result<JobOutput, JobError> {
        let result = self.state_rx
            .wait_for(|s| matches!(s, JobState::Completed(_) | JobState::Cancelled))
            .await;
        
        match result {
            Ok(state) => match state.deref() {
                JobState::Completed(result) => result.clone(),
                JobState::Cancelled => Err(JobError::Cancelled),
                _ => unreachable!("wait_for should guarantee state is Completed or Cancelled when it returns"),
            }
            Err(e) => Err(JobError::Internal(e.to_string())),
        }
    }

    pub async fn cancel(self) {
        self.abort.abort();
        let cancel_target = match self.state_rx.borrow().deref() {
            JobState::Queued => Some(None), // cancel at orchestrator
            JobState::Executing { worker_address } => Some(Some(worker_address.clone())), // cancel at worker
            _ => None // job already finished or cancelled
        };

        match cancel_target {
            Some(None) => {
                if self.client.cancel_queued_job(self.job_id).await.is_err() {
                    eprintln!("failed to cancel job with id: {}", self.job_id);
                }
            },
            Some(Some(worker_address)) => {
                if self.client.cancel_running_job(self.job_id, &worker_address).await.is_err() {
                    eprintln!("failed to cancel job with id: {}", self.job_id);
                }
            },
            None => {}
        };
        
        self.state_tx.send(JobState::Cancelled).ok();
    }
}

#[derive(Clone, Debug)]
pub struct JobOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Display for JobOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.stdout))?;
        if !self.stderr.is_empty() {
            write!(f, "\nstderr:\n{}", String::from_utf8_lossy(&self.stderr))?;
        }
        Ok(())
    }
}

/// Enum for all recoverable errors that can occur in the Executor.
#[derive(Debug, thiserror::Error, Clone)]
pub enum JobError {
    #[error("the submitted wasm contained an error when compiled or when run: {0}")]
    WasmError(String), // bad wasm input from user

    #[error("internal error: {0}")]
    Internal(String), // unexpected internal system error

    #[error("job timed out")]
    TimedOut, // exceeded user-configured timeout

    #[error("job cancelled by user")]
    Cancelled, // job explicitly cancelled by user
}

impl From<Status> for JobError {
    fn from(status: Status) -> Self {
        let message = status.message().to_string();
        match status.code() {
            Code::InvalidArgument => JobError::WasmError(message),
            Code::Cancelled => JobError::Cancelled,
            _ => JobError::Internal(format!("code: {}, message: {}", status.code(), message))
        }
    }
}