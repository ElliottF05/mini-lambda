use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status, Response, Streaming};

use shared::{OrchestratorMessage, RegistrationAck, WorkerMessage, orchestrator_message, worker_message};
use shared::worker_api_server::WorkerApi;

use crate::orchestrator::Orchestrator;

#[tonic::async_trait]
impl WorkerApi for Orchestrator {
    type ConnectWorkerStream = ReceiverStream<Result<OrchestratorMessage, Status>>;

    async fn connect_worker(
        &self,
        request: Request<Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::ConnectWorkerStream>, Status> {

        // extract inbound stream and create outbound channel
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(32);

        // spawn a task to handle the bidirectional communication
        let orchestrator = self.clone(); // clone for move into async task
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(worker_msg) => {
                        // clone for move into async task
                        let orchestrator = orchestrator.clone();
                        let tx = tx.clone();

                        // handle different message types
                        match worker_msg.message {
                            Some(worker_message::Message::Registration(registration)) => {
                                tokio::spawn(async move {
                                    orchestrator.handle_worker_registration(tx, registration).await;
                                });
                            },
                            None => {
                                println!("Received empty message from worker");
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
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}


// Helper methods for the WorkerApi implementation.
// They all use the outbound tx channel to send messages back to the worker.
type OutboundTx = mpsc::Sender<Result<OrchestratorMessage, Status>>;
impl Orchestrator {
    async fn handle_worker_registration(&self, tx: OutboundTx, registration: shared::WorkerRegistration) {
        println!("Handling worker registration: {:?}", registration);

        self.registry.write().await.register_worker(registration.address);

        // send registration ack back to worker
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
}