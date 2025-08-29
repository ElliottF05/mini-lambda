use serde::{Deserialize, Serialize};

/// Minimal JobId type (string wrapper)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobId(pub String);

/// What client sends as metadata with a module (very small manifest)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobManifest {
    pub args: Vec<String>,
}

/// Response returned by orchestrator when a job is accepted
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub job_id: JobId,
    pub message: Option<String>,
}