use std::time::Duration;

use shared::{CancelJobRequest, JobRequest};
use shared::executor_client::ExecutorClient;
use shared::{WorkerRequest, client_api_client::ClientApiClient};
use tokio::sync::watch;
use tonic::transport::Channel;
use tonic::{Code, Request, Status};
use uuid::Uuid;

use crate::job::{Job, JobError, JobOutput, JobState, RunningJob};

#[derive(Clone)]
pub struct Client {
    orchestrator_client: ClientApiClient<Channel>,
}

impl Client {
    pub async fn connect(orchestrator_endpoint: &str) -> Result<Client, tonic::transport::Error> {
        let orchestrator_client = ClientApiClient::connect(orchestrator_endpoint.to_string()).await?;
        Ok(Client { orchestrator_client })
    }

    pub fn submit_job(&self, job: Job) -> RunningJob {
        let (state_tx, state_rx) = watch::channel(JobState::Queued);
        let job_id = Uuid::new_v4();

        let mut client = self.clone();
        let state_tx1 = state_tx.clone();

        let task = tokio::task::spawn(async move {
            let state_tx2 = state_tx1.clone();
            let future = async move {
                let job_id = job_id.as_bytes().to_vec();
                let worker_request = WorkerRequest { job_id: job_id.clone() };

                let response = match client.orchestrator_client.request_worker(Request::new(worker_request)).await {
                    Ok(r) => r,
                    Err(e) => {
                        state_tx2.send(JobState::Completed(Err(JobError::Internal(e.to_string())))).ok();
                        return;
                    }
                };

                let worker_address = response.into_inner().worker_address; 
                let worker_endpoint = "http://".to_string() + &worker_address;
                if state_tx2.send(JobState::Executing { worker_address }).is_err() {
                    return; // no listening RunningJob's, so no point running the task
                };

                let mut executor_client = match ExecutorClient::connect(worker_endpoint).await {
                    Ok(c) => c,
                    Err(e) => {
                        state_tx2.send(JobState::Completed(Err(JobError::Internal(e.to_string())))).ok();
                        return;
                    }
                };

                let job_request = JobRequest { job_id, wasm_bytes: job.wasm_bytes, args: job.args };

                let execution_result = executor_client.execute_job(Request::new(job_request)).await;
                match execution_result {
                    Ok(job_response) => {
                        let job_response = job_response.into_inner();
                        let stdout = job_response.stdout;
                        let stderr = job_response.stderr;
                        let job_output = JobOutput { stdout, stderr };
                        state_tx2.send(JobState::Completed(Ok(job_output))).ok();
                    },
                    Err(e) => {
                        let message = e.message().to_string();
                        let job_error = match e.code() {
                            Code::InvalidArgument => JobError::WasmError(message),
                            code => JobError::Internal(format!("error code: {}, message: {}", code, message))
                        };
                        state_tx2.send(JobState::Completed(Err(job_error))).ok();
                    }
                }
            };

            let timeout = job.timeout.unwrap_or(Duration::MAX);
            match tokio::time::timeout(timeout, future).await {
                Ok(_) => {},
                Err(_) => {
                    state_tx1.send(JobState::Completed(Err(JobError::TimedOut))).ok();
                }
            }
        });

        let abort_handle = task.abort_handle();

        return RunningJob {
            job_id,
            state_rx,
            state_tx,
            abort: abort_handle,
            client: self.clone()
        }
    }

    pub async fn cancel_queued_job(&self, job_id: Uuid) -> Result<(), Status> {
        let cancellation_result = self.orchestrator_client.clone()
            .cancel_job(CancelJobRequest { 
                job_id: job_id.as_bytes().to_vec()
            }).await;
        match cancellation_result {
            Ok(_) => Ok(()),
            Err(e) => Err(e)
        }
    }

    pub async fn cancel_running_job(&self, job_id: Uuid, worker_address: &str) -> Result<(), Status> {
        let worker_endpoint = format!("http://{}", worker_address);
        let mut executor_client = ExecutorClient::connect(worker_endpoint).await
            .unwrap_or_else(|e| panic!("Failed to connect to the provided Worker: {}", e));
        
        let cancellation_result = executor_client.cancel_job(CancelJobRequest {
            job_id: job_id.as_bytes().to_vec()
        }).await;

        match cancellation_result {
            Ok(_) => Ok(()),
            Err(e) => Err(e)
        }
    }
}