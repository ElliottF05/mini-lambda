use std::net::SocketAddr;
use std::sync::Arc;

use shared::{OrchestratorMessage, WorkerRegistration, worker_api_client::WorkerApiClient, worker_message};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status};
use wasmer_wasix::PluggableRuntime;
use wasmer_wasix::runtime::task_manager::tokio::TokioTaskManager;

use shared::{WorkerMessage, orchestrator_message};

/// Worker struct representing the main Worker component.
/// It implements the Executor service, see executor.rs for details.
/// It also communicates bidirectionally with the Orchestrator, via its orchestrator_tx channel.
/// Worker internally uses Arc<RwLock<_>> so can be cloned cheaply.
#[derive(Debug, Clone)]
pub struct Worker {
    // note: all shared state fields should use Arc<RwLock<...>> for thread safety

    // Fields relating to the Executor service.
    pub addr: SocketAddr,
    pub wasm_runtime: PluggableRuntime,
    // TODO, look into wasmer's built-in caching: 
    // https://docs.rs/crate/wasmer-wasix/latest/source/src/runtime/module_cache/mod.rs

    // Fields relating to communication with the Orchestrator
    pub orchestrator_tx: mpsc::Sender<WorkerMessage>,
}

impl Worker {
    /// Create a new Worker instance.
    pub async fn new(addr: SocketAddr, orchestrator_url: &str) -> Worker {
        // 1. Set up Executor fields
        let tokio_task_manager = TokioTaskManager::new(tokio::runtime::Handle::current());
        let wasm_runtime = PluggableRuntime::new(Arc::new(tokio_task_manager));


        // 2. Set up communication with Orchestrator
        let mut client = WorkerApiClient::connect(orchestrator_url.to_string()).await
            .unwrap_or_else(|e| panic!("Orchestrator should be reachable at {} before workers start: {}", orchestrator_url, e));

        // Set up channel for streaming
        let (tx, rx) = mpsc::channel(32);
        let outbound = ReceiverStream::new(rx);

        // Connect and get the response stream
        let response = client.connect_worker(Request::new(outbound)).await
            .unwrap_or_else(|e| panic!("Orchestrator should accept worker connections during startup, received error {}", e));
        let mut inbound = response.into_inner();

        // Create the Worker instance
        let worker = Worker {
            addr,
            wasm_runtime,
            orchestrator_tx: tx
        };

        // Spawn a task to handle incoming messages from the orchestrator
        let mut worker_clone = worker.clone();
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                if !worker_clone.handle_orchestrator_message(result).await {
                    break;
                }
            }
        });

        // Send the initial registration message
        worker.orchestrator_tx.send(WorkerMessage {
            message: Some(worker_message::Message::Registration(WorkerRegistration { address: addr.to_string() }))
        }).await.unwrap_or_else(|e| panic!("Channel to Orchestrator should be working for initial registration, got error {}", e));

        worker
    }

    pub async fn handle_orchestrator_message(&mut self, result: Result<OrchestratorMessage, Status>) -> bool {
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