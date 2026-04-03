use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status, Response, Streaming};

use shared::{CreditUpdate, OrchestratorMessage, RegistrationAck, WorkerMessage, WorkerResponse, orchestrator_message, worker_message};
use shared::worker_api_server::WorkerApi;

use crate::job_queue::JobQueue;
use crate::orchestrator::Orchestrator;
use crate::registry::WorkerRegistry;

/// Implementation of the WorkerApi service for Orchestrator.
#[tonic::async_trait]
impl WorkerApi for Orchestrator {
    type ConnectWorkerStream = ReceiverStream<Result<OrchestratorMessage, Status>>;

    /// A function exposed by the Orchestrator that Worker instances call to connect
    /// to this Orchestrator.
    async fn connect_worker(
        &self,
        request: Request<Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::ConnectWorkerStream>, Status> {

        // Extract inbound stream and create outbound channel
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(32);

        // Spawn a task to handle the bidirectional communication
        let orchestrator = self.clone(); // Clone for move into async task
        tokio::spawn(async move {

            // Expect a registration as the first message
            let worker_address = match inbound.next().await {
                Some(Ok(WorkerMessage { message: Some(worker_message::Message::Registration(registration)) })) => {
                    orchestrator.handle_worker_registration(tx.clone(), &registration).await;
                    registration.address
                },
                _ => {
                    eprintln!("Worker connected without sending registration first");
                    return;
                }
            };

            while let Some(result) = inbound.next().await {
                match result {
                    Ok(worker_msg) => {
                        // Handle different message types
                        match worker_msg.message {
                            Some(worker_message::Message::CreditUpdate(credit_update)) => {
                                orchestrator.handle_credit_update(&worker_address, credit_update).await;
                            },
                            Some(worker_message::Message::Registration(_)) => {
                                eprintln!("Worker {} sent a duplicate registration", worker_address);
                            },
                            None => {
                                eprintln!("Received empty message from worker");
                            }
                        }

                    },
                    Err(e) => {
                        eprintln!("Error receiving message from worker: {:?}", e);
                        break;
                    }
                }
            }
            println!("Worker disconnected");
            orchestrator.registry.lock().await.deregister_worker(&worker_address);
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}


// Helper methods for the WorkerApi implementation.
// They may use the outbound tx channel to send messages back to the worker.
type OutboundTx = mpsc::Sender<Result<OrchestratorMessage, Status>>;
impl Orchestrator {
    /// Handles an incoming Worker registration message.
    async fn handle_worker_registration(&self, tx: OutboundTx, registration: &shared::WorkerRegistration) {
        println!("Handling worker registration: {:?}", registration);
        {
            let mut queue = self.job_queue.lock().await;
            let mut registry = self.registry.lock().await;

            registry.register_worker(registration.address.to_owned(), registration.credits);
            Self::dispatch_pending_jobs(&mut queue, &mut registry);
        }

        // Send registration ack back to worker
        let ack = OrchestratorMessage {
            message: Some(orchestrator_message::Message::RegistrationAck(
                RegistrationAck {}
            ))
        };
        if tx.send(Ok(ack)).await.is_err() {
            eprintln!("Failed to send registration ack to worker");
        } else {
            println!("Worker registered, registration ack sent to worker");
        }
    }

    /// Handles a credit update from a Worker, updating its available credits in the registry
    /// and dispatching any pending jobs that can now be served.
    async fn handle_credit_update(&self, worker_address: &str, credit_update: CreditUpdate) {
        let mut queue = self.job_queue.lock().await;
        let mut registry = self.registry.lock().await;

        registry.update_credits(worker_address, credit_update.delta);
        Self::dispatch_pending_jobs(&mut queue, &mut registry);
    }

    /// Dispatches as many pending jobs as possible to available workers, consuming one registry
    /// credit per job. Stops when the queue is empty or no credits remain.
    /// The caller must hold write guards on both the queue and registry for the duration.
    pub fn dispatch_pending_jobs(queue: &mut JobQueue, registry: &mut WorkerRegistry) {
        while registry.has_available_credits() {
            match queue.dequeue() {
                Some((_job_id, tx)) => {
                    let worker_address = registry.get_worker()
                        .expect("registry credit availability checked by has_available_credits()");
                    tx.send(WorkerResponse { worker_address }).ok();
                },
                None => break
            }
        }
    }
}

// TODO: add simple docs
pub fn check_worker_auth(orchestrator: Orchestrator) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    let password = orchestrator.worker_password.clone();
    move |req: Request<()>| {
        if let Some(expected) = &password {
            let actual = req.metadata().get("authorization")
                .and_then(|v| v.to_str().ok());
            if actual != Some(expected) {
                return Err(Status::unauthenticated("invalid worker password"));
            }
        }
        Ok(req)
    }
}