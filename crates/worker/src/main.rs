use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::Server;
use tonic::{Request, Status, Response};

use shared::executor_server::{Executor, ExecutorServer};
use shared::{JobRequest, JobResponse, WorkerMessage, WorkerRegistration};
use shared::worker_api_client::WorkerApiClient;
use shared::orchestrator_message;
use shared::worker_message;

#[derive(Debug)]
struct Worker {
    addr: SocketAddr,
}

#[tonic::async_trait]
impl Executor for Worker {

    async fn execute_job(
        &self, 
        _request: Request<JobRequest>
    ) -> Result<Response<JobResponse>, Status> {

        let response = JobResponse {
            result: format!("Job executed by worker at {}", self.addr).into_bytes(),
        };

        return Ok(Response::new(response));
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let addr = listener.local_addr()?;

    let worker = Worker { addr };
    
    // start the executor server
    println!("Worker listening on {}", addr);
    let _server_handle = tokio::spawn(async move {
        let incoming = TcpListenerStream::new(listener);
        Server::builder()
            .add_service(ExecutorServer::new(worker))
            .serve_with_incoming(incoming)
            .await
    });
    

    // register this worker with the orchestrator
    let orchestrator_url = "http://127.0.0.1:50051";
    let mut client = WorkerApiClient::connect(orchestrator_url).await?;

    // set up channel for streaming
    let (tx, rx) = mpsc::channel(32);
    let outbound = ReceiverStream::new(rx);

    // connect and get the response stream
    let response = client.connect_worker(Request::new(outbound)).await?;
    let mut inbound = response.into_inner();

    // spawn a task to handle incoming messages from the orchestrator
    tokio::spawn(async move {
        while let Some(result) = inbound.next().await {
            match result {
                Ok(orchestrator_msg) => {
                    match orchestrator_msg.message {
                        Some(orchestrator_message::Message::RegistrationAck(ack)) => {
                            println!("Received registration ack: {:?}", ack);
                        },
                        None => {
                            println!("Received empty message from orchestrator");
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Error receiving message from orchestrator: {:?}", e);
                    break;
                }
            }
        }
    });

    // send the initial registration message
    tx.send(WorkerMessage {
        message: Some(worker_message::Message::Registration(WorkerRegistration { address: addr.to_string() }))
    }).await?;

    // keep the main task alive
    tokio::signal::ctrl_c().await?;

    Ok(())
}