use jsonwebtoken::{EncodingKey, Header};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status, Response, Streaming};

use shared::{CreditUpdate, JobClaims, OrchestratorMessage, RegistrationAck, WorkerMessage, WorkerResponse, orchestrator_message, worker_message};
use shared::worker_api_server::WorkerApi;

use crate::job_queue::JobQueue;
use crate::orchestrator::Orchestrator;
use crate::registry::WorkerRegistry;

/// Implementation of the WorkerApi service for Orchestrator.
#[tonic::async_trait]
impl WorkerApi for Orchestrator {
    type ConnectWorkerStream = ReceiverStream<Result<OrchestratorMessage, Status>>;

    /// A function exposed by the Orchestrator that Worker instances call to connect
    /// to this Orchestrator.
    async fn connect_worker(
        &self,
        request: Request<Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::ConnectWorkerStream>, Status> {

        // Extract inbound stream and create outbound channel
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(32);

        // Spawn a task to handle the bidirectional communication
        let orchestrator = self.clone(); // Clone for move into async task
        tokio::spawn(async move {
            tracing::debug!("waiting for worker registration");
            // Expect a registration as the first message
            let worker_address = match inbound.message().await {
                Ok(Some(WorkerMessage { message: Some(worker_message::Message::Registration(registration)) })) => {
                    if !orchestrator.handle_worker_registration(tx.clone(), &registration).await {
                        tracing::warn!(worker = %registration.address, "failed to handle worker registration");
                        return;
                    };
                    registration.address
                },
                Ok(Some(m)) => {
                    tracing::error!("ERROR: should always receive registration as first message, got {:?}", m);
                    std::process::exit(1);
                },
                Ok(None) => {
                    tracing::warn!("worker disconnected before sending registration");
                    return;
                },
                Err(e) => {
                    tracing::warn!(error = ?e, "stream error before worker registration");
                    return;
                }
            };

            tracing::info!(worker = %worker_address, "worker registered");
            loop {
                match inbound.message().await {
                    Ok(Some(worker_message)) => {
                        match worker_message.message {
                            Some(worker_message::Message::CreditUpdate(credit_update)) => {
                                orchestrator.handle_credit_update(&worker_address, credit_update).await;
                            },
                            Some(worker_message::Message::Registration(_)) => {
                                tracing::error!(worker = %worker_address, "ERROR: worker sent a second registration after already registering, this should never happen");
                                std::process::exit(1);
                            },
                            None => {
                                tracing::error!(worker = %worker_address, "ERROR: worker sent a message with no content, this should never happen");
                                std::process::exit(1);
                            }
                        }
                    },
                    Ok(None) => {
                        tracing::info!(worker = %worker_address, "worker disconnected");
                        break;
                    },
                    Err(e) => {
                        tracing::warn!(worker = %worker_address, error = ?e, "worker stream error, deregistering");
                        break;
                    }
                }
            }
            orchestrator.registry.lock().await.deregister_worker(&worker_address);
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}


// Helper methods for the WorkerApi implementation.
// They may use the outbound tx channel to send messages back to the worker.
type OutboundTx = mpsc::Sender<Result<OrchestratorMessage, Status>>;
impl Orchestrator {
    /// Handles an incoming Worker registration message.
    async fn handle_worker_registration(&self, tx: OutboundTx, registration: &shared::WorkerRegistration) -> bool {
        tracing::debug!(worker = %registration.address, credits = registration.credits, "handling worker registration");
        {
            let mut queue = self.job_queue.lock().await;
            let mut registry = self.registry.lock().await;

            registry.register_worker(registration.address.to_owned(), registration.credits);
            Self::dispatch_pending_jobs(&mut queue, &mut registry, &self.jwt_secret);
        }

        // Send registration ack back to worker
        let ack = OrchestratorMessage {
            message: Some(orchestrator_message::Message::RegistrationAck(
                RegistrationAck { jwt_secret: self.jwt_secret.to_vec() }
            ))
        };
        if tx.send(Ok(ack)).await.is_err() {
            tracing::error!(worker = %registration.address, "failed to send registration ack, deregistering");
            self.registry.lock().await.deregister_worker(&registration.address);
            false
        } else {
            tracing::info!(worker = %registration.address, "registration ack sent to worker");
            true
        }
    }

    /// Handles a credit update from a Worker, updating its available credits in the registry
    /// and dispatching any pending jobs that can now be served.
    async fn handle_credit_update(&self, worker_address: &str, credit_update: CreditUpdate) {
        tracing::debug!(worker = %worker_address, delta = credit_update.delta, "credit update received");
        let mut queue = self.job_queue.lock().await;
        let mut registry = self.registry.lock().await;

        registry.update_credits(worker_address, credit_update.delta);
        Self::dispatch_pending_jobs(&mut queue, &mut registry, &self.jwt_secret);
    }

    /// Dispatches as many pending jobs as possible to available workers, consuming one registry
    /// credit per job. Stops when the queue is empty or no credits remain.
    /// The caller must hold write guards on both the queue and registry for the duration.
    pub fn dispatch_pending_jobs(queue: &mut JobQueue, registry: &mut WorkerRegistry, jwt_secret: &[u8]) {
        while registry.has_available_credits() {
            match queue.dequeue() {
                Some((job_id, tx)) => {
                    let worker_address = registry.get_worker()
                        .unwrap_or_else(|| {
                            tracing::error!("ERROR: worker availability in registry should be guaranteed by has_available_credits() above");
                            std::process::exit(1);
                        });
                    let header = Header::default();
                    let job_claims = JobClaims::new(job_id);
                    let key = EncodingKey::from_secret(jwt_secret);
                    let jwt_token = jsonwebtoken::encode(&header, &job_claims, &key)
                        .unwrap_or_else(|e| {
                            tracing::error!(error = %e, "ERROR: jwt encoding failed, this should not happen");
                            std::process::exit(1);
                        });

                    tracing::debug!(job_id = %job_id, worker = %worker_address, "job dispatched to worker");
                    if tx.send(WorkerResponse { worker_address: worker_address.clone(), jwt_token }).is_err() {
                        registry.update_credits(&worker_address, 1);
                    }
                },
                None => break
            }
        }
    }
}

/// Interceptor that verifies the authorization header matches the configured worker password.
/// Returns Unauthenticated if the password is wrong or missing. No-op if no password is configured.
pub fn check_worker_auth(orchestrator: Orchestrator) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    let password = orchestrator.worker_password.clone();
    move |req: Request<()>| {
        if let Some(expected) = &password {
            let actual = req.metadata().get("authorization")
                .and_then(|v| v.to_str().ok());
            if actual != Some(expected) {
                return Err(Status::unauthenticated("invalid worker password"));
            }
        }
        Ok(req)
    }
}