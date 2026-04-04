use shared::{OrchestratorMessage, WorkerRegistration, orchestrator_message, worker_api_client::WorkerApiClient, worker_message};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status, Streaming, transport::Channel};

use shared::{WorkerMessage};

use crate::worker::Worker;

// Implement Worker function related to communication with the Orchestrator
impl Worker {

    /// Connects to the Orchestrator and returns a sender for outbound messages,
    /// and a stream for inbound messages.
    pub async fn connect_to_orchestrator(orchestrator_endpoint: &str, password: Option<String>) -> (Sender<WorkerMessage>, Streaming<OrchestratorMessage>) {

        let channel = Channel::from_shared(orchestrator_endpoint.to_string())
            .unwrap_or_else(|e| panic!("invalid orchestrator endpoint '{}': {}", orchestrator_endpoint, e))
            .connect().await
            .unwrap_or_else(|e| panic!("failed to connect to orchestrator at '{}': {}", orchestrator_endpoint, e));
        let mut client = WorkerApiClient::with_interceptor(channel, move |mut req: Request<()>| {
            if let Some(pass) = &password {
                let val = pass.parse()
                    .map_err(|e| Status::invalid_argument(format!("invalid authorization header value: {e}")))?;
                req.metadata_mut().insert("authorization", val);
            }
            Ok(req)
        });

        // Set up channel for streaming
        let (tx, rx) = mpsc::channel(32);
        let outbound = ReceiverStream::new(rx);

        // Connect and get the response stream
        let response = client.connect_worker(Request::new(outbound)).await
            .unwrap_or_else(|e| panic!("Orchestrator should accept worker connections during startup, received error {}", e));
        let inbound = response.into_inner();

        (tx, inbound)
    }

    /// Start a bidirectional communication session with the Orchestrator. This consists of 
    /// spawing a task to process inbound messages, and sending the initial registration message.
    pub async fn start_orchestrator_session(&self, mut inbound: Streaming<OrchestratorMessage>, credits: u32) {

        // Send the initial registration message
        let address = self.addr.to_string();
        self.orchestrator_tx.send(WorkerMessage {
            message: Some(worker_message::Message::Registration(WorkerRegistration { address, credits }))
        }).await.unwrap_or_else(|e| panic!("Channel to Orchestrator should be working for initial registration, got error {}", e));
        
        let jwt_secret = match inbound.message().await {
            Ok(Some(OrchestratorMessage { message: Some(orchestrator_message::Message::RegistrationAck(ack)) })) => {
                ack.jwt_secret.try_into()
                    .unwrap_or_else(|e| panic!("received malformed jwt secret: {:?}", e))
            },
            msg => panic!("expected registration ack as first message, got: {:?}", msg)
        };
        self.jwt_secret.set(jwt_secret).ok();

        // Spawn a task to handle incoming messages from the orchestrator
        let worker = self.clone();
        tokio::spawn(async move {
            loop {
                match inbound.message().await {
                    Ok(Some(message)) => worker.handle_orchestrator_message(message).await,
                    Ok(None) => {
                        eprintln!("Orchestrator stream closed, shutting down");
                        break;
                    },
                    Err(e) => {
                        eprintln!("Received an error from the Orchestrator: {}, shutting down", e);
                        break;
                    }
                }
            }
            std::process::exit(1);
        });
    }

    /// Handles all incoming messages from the Orchestrator.
    pub async fn handle_orchestrator_message(&self, message: OrchestratorMessage) {
        match message.message {
            Some(orchestrator_message::Message::RegistrationAck(_ack)) => {
                eprintln!("Unexpectedly received registration ack while already registered");
            },
            None => {
                println!("Received empty message from orchestrator");
            }
        }
    }
}