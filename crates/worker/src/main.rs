use std::{io::Read, sync::Arc, time::{Instant}};

use anyhow::anyhow;
use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    extract::Extension,
    Json, Router,
};
use mini_lambda_proto::{JobSubmission, SubmitResponse};
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
    // #[error("WASM compilation error, the WASM module failed to compile: {0}")]
    // Compile(String),

    #[error("WASM execution error during runtime: {0}")]
    Execution(String),

    #[error("WASM I/O error, failed to read captured stdout: {0}")]
    Io(#[from] std::io::Error),

    #[error("WASM thread failed to join (wasm thread panicked): {0}")]
    JoinError(#[from] tokio::task::JoinError)
}

/// Runs the inputted wasm module in a separate tokio spawn_blocking thread
async fn run_wasm_module(engine: Engine, module: Module) -> Result<String, WasmRunError> {
    let run_result = tokio::task::spawn_blocking(move || -> Result<String, WasmRunError> {

        // create pipe pair for stdout
        let (stdout_sender, mut stdout_reader) = Pipe::channel();
        {
            // create and configure the runner
            let mut runner = WasiRunner::new();
            runner.with_stdout(Box::new(stdout_sender));

            // run the module (blocking)
            runner.run_wasm(
                RuntimeOrEngine::Engine(engine),
                "temp", // TODO: replace name
                module,
                wasmer_types::ModuleHash::random()
            ).map_err(|e| WasmRunError::Execution(e.to_string()))?;
        }

        // read output and return it
        let mut buf = String::new();
        stdout_reader.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await?; // JoinHandle result

    return run_result;

}

async fn submit(
    Extension(engine): Extension<Arc<Engine>>,
    Json(data): Json<JobSubmission>,
) -> impl IntoResponse {

    if data.module_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty wasm module".to_string()).into_response();
    }

    let job_id = Uuid::new_v4();

    info!("running job with id {}", job_id);

    let module = match Module::new(&engine, &data.module_bytes) {
        Ok(m) => m,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("failed to compile wasm module: {}", e)).into_response(),
    };

    let run_result = run_wasm_module((*engine).clone(), module).await;

    // handle results
    let buf = match run_result {
        Ok(s) => s,
        Err(exec_err) => {
            match exec_err {
                WasmRunError::Execution(msg) => {
                    return (StatusCode::UNPROCESSABLE_ENTITY, msg).into_response();
                }
                WasmRunError::Io(ioe) => {
                    error!("I/O while running wasm: {}", ioe);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "internal I/O error".to_string()).into_response();
                },
                WasmRunError::JoinError(je) => {
                    error!("wasm thread join failed: {}", je);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "wasm thread panicked".to_string()).into_response();
                }
            }
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