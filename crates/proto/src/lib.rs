use serde::{Deserialize, Serialize, Deserializer, Serializer};
use uuid::Uuid;

use base64::{engine::general_purpose, Engine as _};

/// What client sends as metadata with a module
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobManifest {
    pub call_args: Vec<String>,
}

/// What client sends as payload (includes WASM binary module and JobManifest metadata)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobSubmission {
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