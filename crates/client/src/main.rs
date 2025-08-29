use clap::Parser;
use reqwest::multipart;
use std::path::PathBuf;
use tokio::fs;
use mini_lambda_proto::{JobManifest, SubmitResponse};

#[derive(Parser)]
/// Command-line arguments for the mini-lambda client application.
struct Args {
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
    let args = Args::parse();

    let wasm_bytes = fs::read(&args.wasm).await?;
    let manifest = JobManifest { args: args.call_args };

    let client = reqwest::Client::new();

    let manifest_json = serde_json::to_string(&manifest)?;
    let form = multipart::Form::new()
        .part("module", multipart::Part::bytes(wasm_bytes).file_name(
            args.wasm.file_name().and_then(|s| s.to_str()).unwrap_or("module.wasm").to_string()
        ))
        .text("manifest", manifest_json);

    let resp = client
        .post(format!("{}/submit", args.server.trim_end_matches('/')))
        .multipart(form)
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
    println!("submitted job: {}", (submit.job_id).0);
    if let Some(msg) = submit.message {
        println!("{}", msg);
    }

    Ok(())
}