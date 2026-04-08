use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use blake3::Hash;
use dashmap::DashMap;
use lru::LruCache;
use tokio::sync::{OnceCell, Mutex, mpsc};

use shared::{WorkerMessage};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine};

use crate::executor::ComponentRunStates;

/// Worker struct representing the main Worker component.
/// It implements the Executor service, see executor.rs for details.
/// It also communicates bidirectionally with the Orchestrator, via its orchestrator_tx channel.
/// Worker internally uses Arc<RwLock/Mutex<_>> so can be cloned cheaply.
#[derive(Clone)]
pub struct Worker {
    // note: all shared state fields should use Arc<RwLock/Mutex<...>> for thread safety

    // Fields relating to the Executor service.
    pub addr: SocketAddr,
    pub wasm_engine: Engine,
    pub wasm_linker: Linker<ComponentRunStates>,
    pub cancellation_tokens: Arc<DashMap<Uuid, CancellationToken>>,
    pub component_cache: Arc<Mutex<LruCache<Hash, Arc<OnceCell<Component>>>>>,

    // Fields relating to communication with the Orchestrator
    pub orchestrator_tx: mpsc::Sender<WorkerMessage>,

    // Fields relating to both
    pub jwt_secret: Arc<OnceLock<[u8; 32]>>,
    pub network_access_allowed: Arc<OnceLock<bool>>,
}

impl Worker {
    /// Create a new Worker instance.
    pub async fn new(addr: SocketAddr, orchestrator_endpoint: &str, password: Option<String>, worker_credits: u32) -> Worker {

        // Set up Executor fields
        let wasm_engine = Engine::new(Config::new().epoch_interruption(true))
            .unwrap_or_else(|e| panic!("Failed to initialize the wasmtime engine: {e}"));
        let mut wasm_linker: Linker<ComponentRunStates> = Linker::new(&wasm_engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut wasm_linker)
            .unwrap_or_else(|e| panic!("Failed to add WASI to the linker: {e}"));

        // Create the background tasks that increments epoch
        let engine = wasm_engine.clone();
        tokio::task::spawn(async move {
            let mut inteval = tokio::time::interval(Duration::from_millis(10));
            loop {
                inteval.tick().await;
                engine.increment_epoch();
            }
        });


        // Set up communication with Orchestrator
        let (orchestrator_tx, inbound) = Worker::connect_to_orchestrator(orchestrator_endpoint, password).await;

        // Create the Worker instance
        let worker = Worker {
            addr,
            wasm_engine,
            wasm_linker,
            cancellation_tokens: Arc::new(DashMap::new()),
            orchestrator_tx,
            component_cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(64).unwrap()))),
            jwt_secret: Arc::new(OnceLock::new()),
            network_access_allowed: Arc::new(OnceLock::new())
        };

        // Begin the bidirectional communication session with the Orchestrator
        worker.start_orchestrator_session(inbound, worker_credits).await;
        worker
    }
}