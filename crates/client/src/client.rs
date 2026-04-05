use std::time::Duration;

use shared::{CancelJobRequest, JobRequest};
use shared::executor_client::ExecutorClient;
use shared::{WorkerRequest, client_api_client::ClientApiClient};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::service::Interceptor;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic::{Request, Status};
use uuid::Uuid;

use crate::job::{Job, JobError, JobOutput, JobState, RunningJob};

// Note for error handling in this crate. Since this is meant to be a library, avoid panics
// and exiting the process. Instead, return internal error status codes with descriptive messages,
// indicating if a program invariant was violated.

/// The main entry point for submitting jobs to the distributed compute platform.
/// Connects to an Orchestrator which assigns workers to run your wasm jobs.
#[derive(Clone)]
pub struct Client {
    orchestrator_client: ClientApiClient<InterceptedService<Channel, OrchestratorAuthInterceptor>>,
}

impl Client {
    /// Connect to an Orchestrator at the given endpoint, returning a Client on success.
    pub async fn connect(orchestrator_endpoint: &str, password: Option<String>) -> Result<Client, ClientError> {
        let channel = Channel::from_shared(orchestrator_endpoint.to_string())
            .map_err(|e| ClientError::InvalidEndpoint(e.to_string()))?
            .connect().await?;
        let orchestrator_client = ClientApiClient::with_interceptor(channel, OrchestratorAuthInterceptor { password });
        Ok(Client { orchestrator_client })
    }

    /// Submit a job for execution and return a RunningJob handle immediately.
    /// The job is queued until a worker becomes available, then executed automatically.
    pub fn submit_job(&self, job: Job) -> RunningJob {
        let job_id = Uuid::new_v4();
        let (state_tx, state_rx) = watch::channel(JobState::Queued);
        let cancel_token = CancellationToken::new();

        let state_tx_timeout = state_tx.clone();
        let cancel_token_handle = cancel_token.clone();
        let cancel_token_timeout = cancel_token.clone();
        let mut client = self.clone();

        tokio::spawn(async move {
            let mut submit_task = tokio::spawn(async move {
                let job_id_bytes = job_id.as_bytes().to_vec();
                let worker_request = Request::new(WorkerRequest { job_id: job_id_bytes.clone() });

                let response = tokio::select! {
                    result = client.orchestrator_client.request_worker(worker_request) => {
                        match result {
                            Ok(r) => r.into_inner(),
                            Err(e) => {
                                state_tx.send(JobState::Completed(Err(JobError::Internal(e.to_string())))).ok();
                                return;
                            }
                        }
                    },
                    _ = cancel_token.cancelled() => {
                        client.cancel_queued_job(job_id).await;
                        state_tx.send(JobState::Cancelled).ok();
                        return;
                    }
                };

                let worker_address = response.worker_address; 
                let worker_endpoint = "http://".to_string() + &worker_address;
                let jwt_token = response.jwt_token;
                if state_tx.send(JobState::Executing).is_err() {
                    return; // no listening RunningJob's, so no point running the task
                };
                
                let channel = match Channel::from_shared(worker_endpoint.to_string()) {
                    Ok(endpoint) => {
                        match endpoint.connect().await {
                            Ok(c) => c,
                            Err(e) => {
                                state_tx.send(JobState::Completed(Err(JobError::Internal(e.to_string())))).ok();
                                return;
                            }
                        }
                    },
                    Err(e) => {
                        state_tx.send(JobState::Completed(Err(JobError::Internal(
                            format!("received a malformed worker endpoint from the orchestrator, this should never occur: {}", e)
                        )))).ok();
                        return;
                    }
                };
                let mut executor_client = ExecutorClient::with_interceptor(channel, WorkerJwtInterceptor { jwt_token });

                let job_request = Request::new(JobRequest { 
                    job_id: job_id_bytes, 
                    wasm_bytes: job.wasm_bytes, 
                    args: job.args 
                });

                let execution_result = tokio::select! {
                    r = executor_client.execute_job(job_request) => r,
                    _ = cancel_token.cancelled() => {
                        client.cancel_running_job(job_id, executor_client).await;
                        state_tx.send(JobState::Cancelled).ok();
                        return;
                    }
                };
                match execution_result {
                    Ok(job_response) => {
                        let job_response = job_response.into_inner();
                        let stdout = job_response.stdout;
                        let stderr = job_response.stderr;
                        let job_output = JobOutput { stdout, stderr };
                        state_tx.send(JobState::Completed(Ok(job_output))).ok();
                    },
                    Err(e) => {
                        state_tx.send(JobState::Completed(Err(JobError::from(e)))).ok();
                    }
                }
            });

            let timeout = job.timeout.unwrap_or(Duration::MAX);
            tokio::select! {
                _ = &mut submit_task => {},
                _ = tokio::time::sleep(timeout) => {
                    cancel_token_timeout.cancel();
                    submit_task.await.ok(); // wait for submit_task cleanup
                    state_tx_timeout.send(JobState::Cancelled).ok();
                }
            };
        });

        return RunningJob {
            state_rx,
            cancel_token: cancel_token_handle,
        }
    }
    
    // TODO: maybe add debug logs (so they only show up if tracing level is very high)
    // if cancel_request return an error? Note that this doesn't mean something went wrong,
    // it's just the result of a network race. The job will be cancelled on the client side
    // by the cancellation token
    /// Send a cancellation request to the Orchestrator to remove a queued job.
    pub(crate) async fn cancel_queued_job(&self, job_id: Uuid) {
        self.orchestrator_client.clone()
            .cancel_job(CancelJobRequest { 
                job_id: job_id.as_bytes().to_vec()
            }).await.ok();
    }

    /// Send a cancellation request to the worker currently executing a job.
    pub(crate) async fn cancel_running_job(&self, job_id: Uuid, mut executor_client: ExecutorClient<InterceptedService<Channel, WorkerJwtInterceptor>>) {
        executor_client.cancel_job(CancelJobRequest {
            job_id: job_id.as_bytes().to_vec()
        }).await.ok();
    }
}


/// Injects the client password into the authorization header of every outbound request
/// to the Orchestrator.
/// No-op if no password is configured.
#[derive(Clone)]
struct OrchestratorAuthInterceptor {
    password: Option<String>
}

impl Interceptor for OrchestratorAuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(pass) = &self.password {
            let val = pass.parse()
                .map_err(|e| Status::invalid_argument(format!("password could not be parsed: {e}")))?;
            request.metadata_mut().insert("authorization", val);
        }
        Ok(request)
    }
}

/// Injects the job's jwt token into the authorization header of every outbound request to the worker.
#[derive(Clone)]
pub(crate) struct WorkerJwtInterceptor {
    jwt_token: String,
}

impl Interceptor for WorkerJwtInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let val = self.jwt_token.parse()
            .map_err(|e| Status::internal(format!("the orchestrator returned a jwt token that couldn't be parsed, this should never occur: {}", e)))?;
        request.metadata_mut().insert("authorization", val);
        Ok(request)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the provided orchestrator endpoint is invalid: {0}")]
    InvalidEndpoint(String),

    #[error("failed to connect to the orchestrator: {0}")]
    ConnectionFailed(#[from] tonic::transport::Error),
}