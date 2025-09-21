#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    pub dsl: serde_json::Value,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartExecutionRequest {
    pub workflow_id: String,
    pub flow: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeExecutionRequest {
    pub input: serde_json::Value,
}


