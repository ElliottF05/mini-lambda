use std::{ops::Range, time::Duration};

use rand;
use tokio::task::JoinSet;

use client::{Client, Job};

/// An example of how to use the Client and its associated APIs in Rust code.
#[tokio::main]
async fn main() {
    // Configure the path to the wasm file here.
    let wasm_path = "./crates/client/test-wasm/test-wasm.wasm";
    let wasm_bytes = std::fs::read(wasm_path)
        .unwrap_or_else(|e| panic!("wasm path not found: {}", e));
    let password = "password".to_string();

    // Configure the number of clients and their job submission patterns here.
    let num_clients = 1; // should be >= 1
    let total_jobs_per_client = 100;
    let concurrent_jobs_per_client = 12; // should be less that total_jobs_per_client
    let cached_probability = 0.2; // should be in [0,1]

    // Configure job details
    let sleep_range = 0..5;
    let timeout_range = 1..12;
    let job_failure_probability = 0.15;

    let mut join_handles = JoinSet::new();
    for _ in 0..num_clients {
        let wasm_bytes = wasm_bytes.clone();
        let password = password.clone();
        let sleep_range = sleep_range.clone();
        let timeout_range = timeout_range.clone();
        join_handles.spawn(tokio::spawn(async move {
            let client = Client::connect("http://127.0.0.1:50051", Some(password), true).await
                .unwrap_or_else(|e| panic!("failed to connect to the client: {}", e));

            let mut join_set = JoinSet::new();
            for _ in 0..concurrent_jobs_per_client {
                let job = create_job(&wasm_bytes, cached_probability, job_failure_probability, &sleep_range, &timeout_range);
                join_set.spawn(client.submit_job(job).wait());
            }

            let mut jobs_remaining = total_jobs_per_client - concurrent_jobs_per_client;
            while let Some(result) = join_set.join_next().await {
                match result {
                    Err(e) => panic!("client task panicked: {}", e),
                    Ok(Ok(output)) => println!("{}", output),
                    Ok(Err(e)) => eprintln!("{}", e),
                }

                if jobs_remaining > 0 {
                    jobs_remaining -= 1;
                    let job = create_job(&wasm_bytes, cached_probability, job_failure_probability, &sleep_range, &timeout_range);
                    join_set.spawn(client.submit_job(job).wait());
                }
            }
        }));

        while let Some(result) = join_handles.join_next().await {
            _ = result.unwrap_or_else(|e| panic!("client task panicked: {}", e));
        }
    }
}

fn create_job(
    wasm: &[u8], 
    cached_probability: f64, 
    job_failure_probability: f64, 
    sleep_range: &Range<i32>, 
    timeout_range: &Range<u64>) -> Job {
    let bytes = if rand::random::<f64>() < cached_probability { wasm.to_vec() } else {
        let salt: [u8; 64] = rand::random();
        add_custom_section(&wasm, &salt)
    };

    let arg = if rand::random::<f64>() < job_failure_probability { "invalid".to_string() } else {
        rand::random_range(sleep_range.clone()).to_string()
    };
    let job = Job::from_bytes(bytes)
        .arg(arg)
        .timeout(Duration::from_secs(rand::random_range(timeout_range.clone())));

    return job;
}

fn add_custom_section(wasm: &[u8], salt: &[u8]) -> Vec<u8> {
    let name = b"salt";
    
    // custom section content = LEB128(name.len) + name + salt
    let mut section_content = Vec::new();
    leb128_encode(&mut section_content, name.len() as u32);
    section_content.extend_from_slice(name);
    section_content.extend_from_slice(salt);

    let mut result = wasm.to_vec();
    result.push(0x00); // custom section id
    leb128_encode(&mut result, section_content.len() as u32);
    result.extend(section_content);
    result
}

fn leb128_encode(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}