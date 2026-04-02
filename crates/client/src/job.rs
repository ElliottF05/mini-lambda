use std::{ops::Deref, path::Path, time::Duration};
use std::fmt::Display;

use shared::{JobResponse, WorkerResponse};
use tokio::{sync::watch, task::AbortHandle};
use tonic::{Response, Status};
use uuid::Uuid;

use crate::{client::Client, errors::JobError};

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
}

// TODO: check if this is safely cloneable (i think all inner methods should be fine)
#[derive(Clone)]
pub struct RunningJob {
    pub(crate) job_id: Uuid,
    pub(crate) state: watch::Receiver<JobState>,
    pub(crate) abort: AbortHandle,
    pub(crate) client: Client,
}

impl RunningJob {
    pub async fn wait(mut self) -> Result<JobOutput, JobError> {
        let state = self.state
            .wait_for(|s| matches!(s, JobState::Completed(_))).await;
        
        match state {
            Ok(job_state) => {
                match job_state.deref() {
                    JobState::Completed(result) => result.clone(),
                    _ => unreachable!("wait_for should guarantee state is Completed when it returns"),
                }
            },
            Err(e) => {
                Err(JobError::Internal(e.to_string()))
            }
        }
    }

    pub async fn cancel(mut self) -> bool {
        self.abort.abort();
        match self.state.borrow().deref() {
            JobState::Queued => {
                self.client.cancel_queued_job(self.job_id).await.is_ok()
            },
            JobState::Executing { worker_address } => {
                self.client.cancel_running_job(self.job_id, worker_address).await.is_ok()
            },
            JobState::Completed(_) => true
        }
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