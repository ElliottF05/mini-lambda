mod registry;
mod errors;
mod handlers;
mod heartbeat;
mod queue;

use std::net::SocketAddr;

use axum::{extract::Extension, routing::post, Router};
use tracing::{error, info};
use clap::Parser;
use tower_http::trace::TraceLayer;

use registry::WorkerRegistry;

use crate::{handlers::{register_worker, request_worker, unregister_worker, update_credits}, heartbeat::handle_heartbeat_received, queue::PendingQueue};

const MAX_QUEUE_SIZE: usize = 10;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    #[derive(Parser)]
    struct Opts {
        /// address to bind, e.g. 127.0.0.1:8080
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
    }

    let opts = Opts::parse();

    let registry = WorkerRegistry::new();
    let pending_queue = PendingQueue::new(MAX_QUEUE_SIZE);

    // bind to configured address
    let listener = tokio::net::TcpListener::bind(&opts.bind).await.unwrap();
    let app = Router::new()
        .route("/register_worker", post(register_worker))
        .route("/unregister_worker", post(unregister_worker))
        .route("/update_credits", post(update_credits))
        .route("/request_worker", post(request_worker))
        .route("/heartbeat", post(handle_heartbeat_received)) // <-- new route to accept worker heartbeats
        .layer(TraceLayer::new_for_http()) // add request tracing
        .layer(Extension(registry)) // inject registry
        .layer(Extension(pending_queue)); // inject pending queue

    info!("orchestrator listening on {}", opts.bind);

    let server = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>());
    let graceful = server.with_graceful_shutdown(async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }
}
