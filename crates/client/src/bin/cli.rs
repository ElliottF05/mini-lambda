use clap::Parser;
use client::{Client, Job};

#[derive(Parser, Debug)]
#[command(about = "Submit a wasm job to the distributed compute platform")]
struct Args {
    wasm_path: String,
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    orchestrator: String,
    #[arg(trailing_var_arg = true)]
    wasm_args: Vec<String>
}

/// The main cli entrypoint to the Client, allowing submission of a wasm job.
#[tokio::main]
pub async fn main() {
    let args = Args::parse();
    let job = Job::from_path(&args.wasm_path)
        .expect("wasm file path not found")
        .args(args.wasm_args);

    let mut client = Client::connect(args.orchestrator).await;
    let result = client.submit_job(job).wait().await;
    match result {
        Ok(output) => {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            if !output.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
            }
        },
        Err(e) => {
            eprintln!("Job failed: {}", e);
        }
    }
}