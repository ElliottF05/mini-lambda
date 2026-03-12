use shared::{OrchestratorMessage, WorkerRegistration, orchestrator_message, worker_api_client::WorkerApiClient, worker_message};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status, Streaming};

use shared::{WorkerMessage};

use crate::worker::Worker;

// Implement Worker function related to communication with the Orchestrator
impl Worker {

    /// Connects to the Orchestrator and returns a sender for outbound messages,
    /// and a stream for inbound messages.
    pub async fn connect_to_orchestrator(orchestrator_url: &str) -> (Sender<WorkerMessage>, Streaming<OrchestratorMessage>) {
        let mut client = WorkerApiClient::connect(orchestrator_url.to_string()).await
            .unwrap_or_else(|e| panic!("Orchestrator should be reachable at {} before workers start: {}", orchestrator_url, e));

        // Set up channel for streaming
        let (tx, rx) = mpsc::channel(32);
        let outbound = ReceiverStream::new(rx);

        // Connect and get the response stream
        let response = client.connect_worker(Request::new(outbound)).await
            .unwrap_or_else(|e| panic!("Orchestrator should accept worker connections during startup, received error {}", e));
        let inbound = response.into_inner();

        return (tx, inbound);
    }

    /// Start a bidirectional communication session with the Orchestrator. This consists of 
    /// spawing a task to process inbound messages, and sending the initial registration message.
    pub async fn start_orchestrator_session(&mut self, mut inbound: Streaming<OrchestratorMessage>) {
        // Spawn a task to handle incoming messages from the orchestrator
        let worker_clone = self.clone();
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                if !worker_clone.handle_orchestrator_message(result).await {
                    break;
                }
            }
        });

        // Send the initial registration message
        self.orchestrator_tx.send(WorkerMessage {
            message: Some(worker_message::Message::Registration(WorkerRegistration { address: self.addr.to_string() }))
        }).await.unwrap_or_else(|e| panic!("Channel to Orchestrator should be working for initial registration, got error {}", e));
    }

    /// Handles all incoming messages from the Orchestrator.
    pub async fn handle_orchestrator_message(&self, result: Result<OrchestratorMessage, Status>) -> bool {
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
                // TODO: error here likely means broken pipe, necessitating reconnection,
                // this is an issue that can be handled later?
                return false;
            }
        }

        return true;
    }
}