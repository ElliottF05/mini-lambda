use std::io::{Read, Write};

use axum::{
    extract::Multipart,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use mini_lambda_proto::{JobId, JobManifest, SubmitResponse};
use serde_json::json;
use std::{net::SocketAddr, ops::Sub, path::PathBuf};
use tokio::fs;
use tracing::{error, info};
use uuid::Uuid;

use wasmer::Module;
use wasmer_wasix::{
    runners::wasi::{WasiRunner, RuntimeOrEngine},
    Pipe
};

async fn submit(mut multipart: Multipart) -> impl IntoResponse {
    let mut manifest: Option<JobManifest> = None;
    let mut wasm_bytes: Option<Vec<u8>> = None;

    // iterate fields
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name().unwrap() {
            "manifest" => {
                match field.text().await {
                    Ok(text) => match serde_json::from_str::<JobManifest>(&text) {
                        Ok(m) => manifest = Some(m),
                        Err(e) => {
                            let msg = format!("failed to parse manifest JSON: {}", e);
                            error!("{}", msg);
                            return (StatusCode::BAD_REQUEST, msg).into_response();
                        }
                    },
                    Err(e) => {
                        let msg = format!("failed to read manifest field: {}", e);
                        error!("{}", msg);
                        return (StatusCode::BAD_REQUEST, msg).into_response();
                    }
                }
            }
            "module" => {
                match field.bytes().await {
                    Ok(bytes) => wasm_bytes = Some(bytes.to_vec()),
                    Err(e) => {
                        let msg = format!("failed to read module bytes: {}", e);
                        error!("{}", msg);
                        return (StatusCode::BAD_REQUEST, msg).into_response();
                    }
                }
            }
            other => {
                info!("ignoring unexpected field: {}", other);
            }
        }
    }

    if manifest.is_none() || wasm_bytes.is_none() {
        let msg = "missing manifest or module field".to_string();
        error!("{}", msg);
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    
    // WASMER STUFF

    // run the Wasm execution in a blocking thread so we don't block the tokio runtime
    let run_result = tokio::task::spawn_blocking(move || -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // create engine and compile inside the blocking thread
        let engine = wasmer::Engine::default();
        let module = Module::new(&engine, &wasm_bytes.as_ref().unwrap()[..])?;

        // create pipe pair for stdout
        let (stdout_sender, mut stdout_reader) = Pipe::channel();

        {
            // create and configure the runner
            let mut runner = WasiRunner::new();
            runner.with_stdout(Box::new(stdout_sender));

            // run the module (blocking)
            runner.run_wasm(
                RuntimeOrEngine::Engine(engine),
                "hello",
                module,
                wasmer_types::ModuleHash::xxhash(&wasm_bytes.unwrap()),
            )?;
            // runner dropped here -> stdout pipe will be closed
        }

        // read all stdout (blocking read) and return
        let mut buf = String::new();
        stdout_reader.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await; // await the JoinHandle

    // handle spawn_blocking result
    let buf = match run_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            error!("wasm execution failed: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("wasm exec error: {}", e)).into_response();
        }
        Err(join_err) => {
            error!("wasm thread panicked: {}", join_err);
            return (StatusCode::INTERNAL_SERVER_ERROR, "wasm thread panicked".to_string()).into_response();
        }
    };


    let resp = SubmitResponse {
        job_id: JobId(Uuid::new_v4().to_string()),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    (StatusCode::CREATED, Json(resp)).into_response()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // hard-coded port for now
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    let app = Router::new().route("/submit", post(submit));

    // Wait for Ctrl+C and use it to trigger graceful shutdown
    let server = axum::Server::bind(&addr).serve(app.into_make_service());
    let graceful = server.with_graceful_shutdown(async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }
}