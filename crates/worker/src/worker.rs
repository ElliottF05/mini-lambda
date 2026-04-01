use std::net::SocketAddr;

use tokio::sync::mpsc;

use shared::{WorkerMessage};
use wasmtime::component::Linker;
use wasmtime::{Config, Engine};

use crate::executor::ComponentRunStates;

/// Worker struct representing the main Worker component.
/// It implements the Executor service, see executor.rs for details.
/// It also communicates bidirectionally with the Orchestrator, via its orchestrator_tx channel.
/// Worker internally uses Arc<RwLock<_>> so can be cloned cheaply.
#[derive(Clone)]
pub struct Worker {
    // note: all shared state fields should use Arc<RwLock<...>> for thread safety

    // Fields relating to the Executor service.
    pub addr: SocketAddr,
    pub wasm_engine: Engine,
    pub wasm_linker: Linker<ComponentRunStates>,

    // Fields relating to communication with the Orchestrator
    pub orchestrator_tx: mpsc::Sender<WorkerMessage>,
}

impl Worker {
    /// Create a new Worker instance.
    pub async fn new(addr: SocketAddr, orchestrator_endpoint: &str, worker_credits: u32) -> Worker {

        // Set up Executor fields
        let wasm_engine = Engine::new(Config::new().epoch_interruption(true))
            .unwrap_or_else(|e| panic!("Failed to initialize the wasmtime engine: {e}"));
        let mut wasm_linker: Linker<ComponentRunStates> = Linker::new(&wasm_engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut wasm_linker)
            .unwrap_or_else(|e| panic!("Failed to add WASI to the linker: {e}"));


        // Set up communication with Orchestrator
        let (orchestrator_tx, inbound) = Worker::connect_to_orchestrator(orchestrator_endpoint).await;

        // Create the Worker instance
        let worker = Worker {
            addr,
            wasm_engine,
            wasm_linker,
            orchestrator_tx,
        };

        // Begin the bidirectional communication session with the Orchestrator
        worker.start_orchestrator_session(inbound, worker_credits).await;
        worker
    }
}