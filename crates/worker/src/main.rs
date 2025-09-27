mod module_cache;
mod job_ticket;
mod errors;
mod runner;
mod registration;
mod submission;
mod heartbeat;

use std::sync::{atomic::AtomicUsize, Arc};
use axum::{
    routing::post,
    extract::Extension,
    Router,
};

use tracing::{error, info};
use clap::Parser;
use reqwest::Client;

use wasmer::{Module, Engine};

use crate::heartbeat::start_heartbeat_loop;
use crate::module_cache::ModuleCache;
use crate::errors::WorkerError;
use crate::registration::{register_with_orchestrator, unregister_with_orchestrator};
use crate::submission::{handle_submit_wasm, handle_submit_hash};


// Basic constants for config for now
const HEARTBEAT_INTERVAL_MS: u64 = 500;
const MAX_CREDITS: usize = 1;


#[tokio::main]
async fn main() -> Result<(), WorkerError> {
    tracing_subscriber::fmt::init();

    #[derive(Parser)]
    struct Opts {
        /// Orchestrator base URL, e.g. http://127.0.0.1:8080
        #[arg(long)]
        orchestrator: String,
    }

    let opts = Opts::parse();

    // Create the shared engine
    let engine = Arc::new(Engine::default());

    // Create the module cache
    let module_cache = Arc::new(tokio::sync::Mutex::new(ModuleCache::<Module>::new()));

    // bind to an OS-assigned port so multiple workers can run on the same host
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.map_err(|e| WorkerError::Registration(format!("failed to bind listener: {}", e)))?;
    let local = listener.local_addr().map_err(|e| WorkerError::Registration(format!("failed to get local_addr: {}", e)))?;
    let port = local.port();

    // register with orchestrator
    let base = if opts.orchestrator.starts_with("http://") || opts.orchestrator.starts_with("https://") {
        opts.orchestrator.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", opts.orchestrator.trim_end_matches('/'))
    };

    let client = Client::new();
    let worker_id = register_with_orchestrator(&client, &base, port).await?;
    let num_jobs = Arc::new(AtomicUsize::new(0));
    let seq = Arc::new(AtomicUsize::new(0));

    let heartbeat_shutdown_tx = start_heartbeat_loop(client.clone(), base.clone(), worker_id, num_jobs.clone(), seq.clone());

    // build and serve app using the already-bound listener
    let app = Router::new()
        .route("/submit_wasm", post(handle_submit_wasm))
        .route("/submit_hash", post(handle_submit_hash))
        .layer(Extension(engine)) // inject engine
        .layer(Extension(module_cache)) // inject module cache
        .layer(Extension(num_jobs)) // inject number of jobs
        .layer(Extension(seq)); // inject sequence number


    let server = axum::serve(listener, app);
    let graceful = server.with_graceful_shutdown(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
            return;
        }

        // stop heartbeat loop
        heartbeat_shutdown_tx.send(()).map_err(|_| error!("failed to stop heartbeat loop")).ok();

        // unregister on shutdown
        info!("shutdown signal received, unregistering from orchestrator...");
        match unregister_with_orchestrator(&client, &base, worker_id).await {
            Ok(_) => info!("unregistered successfully"),
            Err(e) => error!("failed to unregister during shutdown: {}", e),
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }

    Ok(())
}