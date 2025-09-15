use clap::Parser;
use anyhow::anyhow;
use reqwest::{StatusCode};
use std::{path::PathBuf, time::Instant};
use tokio::fs;
use mini_lambda_proto::{hash_wasm_module, JobManifest, JobSubmissionHash, JobSubmissionWasm, SubmitResponse};

#[derive(Parser)]
/// Command-line arguments for the mini-lambda client application.
struct Opts {
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

async fn submit(
    client: &reqwest::Client,
    server_address: &str,
    wasm_bytes: Vec<u8>,
    job_manifest: &JobManifest,
) -> anyhow::Result<SubmitResponse> {

    let server_address = server_address.trim_end_matches('/');

    // create hash submission
    let wasm_hash = hash_wasm_module(&wasm_bytes);
    let hash_submission = JobSubmissionHash {
        module_hash: wasm_hash,
        manifest: job_manifest.clone(),
    };


    // try submit_hash first
    let resp = client
        .post(format!("{}/submit_hash", server_address))
        .json(&hash_submission)
        .send()
        .await?;

    let status = resp.status();
    let mut body = resp.bytes().await.unwrap_or_default();

    // if not found, upload wasm
    if status == StatusCode::NOT_FOUND {

        // create wasm submission
        let wasm_submission = JobSubmissionWasm {
            module_bytes: wasm_bytes,
            manifest: job_manifest.clone(),
        };

        let resp = client
            .post(format!("{}/submit_wasm", server_address))
            .json(&wasm_submission)
            .send()
            .await?;

        let status = resp.status();
        body = resp.bytes().await.unwrap_or_default();

        if !status.is_success() {
            return Err(anyhow!(
                "submit failed: {} - {}",
                status,
                String::from_utf8_lossy(&body)
            ));
        }

        let submit: SubmitResponse = serde_json::from_slice(&body)?;
        return Ok(submit);
    }

    if !status.is_success() {
        return Err(anyhow!(
            "submit_hash failed: {} - {}",
            status,
            String::from_utf8_lossy(&body)
        ));
    }

    let submit: SubmitResponse = serde_json::from_slice(&body)?;
    Ok(submit)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start = Instant::now();
    let opts = Opts::parse();

    let wasm_bytes = fs::read(&opts.wasm).await?;
    let manifest = JobManifest { call_args: opts.call_args };

    let client = reqwest::Client::new();

    let submit = submit(&client, &opts.server, wasm_bytes, &manifest).await?;

    println!("job completed: {}", submit.job_id);
    if let Some(msg) = submit.message {
        println!("{}", msg);
    }

    println!("total time: {:.2?}", start.elapsed());

    Ok(())
}