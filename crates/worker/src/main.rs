use std::{io::{Read, Write}, sync::Arc};

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    extract::Extension,
    Json, Router,
};
use mini_lambda_proto::{JobManifest, JobSubmission, SubmitResponse};
use serde_json::json;
use std::{net::SocketAddr, ops::Sub, path::PathBuf};
use tokio::fs;
use thiserror::Error;
use tracing::{error, info};
use uuid::Uuid;

use wasmer::{Module, Engine};
use wasmer_wasix::{
    runners::wasi::{WasiRunner, RuntimeOrEngine},
    Pipe
};

#[derive(Error, Debug)]
pub enum WasmRunError {
    #[error("WASM compilation error, the WASM module failed to compile: {0}")]
    Compile(String),
    #[error("WASM execution error during runtime: {0}")]
    Execution(String),
    #[error("WASM I/O error, failed to read captured stdout: {0}")]
    Io(#[from] std::io::Error)
}

async fn submit(
    Extension(engine): Extension<Arc<Engine>>,
    Json(data): Json<JobSubmission>,
) -> impl IntoResponse {

    if data.module_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty module".to_string()).into_response();
    }

    let job_id = Uuid::new_v4();

    // run the wasm execution in a blocking thread to not block the tokio runtime
    let run_result = tokio::task::spawn_blocking(move || -> Result<String, WasmRunError> {

        let module = Module::new(&engine, &data.module_bytes)
            .map_err(|e| WasmRunError::Compile(e.to_string()))?;

        // create pipe pair for stdout
        let (stdout_sender, mut stdout_reader) = Pipe::channel();
        {
            // create and configure the runner
            let mut runner = WasiRunner::new();
            runner.with_stdout(Box::new(stdout_sender));

            // run the module (blocking)
            runner.run_wasm(
                RuntimeOrEngine::Engine((*engine).clone()),
                &job_id.to_string(),
                module,
                wasmer_types::ModuleHash::xxhash(&data.module_bytes),
            ).map_err(|e| WasmRunError::Execution(e.to_string()))?;
        }

        // read output and return it
        let mut buf = String::new();
        stdout_reader.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await; // JoinHandle result

    // handle results
    let buf = match run_result {
        Ok(Ok(s)) => s,
        Ok(Err(exec_err)) => {
            match exec_err {
                WasmRunError::Compile(msg) => {
                    // compilation is a user's code problem -> 400
                    return (StatusCode::BAD_REQUEST, msg).into_response();
                }
                WasmRunError::Execution(msg) => {
                    return (StatusCode::UNPROCESSABLE_ENTITY, msg).into_response();
                }
                WasmRunError::Io(ioe) => {
                    error!("I/O while running wasm: {}", ioe);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "internal I/O error".to_string()).into_response();
                }
            }
        }
        Err(join_err) => {
            // the spawned thread panicked / was cancelled
            error!("wasm thread join failed: {}", join_err);
            return (StatusCode::INTERNAL_SERVER_ERROR, "wasm thread panicked".to_string()).into_response();
        }
    };


    let resp = SubmitResponse {
        job_id: Uuid::new_v4(),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    (StatusCode::CREATED, Json(resp)).into_response()
}



#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Create the shared engine
    let engine = Arc::new(Engine::default());

    // hard-coded port for now
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    let app = Router::new()
        .route("/submit", post(submit))
        .layer(Extension(engine)); // inject engine

    let server = axum::serve(listener, app);
    let graceful = server.with_graceful_shutdown(async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("failed to listen for ctrl_c: {}", e);
        }
    });

    if let Err(e) = graceful.await {
        error!("server error: {}", e);
    }
}