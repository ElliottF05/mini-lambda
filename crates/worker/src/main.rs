use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::Server;
use tonic::{Request, Status, Response};
use wasmer::Module;
use wasmer_wasix::runners::wasi::{RuntimeOrEngine, WasiRunner};
use wasmer_wasix::{Pipe, PluggableRuntime};
use wasmer_wasix::runtime::task_manager::tokio::TokioTaskManager;

use shared::executor_server::{Executor, ExecutorServer};
use shared::{JobRequest, JobResponse, WorkerMessage, WorkerRegistration};
use shared::worker_api_client::WorkerApiClient;
use shared::orchestrator_message;
use shared::worker_message;

#[derive(Debug)]
struct Worker {
    addr: SocketAddr,
    wasm_runtime: PluggableRuntime,
    // TODO, look into wasmer's built-in caching: 
    // https://docs.rs/crate/wasmer-wasix/latest/source/src/runtime/module_cache/mod.rs
}

impl Worker {
    pub fn new(addr: SocketAddr) -> Worker {
        let tokio_task_manager = TokioTaskManager::new(tokio::runtime::Handle::current());
        let runtime = PluggableRuntime::new(Arc::new(tokio_task_manager));
        return Worker {
            addr,
            wasm_runtime: runtime
        }
    }
}

#[tonic::async_trait]
impl Executor for Worker {

    async fn execute_job(
        &self, 
        request: Request<JobRequest>
    ) -> Result<Response<JobResponse>, Status> {
        println!("Received job to execute...");

        let wasm_bytes = request.into_inner().wasm_bytes;
        let runtime = self.wasm_runtime.clone();

        // Run the compilation and wasm execution on a separate blocking task
        // TODO: handle errors everywhere in here instead of unwrapping
        let output_buf = tokio::task::spawn_blocking(move || {
            // Compile the wasm module
            let wasm_module = Module::new(&runtime.engine, wasm_bytes).unwrap();

            // Create pipes to capture stdout
            let (stdout_tx, mut stdout_rx) = Pipe::channel();
            {
                // Configure the runtime environment and run it
                let mut runner = WasiRunner::new();
                runner.with_stdout(Box::new(stdout_tx));
                // TODO: add args, file system, etc

                runner.run_wasm(
                    RuntimeOrEngine::Runtime(Arc::new(runtime)), 
                    "unnamed", 
                    wasm_module, 
                    wasmer_types::ModuleHash::random()
                ).unwrap();
            }

            // Capture stdout
            let mut output_buf = vec![];
            stdout_rx.read_to_end(&mut output_buf).unwrap();

            output_buf
        }).await.unwrap();

        let response = JobResponse {
            result: output_buf,
        };
        return Ok(Response::new(response));
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let worker = Worker::new(addr);
    
    // start the executor server
    println!("Worker listening on {}", addr);
    let _server_handle = tokio::spawn(async move {
        let incoming = TcpListenerStream::new(listener);
        Server::builder()
            .add_service(ExecutorServer::new(worker))
            .serve_with_incoming(incoming)
            .await
    });
    

    // register this worker with the orchestrator
    let orchestrator_url = "http://127.0.0.1:50051";
    let mut client = WorkerApiClient::connect(orchestrator_url).await?;

    // set up channel for streaming
    let (tx, rx) = mpsc::channel(32);
    let outbound = ReceiverStream::new(rx);

    // connect and get the response stream
    let response = client.connect_worker(Request::new(outbound)).await?;
    let mut inbound = response.into_inner();

    // spawn a task to handle incoming messages from the orchestrator
    tokio::spawn(async move {
        while let Some(result) = inbound.next().await {
            match result {
                Ok(orchestrator_msg) => {
                    match orchestrator_msg.message {
                        Some(orchestrator_message::Message::RegistrationAck(ack)) => {
                            println!("Received registration ack: {:?}", ack);
                        },
                        None => {
                            println!("Received empty message from orchestrator");
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Error receiving message from orchestrator: {:?}", e);
                    break;
                }
            }
        }
    });

    // send the initial registration message
    tx.send(WorkerMessage {
        message: Some(worker_message::Message::Registration(WorkerRegistration { address: addr.to_string() }))
    }).await?;

    // keep the main task alive
    tokio::signal::ctrl_c().await?;

    Ok(())
}