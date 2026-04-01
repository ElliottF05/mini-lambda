mod worker;
mod executor;
mod errors;
mod orchestrator_client;
mod credit_guard;

use clap::Parser;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

use shared::executor_server::ExecutorServer;

use crate::worker::Worker;

#[derive(Parser, Debug)]
#[command(about = "Run a Worker server")]
struct Args {
    bind_host: String,
    worker_credits: u32,
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    orchestrator: String,
}

/// Main entry point for the Worker server binary.
#[tokio::main]
pub async fn main() {
    let args = Args::parse();
    let orchestrator_endpoint = &args.orchestrator;
    let bind_host = &args.bind_host;
    let worker_credits = args.worker_credits;

    let listener = TcpListener::bind(format!("{}:0", bind_host)).await
        .unwrap_or_else(|e| panic!("Failed to bind to host {}: {}", bind_host, e));
    let addr = listener.local_addr()
        .unwrap_or_else(|e| panic!("Failed to fetch port Worker is bound to: {}", e));

    // Register this worker with the orchestrator
    let worker = Worker::new(addr, orchestrator_endpoint, worker_credits).await;
    
    // Start the executor server
    println!("Worker listening on {}", addr);
    let incoming = TcpListenerStream::new(listener);
    Server::builder()
        .add_service(ExecutorServer::new(worker))
        .serve_with_incoming_shutdown(incoming, async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
        .unwrap_or_else(|e| panic!("Executor server failed: {}", e));
}