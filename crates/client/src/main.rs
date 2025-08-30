use clap::Parser;
use std::path::PathBuf;
use tokio::fs;
use mini_lambda_proto::{JobManifest, SubmitResponse, JobSubmission};

#[derive(Parser)]
/// Command-line arguments for the mini-lambda client application.
struct CliArgs {
    /// WASM file to submit
    #[arg(value_name = "WASM")]
    wasm: PathBuf,

    /// Arguments passed to the wasm function (positional, zero or more)
    #[arg(value_name = "ARGS")]
    call_args: Vec<String>,

    /// Orchestrator URL
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli_args = CliArgs::parse();

    let wasm_bytes = fs::read(&cli_args.wasm).await?;
    let manifest = JobManifest { call_args: cli_args.call_args };

    let job_submission = JobSubmission {
        module_bytes: wasm_bytes,
        manifest: manifest
    };

    println!("sending job submission to {} with manifest: {:?} and module size {}\n", cli_args.server, job_submission.manifest, job_submission.module_bytes.len());

    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/submit", cli_args.server.trim_end_matches('/')))
        .json(&job_submission)
        .send()
        .await?;

    let status = resp.status();
    let body_bytes = resp.bytes().await.unwrap_or_default();

    if !status.is_success() {
        eprintln!(
            "submit failed: {} - {}",
            status,
            String::from_utf8_lossy(&body_bytes)
        );
        std::process::exit(1);
    }

    let submit: SubmitResponse = serde_json::from_slice(&body_bytes)?;
    println!("submitted job: {}", submit.job_id);
    if let Some(msg) = submit.message {
        println!("{}", msg);
    }

    Ok(())
}