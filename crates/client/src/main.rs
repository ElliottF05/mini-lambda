use clap::Parser;
use reqwest::StatusCode;
use std::{hash, path::PathBuf, time::Instant};
use tokio::fs;
use mini_lambda_proto::{hash_wasm_module, JobManifest, JobSubmissionHash, JobSubmissionWasm, SubmitResponse};

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
    let start = Instant::now();
    let cli_args = CliArgs::parse();

    let wasm_bytes = fs::read(&cli_args.wasm).await?;
    let wasm_hash = hash_wasm_module(&wasm_bytes);
    let manifest = JobManifest { call_args: cli_args.call_args };

    let job_submission_wasm = JobSubmissionWasm {
        module_bytes: wasm_bytes,
        manifest: manifest
    };

    let job_submission_hash = JobSubmissionHash {
        module_hash: wasm_hash,
        manifest: job_submission_wasm.manifest.clone(),
    };

    println!("sending job submission to {} with manifest: {:?} and module size {}\n", cli_args.server, job_submission_wasm.manifest, job_submission_wasm.module_bytes.len());

    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/submit_hash", cli_args.server.trim_end_matches('/')))
        .json(&job_submission_hash)
        .send()
        .await?;

    let mut status = resp.status();
    let mut body_bytes = resp.bytes().await.unwrap_or_default();

    if status == StatusCode::NOT_FOUND {
        println!("module not found in cache, submitting full wasm module ({} bytes)", job_submission_wasm.module_bytes.len());
        let resp = client
            .post(format!("{}/submit_wasm", cli_args.server.trim_end_matches('/')))
            .json(&job_submission_wasm)
            .send()
            .await?;
        status = resp.status();
        body_bytes = resp.bytes().await.unwrap_or_default();
    }

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

    println!("total time: {:.2?}", start.elapsed());

    Ok(())
}