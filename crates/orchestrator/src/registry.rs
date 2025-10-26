use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::SystemTime};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub endpoint: String,
    pub id: Uuid,
    pub credits: usize,
    pub seq: usize,
    pub last_seen: SystemTime
}

/// A small wrapper around the in-memory worker registry used by the orchestrator.
#[derive(Clone, Default)]
pub struct WorkerRegistry {
    inner: Arc<Mutex<HashMap<Uuid, WorkerInfo>>>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Register a new worker endpoint and return the assigned worker id.
    pub async fn register(&self, endpoint: String, credits: usize) -> Uuid {
        let id = Uuid::new_v4();
        let info = WorkerInfo { 
            endpoint, 
            id, 
            credits, 
            seq: 0,
            last_seen: SystemTime::now()
        };
        self.inner.lock().await.insert(id, info);
        id
    }

    /// Unregister a worker by id. Returns true if the worker existed and was unregistered.
    pub async fn unregister(&self, worker_id: Uuid) -> bool {
        self.inner.lock().await.remove(&worker_id).is_some()
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Update the approximate credits for a worker. Returns true if the worker existed.
    pub async fn update_credits(&self, worker_id: Uuid, credits: usize) -> bool {
        let mut m = self.inner.lock().await;
        if let Some(w) = m.get_mut(&worker_id) {
            w.credits = credits;
            true
        } else {
            false
        }
    }

    /// Get worker info by id.
    pub async fn get_worker_info(&self, worker_id: Uuid) -> Option<WorkerInfo> {
        let m = self.inner.lock().await;
        m.get(&worker_id).cloned()
    }

    /// Choose the worker with the most credits, decrement its credits count and return (id, endpoint).
    pub async fn pick_and_decrement(&self) -> Option<(Uuid, String)> {
        let mut m = self.inner.lock().await;
        if m.is_empty() {
            return None;
        }

        let mut chosen_id: Option<Uuid> = None;
        let mut greatest: Option<usize> = None;
        for (id, info) in m.iter() {
            if info.credits > 0 && (greatest.is_none() || info.credits > greatest.unwrap()) {
                greatest = Some(info.credits);
                chosen_id = Some(*id);
            }
        }

        if chosen_id.is_none() {
            return None;
        }

        let id = chosen_id.expect("non-empty checked");
        // clone endpoint before mutating
        let endpoint = m.get(&id).unwrap().endpoint.clone();
        // decrement approximate credits
        m.get_mut(&id).unwrap().credits -= 1;

        Some((id, endpoint))
    }

    /// Expose a read-only snapshot of the registry for debugging/testing.
    pub async fn snapshot(&self) -> HashMap<Uuid, WorkerInfo> {
        self.inner.lock().await.clone()
    }
}



// Tests for WorkerRegistry
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_snapshot_contains_worker() {
        let reg = WorkerRegistry::new();
        let id = reg.register("http://1.2.3.4:1000".to_string(), 10).await;
        let snap = reg.snapshot().await;
        assert!(snap.contains_key(&id));
        let info = snap.get(&id).unwrap();
        assert_eq!(info.endpoint, "http://1.2.3.4:1000");
        assert_eq!(info.credits, 10);
    }

    #[tokio::test]
    async fn pick_and_decrement_chooses_largest_and_decrements() {
        let reg = WorkerRegistry::new();
        let id1 = reg.register("http://a:1".to_string(), 10).await;
        let id2 = reg.register("http://b:2".to_string(), 10).await;

        // make second worker have less credits
        assert!(reg.update_credits(id2, 1).await);

        // pick should choose id1 (more credits)
        let picked = reg.pick_and_decrement().await;
        assert!(picked.is_some());
        let (picked_id, endpoint) = picked.unwrap();
        assert_eq!(picked_id, id1);
        assert_eq!(endpoint, "http://a:1");

        // snapshot verifies credits decremented for id1
        let snap = reg.snapshot().await;
        assert_eq!(snap.get(&id1).unwrap().credits, 9);
    }

    #[tokio::test]
    async fn unregister_and_update_credits_behavior() {
        let reg = WorkerRegistry::new();
        let id = reg.register("http://x:1".to_string(), 10).await;
        assert!(reg.update_credits(id, 3).await);
        assert!(reg.unregister(id).await);
        // further updates should fail since worker is gone
        assert!(!reg.update_credits(id, 1).await);
        let snap = reg.snapshot().await;
        assert!(!snap.contains_key(&id));
    }

    #[tokio::test]
    async fn pick_on_empty_registry_returns_none() {
        let reg = WorkerRegistry::new();
        assert!(reg.pick_and_decrement().await.is_none());
    }
}
