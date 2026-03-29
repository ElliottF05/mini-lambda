use tokio::sync::oneshot;
use tonic::{Request, Status, Response};

use shared::client_api_server::ClientApi;
use shared::{WorkerRequest, WorkerResponse};
use uuid::Uuid;

use crate::job_queue::PendingJob;
use crate::orchestrator::Orchestrator;

/// Implementation of the CliApi service for the Orchestrator.
#[tonic::async_trait]
impl ClientApi for Orchestrator {

    /// A function exposed by the Orchestrator for the client to call to request
    /// a worker be assigned to them to execute their job.
    async fn request_worker(
        &self, 
        _request: Request<WorkerRequest>
    ) -> Result<Response<WorkerResponse>, Status> {

        // let worker_address = self.registry.write().await.get_worker();
        // match worker_address {
        //     Some(worker_address) => {
        //         let response = WorkerResponse { worker_address: worker_address.clone() };
        //         println!("Assigning worker at {} to the client", worker_address);
        //         return Ok(Response::new(response));
        //     },
        //     None => {
        //         let (tx, rx) = oneshot::channel();
        //         let job_id = Uuid::new_v4();
        //         let job = PendingJob { id: job_id.clone(), tx };
        //         println!("No workers currently available, enqueing job with id {}", job.id);
        //         self.job_queue.write().await.enqueue(job);

        //         match rx.await {
        //             Ok(response) => {
        //                 println!("Worker at {} became available, dequeueing job with id {}", response.worker_address, job_id);
        //                 Ok(Response::new(response))
        //             },
        //             Err(_) => Err(Status::cancelled("job cancelled before worker assigned"))
        //         }
        //     }
        // }

        // Create the pending job
        let job_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        let job = PendingJob { id: job_id, tx };

        // Add this job to the queue and dispatch pending jobs atomically
        {
            let mut queue = self.job_queue.write().await;
            let mut registry = self.registry.write().await;

            queue.enqueue(job);
            Self::dispatch_pending_jobs(&mut queue, &mut registry);
        }

        // Awake when job is dispatched
        match rx.await {
            Ok(response) => {
                println!("Worker at {} became available, dequeueing job with id {}", response.worker_address, job_id);
                Ok(Response::new(response))
            },
            Err(_) => Err(Status::cancelled("job cancelled before worker assigned"))
        }
    }
}