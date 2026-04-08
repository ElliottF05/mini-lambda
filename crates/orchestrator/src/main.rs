mod orchestrator;
mod client_api;
mod worker_api;
mod registry;
mod job_queue;
mod errors;
mod diagnostics;
mod tui;

use clap::Parser;
use tonic::transport::Server;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use shared::{client_api_server::ClientApiServer, worker_api_server::WorkerApiServer};
use crate::{client_api::check_client_auth, orchestrator::Orchestrator, worker_api::check_worker_auth};

#[derive(Parser, Debug)]
#[command(about = "Run the Orchestrator server")]
struct Args {
    #[arg(default_value = "127.0.0.1:50051")]
    addr: std::net::SocketAddr,
    #[arg(long, help = "Password required for workers to register. If not set, no password is required.")]
    worker_password: Option<String>,
    #[arg(long, help = "Password required for clients to submit jobs. If not set, no password is required.")]
    client_password: Option<String>,
    #[arg(long, help = "Enable debug logging")]
    verbose: bool,
    #[arg(long, help = "Launch the interactive TUI dashboard")]
    tui: bool,
}

fn init_tracing_plain(verbose: bool) {
    let filter = if verbose { "orchestrator=debug" } else { "orchestrator=info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| filter.into())
        )
        .init();
}

fn init_tracing_tui(verbose: bool) {
    let level = if verbose { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    tui_logger::init_logger(level).unwrap_or_else(|e| {
        eprintln!("failed to init tui-logger: {e}");
        std::process::exit(1);
    });
    tui_logger::set_default_level(level);

    // With the tracing-support feature, init_logger only starts the buffer thread.
    // We must also register TuiTracingSubscriberLayer as the tracing backend.
    let filter = if verbose { "orchestrator=debug" } else { "orchestrator=info" };
    tracing_subscriber::registry()
        .with(tui_logger::TuiTracingSubscriberLayer)
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| filter.into()),
        )
        .init();
}

/// Main entry point for the Orchestrator server binary.
#[tokio::main]
pub async fn main() {
    let args = Args::parse();

    let addr = args.addr;
    let orchestrator = Orchestrator::new(args.worker_password, args.client_password);

    let client_server = ClientApiServer::with_interceptor(orchestrator.clone(), check_client_auth(orchestrator.clone()));
    let worker_server = WorkerApiServer::with_interceptor(orchestrator.clone(), check_worker_auth(orchestrator.clone()));

    if args.tui {
        init_tracing_tui(args.verbose);

        let diagnostics = orchestrator.diagnostics.clone();
        tokio::spawn(async move {
            tracing::info!("Orchestrator listening on {}", addr);
            Server::builder()
                .add_service(client_server)
                .add_service(worker_server)
                .serve(addr)
                .await
                .unwrap_or_else(|e| panic!("Failed to serve the Orchestrator: {}", e));
        });

        tui::run(diagnostics).await
            .unwrap_or_else(|e| eprintln!("TUI error: {e}"));
    } else {
        init_tracing_plain(args.verbose);
        tracing::info!("Orchestrator listening on {}", addr);
        Server::builder()
            .add_service(client_server)
            .add_service(worker_server)
            .serve(addr)
            .await
            .unwrap_or_else(|e| panic!("Failed to serve the Orchestrator: {}", e));
    }
}
