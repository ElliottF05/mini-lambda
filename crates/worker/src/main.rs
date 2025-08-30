mod module_cache;

use std::{io::Read, sync::Arc, time::{Instant}};

use anyhow::anyhow;
use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    extract::Extension,
    Json, Router,
};
use mini_lambda_proto::{hash_wasm_module, JobSubmissionHash, JobSubmissionWasm, SubmitResponse};
use thiserror::Error;
use tracing::{error, info};
use uuid::Uuid;

use wasmer::{Module, Engine};
use wasmer_wasix::{
    runners::wasi::{WasiRunner, RuntimeOrEngine},
    Pipe
};

use crate::module_cache::ModuleCache;

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

impl WasmRunError {
    pub fn to_http_response(&self) -> (StatusCode, String) {
        match self {
            WasmRunError::Execution(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            WasmRunError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal I/O error".to_string()),
            WasmRunError::JoinError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "wasm thread panicked".to_string()),
        }
    }
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

async fn submit_wasm(
    Extension(engine): Extension<Arc<Engine>>,
    Extension(module_cache): Extension<Arc<tokio::sync::Mutex<ModuleCache>>>,
    Json(data): Json<JobSubmissionWasm>,
) -> impl IntoResponse {

    if data.module_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty wasm module".to_string()).into_response();
    }

    let module = match Module::new(&engine, &data.module_bytes) {
        Ok(m) => Arc::new(m),
        Err(e) => return (StatusCode::BAD_REQUEST, format!("failed to compile wasm module: {}", e)).into_response(),
    };

    let module_hash = hash_wasm_module(&data.module_bytes);

    let mut module_cache = module_cache.lock().await;
    module_cache.put(module_hash, module.clone());

    let run_result = run_wasm_module((*engine).clone(), (*module).clone()).await;

    // handle results
    let buf = match run_result {
        Ok(s) => s,
        Err(exec_err) => return exec_err.to_http_response().into_response(),
    };

    let resp = SubmitResponse {
        job_id: Uuid::new_v4(),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    (StatusCode::CREATED, Json(resp)).into_response()
}

async fn submit_hash(
    Extension(engine): Extension<Arc<Engine>>,
    Extension(module_cache): Extension<Arc<tokio::sync::Mutex<ModuleCache>>>,
    Json(data): Json<JobSubmissionHash>,
) -> impl IntoResponse {
    
    if data.module_hash.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty module hash".to_string()).into_response();
    }

    let mut module_cache = module_cache.lock().await;
    let module = match module_cache.get(&data.module_hash) {
        Some(m) => m,
        None => return (StatusCode::NOT_FOUND, "module not found in cache, please submit the full wasm module".to_string()).into_response(),
    };

    let run_result = run_wasm_module((*engine).clone(), (*module).clone()).await;

    // handle results
    let buf = match run_result {
        Ok(s) => s,
        Err(exec_err) => return exec_err.to_http_response().into_response(),
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

    // Create the module cache
    let module_cache = Arc::new(tokio::sync::Mutex::new(ModuleCache::new()));

    // hard-coded port for now
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    let app = Router::new()
        .route("/submit_wasm", post(submit_wasm))
        .route("/submit_hash", post(submit_hash))
        .layer(Extension(engine)) // inject engine
        .layer(Extension(module_cache)); // inject module cache

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