use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trn(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorKind(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub trn: Trn,
    pub connector: ConnectorKind,
    pub name: String,
    pub config_json: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub trn: Trn,
    pub connector: ConnectorKind,
    pub name: String,
    pub connection_trn: Trn,
    pub config_json: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub paused_state: String,
    pub context_json: JsonValue,
    pub await_meta_json: Option<JsonValue>,
}


