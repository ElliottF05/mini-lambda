use std::net::SocketAddr;

use tokio::net::TcpListener;
use tonic::transport::Server;
use tonic::{Request, Status, Response};

use shared::executor_server::{Executor, ExecutorServer};
use shared::{JobRequest, JobResponse};

#[derive(Debug)]
struct Worker {
    addr: SocketAddr,
}

#[tonic::async_trait]
impl Executor for Worker {

    async fn execute_job(
        &self, 
        _request: Request<JobRequest>
    ) -> Result<Response<JobResponse>, Status> {

        let response = JobResponse {
            result: format!("Job executed by worker at {}", self.addr).into_bytes(),
        };

        return Ok(Response::new(response));
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let worker = Worker { addr };
    
    println!("Worker listening on {}", addr);
    Server::builder()
        .add_service(ExecutorServer::new(worker))
        .serve_with_incoming(incoming)
        .await?;

    Ok(())
}