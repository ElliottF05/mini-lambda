pub mod shared {
    tonic::include_proto!("shared");
}

pub mod client_api {
    tonic::include_proto!("client_api");
}

pub mod worker_api {
    tonic::include_proto!("worker_api");
}

pub mod executor {
    tonic::include_proto!("executor");
}

use std::usize;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use shared::*;
pub use client_api::*;
pub use worker_api::*;
pub use executor::*;

/// JWT claims used to authorize a client's access to a specific job on a Worker.
/// The sub field holds the job_id that this token is bound to.
#[derive(Serialize, Deserialize)]
pub struct JobClaims {
    pub sub: Uuid,
    pub exp: usize,
}

impl JobClaims {
    /// Create a new JobClaims for a job with a given job_id.
    /// Has an infinite expiration.
    pub fn new(job_id: Uuid) -> Self {
        Self { 
            sub: job_id,
            exp: usize::MAX
        }
    }
}
