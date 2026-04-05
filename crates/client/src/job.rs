use std::{ops::Deref, path::Path, time::Duration};
use std::fmt::Display;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::{Code, Status};

/// A wasm job to be submitted for remote execution.
/// Construct with from_bytes or from_path, then configure using the builder methods.
/// Note: the wasm binary must target the wasm32-wasip2 compilation target.
/// See the [WASI Preview 2 overview](https://github.com/WebAssembly/WASI/blob/main/preview2/README.md) for more information
pub struct Job {
    pub(crate) wasm_bytes: Vec<u8>,
    pub(crate) args: Vec<String>,
    pub(crate) timeout: Option<Duration>,
}

impl Job {
    /// Create a job from raw wasm bytes.
    pub fn from_bytes(wasm_bytes: Vec<u8>) -> Self {
        Self { 
            wasm_bytes,
            args: vec![],
            timeout: None
        }
    }
    /// Create a job by reading a wasm file from the given path.
    pub fn from_path(wasm_path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        match std::fs::read(wasm_path) {
            Ok(wasm_bytes) => Ok(Self::from_bytes(wasm_bytes)),
            Err(e) => Err(e)
        }
    }
    /// Add a single command-line argument to pass to the wasm program.
    pub fn arg(mut self, arg: impl AsRef<str>) -> Self {
        self.args.push(arg.as_ref().to_string());
        self
    }
    /// Add multiple command-line arguments to pass to the wasm program.
    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        args.into_iter().for_each(|arg| self.args.push(arg.as_ref().to_string()));
        self
    }
    /// Set a maximum duration for the job. The job will fail with JobError::TimedOut if exceeded.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
}

pub enum JobState {
    Queued,
    Executing,
    Completed(Result<JobOutput, JobError>),
    Cancelled,
}

/// A handle to a submitted job. Can be cloned to share across tasks.
/// Use wait to block until the job finishes, or cancel to stop it early.
#[derive(Clone)]
pub struct RunningJob {
    pub(crate) state_rx: watch::Receiver<JobState>,
    pub(crate) cancel_token: CancellationToken,
}

impl RunningJob {
    /// Wait for the job to finish and return its output.
    /// Returns an error if the job failed, timed out, or was cancelled.
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

    /// Cancel the job, stopping it at whichever stage it is currently in.
    /// If queued, removes it from the orchestrator. If running, stops execution on the worker.
    pub async fn cancel(self) {
        self.cancel_token.cancel();
    }
}

/// The captured output of a successfully completed job.
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

/// All the ways a submitted job can fail.
#[derive(Debug, thiserror::Error, Clone)]
pub enum JobError {
    /// The wasm module failed to compile or produced a runtime error. Caused by bad user input.
    #[error("the submitted wasm contained an error when compiled or when run: {0}")]
    WasmError(String), // bad wasm input from user

    /// An unexpected system error occurred, not caused by user input.
    #[error("internal error: {0}")]
    Internal(String), // unexpected internal system error

    /// The job exceeded its configured timeout duration.
    #[error("job timed out")]
    TimedOut, // exceeded user-configured timeout

    /// The job was explicitly cancelled by the caller.
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