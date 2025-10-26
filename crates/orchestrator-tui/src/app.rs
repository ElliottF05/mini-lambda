use chrono::{DateTime, Utc};
use mini_lambda_proto::{JobSummary, WorkerInfo};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DispatchRecord {
    pub when: DateTime<Utc>,
    pub worker_endpoint: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct DispatchedJob {
    pub job_id: Uuid,
    pub when: DateTime<Utc>,
    pub worker: String,
}

pub struct App {
    pub workers: Vec<WorkerInfo>,
    pub pending: Vec<JobSummary>,
    pub dispatches: VecDeque<DispatchRecord>,
    pub dispatched_jobs: VecDeque<DispatchedJob>,
    pub prev_credits: HashMap<Uuid, usize>,
    pub last_error: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            workers: Vec::new(),
            pending: Vec::new(),
            dispatches: VecDeque::with_capacity(100),
            dispatched_jobs: VecDeque::with_capacity(200),
            prev_credits: HashMap::new(),
            last_error: None,
        }
    }

    pub fn record_credit_delta(&mut self, new_workers: &[WorkerInfo]) {
        let now = Utc::now();
        for w in new_workers {
            let prev = self.prev_credits.get(&w.id).cloned().unwrap_or(w.credits);
            if w.credits < prev {
                let delta = prev - w.credits;
                self.dispatches.push_front(DispatchRecord {
                    when: now,
                    worker_endpoint: w.endpoint.clone(),
                    count: delta,
                });
                while self.dispatches.len() > 100 {
                    self.dispatches.pop_back();
                }
            }
            self.prev_credits.insert(w.id, w.credits);
        }
    }

    /// Process a new snapshot from the orchestrator: update worker info, detect pending->dispatched
    /// transitions and record dispatched jobs. `new_workers` and `new_pending` are taken from the
    /// latest monitoring response.
    pub fn process_snapshot(&mut self, new_workers: Vec<WorkerInfo>, new_pending: Vec<JobSummary>) {
        // First update dispatch counts from worker credit deltas
        self.record_credit_delta(&new_workers);

        // Determine which job IDs were removed from pending (i.e., dispatched)
        let prev_ids: std::collections::HashSet<Uuid> = self.pending.iter().map(|j| j.job_id).collect();
        let new_ids: std::collections::HashSet<Uuid> = new_pending.iter().map(|j| j.job_id).collect();
        let removed: Vec<Uuid> = prev_ids.difference(&new_ids).cloned().collect();

        // Assign removed jobs to recent dispatch records (consume counts) so we can attribute worker endpoints
        let now = Utc::now();
        for job_id in removed {
            let mut assigned_worker = "-".to_string();
            // consume from the most recent dispatch record (front)
            while let Some(mut dr) = self.dispatches.pop_front() {
                if dr.count > 0 {
                    assigned_worker = dr.worker_endpoint.clone();
                    dr.count -= 1;
                    // if there are remaining counts, push it back to front
                    if dr.count > 0 {
                        self.dispatches.push_front(dr);
                    }
                    break;
                }
                // if count was zero, continue to next
            }

            self.dispatched_jobs.push_front(DispatchedJob { job_id, when: now, worker: assigned_worker });
            // cap size
            while self.dispatched_jobs.len() > 500 {
                self.dispatched_jobs.pop_back();
            }
        }

        // replace workers and pending with latest snapshot
        self.workers = new_workers;
        self.pending = new_pending;
    }
}
