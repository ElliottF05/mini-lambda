use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Status, Response, Streaming};

use shared::cli_api_server::{CliApi, CliApiServer};
use shared::{OrchestratorMessage, RegistrationAck, WorkerMessage, WorkerRequest, WorkerResponse, orchestrator_message, worker_message};
use shared::worker_api_server::{WorkerApi, WorkerApiServer};

#[derive(Debug, Default, Clone)]
struct Orchestrator {
    // note: all shared state fields should use Arc<RwLock<...>> for thread safety
}

#[tonic::async_trait]
impl CliApi for Orchestrator {

    async fn request_worker(
        &self, 
        _request: Request<WorkerRequest>
    ) -> Result<Response<WorkerResponse>, Status> {

        let response = WorkerResponse {
            worker_address: "worker-1-address".to_string(),
        };

        return Ok(Response::new(response));
    }
}

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
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(worker_msg) => {
                        match worker_msg.message {
                            Some(worker_message::Message::Registration(registration)) => {
                                println!("Worker registered: {:?}", registration);

                                // send registration ack back to worker
                                let ack = OrchestratorMessage {
                                    message: Some(orchestrator_message::Message::RegistrationAck(
                                        RegistrationAck {}
                                    ))
                                };

                                if tx.send(Ok(ack)).await.is_err() {
                                    eprintln!("Failed to send registration ack to worker");
                                    break;
                                }
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

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse()?;
    let orchestrator = Orchestrator::default();
    
    Server::builder()
        .add_service(CliApiServer::new(orchestrator.clone()))
        .add_service(WorkerApiServer::new(orchestrator.clone()))
        .serve(addr)
        .await?;

    Ok(())
}