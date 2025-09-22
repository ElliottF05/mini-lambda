mod module_cache;
mod queue_ticket;
mod errors;
mod runner;
mod registration;
mod submission;

use std::sync::{atomic::AtomicUsize, Arc};
use axum::{
    routing::post,
    extract::Extension,
    Router,
};

use tracing::{error, info};
use clap::Parser;
use reqwest::Client;
use mini_lambda_proto::{RegisterWorkerRequest, RegisterWorkerResponse};
use uuid::Uuid;

use wasmer::{Module, Engine};

use crate::module_cache::ModuleCache;
use crate::errors::WorkerError;
use crate::registration::unregister_worker;
use crate::submission::{handle_submit_wasm, handle_submit_hash};


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

    let register_url = format!("{}/register_worker", base);
    info!("registering with orchestrator at {}", register_url);

    let req = RegisterWorkerRequest { port };

    let client = Client::new();

    let worker_id = match client.post(&register_url).json(&req).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let body: RegisterWorkerResponse = resp.json().await.map_err(|e| WorkerError::Registration(format!("failed to parse response: {}", e)))?;
                info!("registered with orchestrator {}, worker_id is {}", register_url, body.worker_id);
                Ok::<Uuid, WorkerError>(body.worker_id)
            } else {
                error!("failed to register with orchestrator: {} -> status {}", register_url, resp.status());
                Err(WorkerError::Registration(format!("registration failed: status {}", resp.status())))?
            }
        }
        Err(e) => {
            error!("failed to register with orchestrator {}: {}", register_url, e);
            Err(WorkerError::Registration(format!("registration failed: {}", e)))?
        }
    }?;

    let queue_len = Arc::new(AtomicUsize::new(0));

    // build and serve app using the already-bound listener
    let app = Router::new()
        .route("/submit_wasm", post(handle_submit_wasm))
        .route("/submit_hash", post(handle_submit_hash))
        .layer(Extension(engine)) // inject engine
        .layer(Extension(module_cache)) // inject module cache
        .layer(Extension(queue_len)); // inject queue length

    let server = axum::serve(listener, app);
    let graceful = server.with_graceful_shutdown(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
            return;
        }

        // unregister on shutdown
        info!("shutdown signal received, unregistering from orchestrator...");
        match unregister_worker(&client, &base, worker_id).await {
            Ok(_) => info!("unregistered successfully"),
            Err(e) => error!("failed to unregister during shutdown: {}", e),
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }

    Ok(())
}