use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status, Response, Streaming};

use shared::{CreditUpdate, OrchestratorMessage, RegistrationAck, WorkerMessage, WorkerResponse, orchestrator_message, worker_message};
use shared::worker_api_server::WorkerApi;

use crate::orchestrator::Orchestrator;

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
            orchestrator.registry.write().await.deregister_worker(&worker_address);
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

        self.registry.write().await.register_worker(registration.address.clone(), 0);

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

    async fn handle_credit_update(&self, worker_address: &str, credit_update: CreditUpdate) {
        let mut queue = self.job_queue.write().await;
        let mut registry = self.registry.write().await;

        let mut remaining_credits = credit_update.credits;
        while remaining_credits > 0 {
            match queue.dequeue() {
                Some(job) => {
                    let worker_response = WorkerResponse { worker_address: worker_address.to_string() };
                    if job.tx.send(worker_response).is_ok() {
                        remaining_credits -= 1;
                    }
                },
                None => break
            }
        }

        registry.update_credits(worker_address, remaining_credits);
    }
}