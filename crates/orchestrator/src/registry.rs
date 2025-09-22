use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub endpoint: String,
    pub id: Uuid,
    pub queue_len: usize,
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

    pub fn from_map(map: HashMap<Uuid, WorkerInfo>) -> Self {
        Self { inner: Arc::new(Mutex::new(map)) }
    }

    /// Register a new worker endpoint and return the assigned worker id.
    pub async fn register(&self, endpoint: String) -> Uuid {
        let id = Uuid::new_v4();
        let info = WorkerInfo { endpoint, id, queue_len: 0 };
        self.inner.lock().await.insert(id, info);
        id
    }

    /// Unregister a worker by id. Returns true if the worker existed and was unregistered.
    pub async fn unregister(&self, worker_id: Uuid) -> bool {
        self.inner.lock().await.remove(&worker_id).is_some()
    }

    /// Update the approximate queue length for a worker. Returns true if the worker existed.
    pub async fn update_queue(&self, worker_id: Uuid, queue_len: usize) -> bool {
        let mut m = self.inner.lock().await;
        if let Some(w) = m.get_mut(&worker_id) {
            w.queue_len = queue_len;
            true
        } else {
            false
        }
    }

    /// Choose the worker with the smallest queue length, increment its queue count and return (id, endpoint).
    pub async fn pick_and_increment(&self) -> Option<(Uuid, String)> {
        let mut m = self.inner.lock().await;
        if m.is_empty() {
            return None;
        }

        let mut chosen_id: Option<Uuid> = None;
        let mut smallest: Option<usize> = None;
        for (id, info) in m.iter() {
            if smallest.is_none() || info.queue_len < smallest.unwrap() {
                smallest = Some(info.queue_len);
                chosen_id = Some(*id);
            }
        }

        let id = chosen_id.expect("non-empty checked");
        // clone endpoint before mutating
        let endpoint = m.get(&id).unwrap().endpoint.clone();
        // increment approximate queue length
        m.get_mut(&id).unwrap().queue_len += 1;

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
        let id = reg.register("http://1.2.3.4:1000".to_string()).await;
        let snap = reg.snapshot().await;
        assert!(snap.contains_key(&id));
        let info = snap.get(&id).unwrap();
        assert_eq!(info.endpoint, "http://1.2.3.4:1000");
        assert_eq!(info.queue_len, 0);
    }

    #[tokio::test]
    async fn pick_and_increment_chooses_smallest_and_increments() {
        let reg = WorkerRegistry::new();
        let id1 = reg.register("http://a:1".to_string()).await;
        let id2 = reg.register("http://b:2".to_string()).await;

        // make second busier
        assert!(reg.update_queue(id2, 10).await);

        // pick should choose id1 (smaller queue)
        let picked = reg.pick_and_increment().await;
        assert!(picked.is_some());
        let (picked_id, endpoint) = picked.unwrap();
        assert_eq!(picked_id, id1);
        assert_eq!(endpoint, "http://a:1");

        // snapshot verifies queue_len incremented for id1
        let snap = reg.snapshot().await;
        assert_eq!(snap.get(&id1).unwrap().queue_len, 1);
    }

    #[tokio::test]
    async fn unregister_and_update_queue_behaviour() {
        let reg = WorkerRegistry::new();
        let id = reg.register("http://x:1".to_string()).await;
        assert!(reg.update_queue(id, 3).await);
        assert!(reg.unregister(id).await);
        // further updates should fail since worker is gone
        assert!(!reg.update_queue(id, 1).await);
        let snap = reg.snapshot().await;
        assert!(!snap.contains_key(&id));
    }

    #[tokio::test]
    async fn pick_on_empty_registry_returns_none() {
        let reg = WorkerRegistry::new();
        assert!(reg.pick_and_increment().await.is_none());
    }
}
