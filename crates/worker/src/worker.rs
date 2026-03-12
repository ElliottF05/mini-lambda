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

        // Set up Executor fields
        let tokio_task_manager = TokioTaskManager::new(tokio::runtime::Handle::current());
        let wasm_runtime = PluggableRuntime::new(Arc::new(tokio_task_manager));


        // Set up communication with Orchestrator
        let (tx, inbound) = Worker::connect_to_orchestrator(orchestrator_url).await;

        // Create the Worker instance
        let mut worker = Worker {
            addr,
            wasm_runtime,
            orchestrator_tx: tx
        };

        // Begin the bidirectional communication session with the Orchestrator
        worker.start_orchestrator_session(inbound).await;
        worker
    }
}