use shared::{CancelJobRequest, JobRequest};
use shared::executor_client::ExecutorClient;
use shared::{WorkerRequest, client_api_client::ClientApiClient};
use tokio::sync::watch;
use tonic::transport::Channel;
use tonic::{Code, Request, Status};
use uuid::Uuid;

use crate::errors::JobError;
use crate::job::{Job, JobOutput, JobState, RunningJob};

#[derive(Clone)]
pub struct Client {
    orchestrator_client: ClientApiClient<Channel>,
}

impl Client {
    pub async fn connect(orchestrator_endpoint: &str) -> Client {
        let orchestrator_client = ClientApiClient::connect(orchestrator_endpoint.to_string()).await
            .unwrap_or_else(|e| panic!("Failed to connect to the Orchestrator: {}", e));

        Client {
            orchestrator_client
        }
    }

    pub fn submit_job(&mut self, job: Job) -> RunningJob {
        let (state_tx, state_rx) = watch::channel(JobState::Queued);
        let job_id = Uuid::new_v4();

        let mut client = self.clone();
        let task = tokio::task::spawn(async move {
            let job_id = job_id.as_bytes().to_vec();
            let worker_request = WorkerRequest { job_id: job_id.clone() };

            // TODO: handle this error better
            let response = client.orchestrator_client.request_worker(Request::new(worker_request)).await
                .unwrap_or_else(|e| panic!("Failed to request a worker from the Orchestrator: {}", e));

            let worker_address = response.into_inner().worker_address; 
            let worker_endpoint = "http://".to_string() + &worker_address;

            // TODO: cehck if i should store executorclient itself
            // TODO: handle error here?
            state_tx.send(JobState::Executing { worker_address });

            // TODO: handle this error better
            let mut executor_client = ExecutorClient::connect(worker_endpoint).await
                .unwrap_or_else(|e| panic!("Failed to connect to the provided Worker: {}", e));
            let job_request = JobRequest { job_id, wasm_bytes: job.wasm_bytes, args: job.args };

            let execution_result = executor_client.execute_job(Request::new(job_request)).await;
            match execution_result {
                Ok(job_response) => {
                    let job_response = job_response.into_inner();
                    let stdout = job_response.stdout;
                    let stderr = job_response.stderr;
                    let job_output = JobOutput { stdout, stderr };
                    state_tx.send(JobState::Completed(Ok(job_output))); // TODO: handle error here?
                },
                Err(e) => {
                    let message = e.message().to_string();
                    let job_error = match e.code() {
                        Code::InvalidArgument => JobError::WasmError(message),
                        Code::Cancelled => JobError::JobCancelled,
                        code => JobError::Internal(format!("error code: {}, message: {}", code, message))
                    };
                    state_tx.send(JobState::Completed(Err(job_error))); // TODO: handle error here?
                }
            }
        });

        let abort_handle = task.abort_handle();

        return RunningJob {
            job_id,
            state: state_rx,
            abort: abort_handle,
            client: self.clone()
        }
    }

    pub async fn cancel_queued_job(&mut self, job_id: Uuid) -> Result<(), Status> {
        let cancellation_result = self.orchestrator_client.cancel_job(CancelJobRequest { 
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