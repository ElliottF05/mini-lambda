use std::time::Duration;

use client::{Client, Job};
use tokio::task::JoinSet;

/// An example of how to use the Client and its associated APIs in Rust code.
#[tokio::main]
async fn main() {
    let wasm_path = "./crates/client/test-wasm/test-wasm.wasm";
    let wasm_bytes = std::fs::read(wasm_path)
        .unwrap_or_else(|e| panic!("wasm path not found: {}", e));
    let password = "password".to_string();

    let client = Client::connect("http://127.0.0.1:50051", Some(password)).await
        .unwrap_or_else(|e| panic!("failed to connect to the client: {}", e));

    let mut handles = vec![];
    for _ in 0..3 {
        let job = Job::from_bytes(wasm_bytes.clone())
            .arg(10.to_string())
            .timeout(Duration::from_secs(20));
        handles.push(client.submit_job(job));
    }

    let mut set = JoinSet::new();
    for h in handles {
        set.spawn(h.wait());
    }

    while let Some(result) = set.join_next().await {
        match result.unwrap() {
            Ok(output) => println!("{}", output),
            Err(e) => eprintln!("{}", e),
        }
    }
}