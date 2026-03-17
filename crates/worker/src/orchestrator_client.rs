use std::sync::atomic::Ordering;

use shared::{CreditUpdate, OrchestratorMessage, WorkerRegistration, orchestrator_message, worker_api_client::WorkerApiClient, worker_message};
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
    pub async fn start_orchestrator_session(&self, mut inbound: Streaming<OrchestratorMessage>, worker_credits: u32) {
        // Spawn a task to handle incoming messages from the orchestrator
        let worker_clone = self.clone();
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                worker_clone.handle_orchestrator_message(result).await;
            }
            eprintln!("Orchestrator stream closed, shutting down");
            std::process::exit(1);
        });

        // Send the initial registration message
        self.orchestrator_tx.send(WorkerMessage {
            message: Some(worker_message::Message::Registration(WorkerRegistration { address: self.addr.to_string(), credits: worker_credits }))
        }).await.unwrap_or_else(|e| panic!("Channel to Orchestrator should be working for initial registration, got error {}", e));
    }

    /// Sends a credit update to the Orchestrator with the current available credit count
    /// for this worker. Spawns a background task to do this, does not block the caller.
    pub fn send_credit_update(&self) {
        let tx= self.orchestrator_tx.clone();
        let credits = self.credits.load(Ordering::Relaxed);
        tokio::spawn(async move {
            if tx.send(WorkerMessage { 
                message: Some(worker_message::Message::CreditUpdate(CreditUpdate { credits }))
            }).await.is_err() {
                eprintln!("Lost connection to the Orchestrator, shutting down");
                std::process::exit(1);
            }
        });
    }

    /// Handles all incoming messages from the Orchestrator.
    pub async fn handle_orchestrator_message(&self, result: Result<OrchestratorMessage, Status>) {
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
                eprintln!("Connection with Orchestrator dropped, shutting down: {:?}", e);
                std::process::exit(1);
            }
        }
    }
}