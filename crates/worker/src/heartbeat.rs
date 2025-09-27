use std::{sync::{atomic::{AtomicUsize, Ordering}, Arc}};

use mini_lambda_proto::HeartbeatUpdate;
use reqwest::Client;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{errors::WorkerError, HEARTBEAT_INTERVAL_MS, MAX_CREDITS};

pub async fn send_heartbeat(
    client: &Client,
    base: &str,
    worker_id: Uuid,
    num_jobs: Arc<AtomicUsize>,
    seq: Arc<AtomicUsize>,
) -> Result<(), WorkerError> {

    let new_seq = seq.fetch_add(1, Ordering::SeqCst).saturating_add(1);

    let credits = MAX_CREDITS - num_jobs.load(Ordering::SeqCst);

    let url = format!("{}/heartbeat", base);
    let req = HeartbeatUpdate { worker_id, credits, seq: new_seq };

    let resp = client.post(&url).json(&req).send().await.map_err(|e| {
        WorkerError::Heartbeat(format!("failed to send heartbeat: {}", e))
    })?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(WorkerError::Heartbeat(format!(
            "sent heartbeat but not accepted by orchestrator, orchestrator responded with {}",
            resp.status()
        )))
    }
}

/// Start a background heartbeat loop. Returns a oneshot sender that is used to stop it on shutdown.
pub fn start_heartbeat_loop(
    client: Client,
    base: String,
    worker_id: Uuid,
    num_jobs: Arc<AtomicUsize>,
    seq: Arc<AtomicUsize>,
) -> oneshot::Sender<()> {
    let (tx, mut rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(HEARTBEAT_INTERVAL_MS));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = send_heartbeat(&client, &base, worker_id, num_jobs.clone(), seq.clone()).await {
                        tracing::warn!("heartbeat failed: {}", e);
                    }
                }
                _ = &mut rx => {
                    // shutdown requested
                    tracing::info!("heartbeat loop shutting down");
                    break;
                }
            }
        }
    });
    tx
}