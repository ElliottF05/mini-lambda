mod module_cache;

use std::{io::{Read,Write}, sync::Arc};
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
use clap::Parser;
use reqwest::Client;
use mini_lambda_proto::{RegisterWorkerRequest, RegisterWorkerResponse};
use uuid::Uuid;

use wasmer::{Module, Engine};
use wasmer_wasix::{
    runners::wasi::{WasiRunner, RuntimeOrEngine},
    Pipe
};

use crate::module_cache::ModuleCache;

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("WASM compilation error: {0}")]
    Compile(String),

    #[error("WASM execution error during runtime: {0}")]
    Execution(String),

    #[error("module not found in cache: {0}")]
    ModuleNotFound(String),

    #[error("WASM I/O error, failed to read captured stdout: {0}")]
    Io(#[from] std::io::Error),

    #[error("WASM thread failed to join (wasm thread panicked): {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

impl WorkerError {
    pub fn to_http_response(&self) -> (StatusCode, String) {
        match self {
            WorkerError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            WorkerError::Compile(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            WorkerError::Execution(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            WorkerError::ModuleNotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            WorkerError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal I/O error".to_string()),
            WorkerError::JoinError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "wasm thread panicked".to_string()),
        }
    }
}

impl IntoResponse for WorkerError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = self.to_http_response();
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

/// Runs the inputted wasm module in a separate tokio spawn_blocking thread
async fn run_wasm_module(engine: Engine, module: Module) -> Result<String, WorkerError> {
    let run_result = tokio::task::spawn_blocking(move || -> Result<String, WorkerError> {

        // create pipe pair for stdout
        let (stdout_sender, mut stdout_reader) = Pipe::channel();

        {
            // create and configure the runner
            let mut runner = WasiRunner::new();
            runner
                .with_stdout(Box::new(stdout_sender)) // use stdout to capture output
                .with_args(vec!["10"]); // send args

            // run the module (blocking)
            runner.run_wasm(
                RuntimeOrEngine::Engine(engine),
                "temp", // TODO: replace name
                module,
                wasmer_types::ModuleHash::random()
            ).map_err(|e| WorkerError::Execution(e.to_string()))?;
        }

        // read output and return it
        let mut buf = String::new();
        stdout_reader.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await?; // JoinHandle result

    return run_result;

}

async fn handle_submit_wasm(
    Extension(engine): Extension<Arc<Engine>>,
    Extension(module_cache): Extension<Arc<tokio::sync::Mutex<ModuleCache>>>,
    Json(data): Json<JobSubmissionWasm>,
) -> Result<(StatusCode, Json<SubmitResponse>), WorkerError> {

    if data.module_bytes.is_empty() {
        return Err(WorkerError::Validation("empty wasm module".into()));
    }

    let module = Module::new(&engine, &data.module_bytes).map_err(|e| WorkerError::Compile(e.to_string()))?;
    let module = Arc::new(module);

    let module_hash = hash_wasm_module(&data.module_bytes);

    let mut module_cache = module_cache.lock().await;
    module_cache.put(module_hash, module.clone());

    let buf = run_wasm_module((*engine).clone(), (*module).clone()).await?;

    let resp = SubmitResponse {
        job_id: Uuid::new_v4(),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

async fn handle_submit_hash(
    Extension(engine): Extension<Arc<Engine>>,
    Extension(module_cache): Extension<Arc<tokio::sync::Mutex<ModuleCache>>>,
    Json(data): Json<JobSubmissionHash>,
) -> Result<(StatusCode, Json<SubmitResponse>), WorkerError> {
    
    if data.module_hash.is_empty() {
        return Err(WorkerError::Validation("empty module hash".into()));
    }

    let mut module_cache = module_cache.lock().await;
    let module = module_cache.get(&data.module_hash).ok_or_else(|| WorkerError::ModuleNotFound(data.module_hash.clone()))?;

    let buf = run_wasm_module((*engine).clone(), (*module).clone()).await?;

    let resp = SubmitResponse {
        job_id: Uuid::new_v4(),
        message: Some(format!("job accepted, output: {}", buf)),
    };

    Ok((StatusCode::CREATED, Json(resp)))
}


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    #[derive(Parser)]
    struct Opts {
        /// Orchestrator base URL, e.g. http://127.0.0.1:8080
        #[arg(long)]
        orchestrator: String,
    }

    let opts = Opts::parse();

    // Create the shared engine
    let engine = Arc::new(Engine::default());

    // Create the module cache
    let module_cache = Arc::new(tokio::sync::Mutex::new(ModuleCache::new()));

    // bind to an OS-assigned port so multiple workers can run on the same host
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    let port = local.port();

    // register with orchestrator
    let base = if opts.orchestrator.starts_with("http://") || opts.orchestrator.starts_with("https://") {
        opts.orchestrator.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", opts.orchestrator.trim_end_matches('/'))
    };

    let register_url = format!("{}/register_worker", base);
    info!("registering with orchestrator at {}", register_url);

    let req = RegisterWorkerRequest { port };

    let client = Client::new();
    match client.post(&register_url).json(&req).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let _body: RegisterWorkerResponse = resp.json().await.unwrap_or(RegisterWorkerResponse { worker_id: Uuid::new_v4() });
                tracing::info!("registered with orchestrator {}", register_url);
            } else {
                error!("failed to register with orchestrator: {} -> status {}", register_url, resp.status());
            }
        }
        Err(e) => {
            error!("failed to register with orchestrator {}: {}", register_url, e);
        }
    }

    // build and serve app using the already-bound listener
    let app = Router::new()
        .route("/submit_wasm", post(handle_submit_wasm))
        .route("/submit_hash", post(handle_submit_hash))
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