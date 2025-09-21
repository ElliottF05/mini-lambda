use serde::{Deserialize, Serialize, Deserializer, Serializer};
use uuid::Uuid;
use sha2::{Digest, Sha256};

use base64::{engine::general_purpose, Engine as _};

/// What client sends as metadata with a module
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobManifest {
    pub call_args: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobSubmissionHash {
    pub module_hash: String, // hex-encoded sha256 hash of the module bytes
    pub manifest: JobManifest,
}

/// What client sends as payload to be executed (includes WASM binary module and JobManifest metadata).
/// This contains the WASM binary itself, there also exists JobSubmissionHash which just sends a hash
/// over, avoiding the need to send the entire binary (if the module is cached on the worker).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobSubmissionWasm {
    #[serde(
        serialize_with = "serialize_base64",
        deserialize_with = "deserialize_base64"
    )]
    pub module_bytes: Vec<u8>,
    pub manifest: JobManifest,
}

fn serialize_base64<S>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let enc = general_purpose::STANDARD.encode(bytes);
    s.serialize_str(&enc)
}

fn deserialize_base64<'de, D>(d: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    general_purpose::STANDARD
        .decode(&s)
        .map_err(serde::de::Error::custom)
}

/// Response returned by orchestrator when a job is accepted
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub job_id: Uuid,
    pub message: Option<String>,
}

pub fn hash_wasm_module(module_bytes: &[u8]) -> String {
    return hex::encode(Sha256::digest(module_bytes));
}

/// Worker registration request used by workers to tell the orchestrator which port they'll listen on.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterWorkerRequest {
    /// The port the worker is listening on (worker may have bound to port 0).
    pub port: u16,
}

/// Worker unregistration request used by workers to tell the orchestrator they are shutting down.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnregisterWorkerRequest {
    /// The id of the worker to unregister.
    pub worker_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterWorkerResponse {
    pub worker_id: Uuid,
}