#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use chrono::{DateTime, Utc};
#[cfg(all(feature = "server", feature = "openapi"))]
#[allow(unused_imports)] // Used in schema examples via json! macro
use serde_json::json;
#[cfg(all(feature = "server", feature = "openapi"))]
use utoipa::ToSchema;

#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(feature = "server", feature = "openapi"), schema(example = json!({
    "name": "GitHub OAuth Flow",
    "description": "Complete GitHub OAuth2 authentication workflow",
    "dsl": {
        "startAt": "Config",
        "states": {
            "Config": {
                "type": "pass",
                "assign": {
                    "config": {
                        "authorizeUrl": "https://github.com/login/oauth/authorize",
                        "tokenUrl": "https://github.com/login/oauth/access_token",
                        "redirectUri": "http://localhost:8080/oauth/callback",
                        "defaultScope": "user:email"
                    }
                },
                "next": "StartAuth"
            },
            "StartAuth": {
                "type": "task", 
                "resource": "oauth2.authorize_redirect",
                "parameters": {
                    "authorizeUrl": "{% $config.authorizeUrl %}",
                    "clientId": "{% $creds.client_id %}",
                    "redirectUri": "{% $config.redirectUri %}",
                    "scope": "{% $config.defaultScope %}"
                },
                "end": true
            }
        }
    }
})))]
pub struct CreateWorkflowRequest {
    #[cfg_attr(
        all(feature = "server", feature = "openapi"),
        schema(example = "GitHub OAuth Flow")
    )]
    pub name: String,
    #[cfg_attr(
        all(feature = "server", feature = "openapi"),
        schema(example = "Complete GitHub OAuth2 authentication workflow")
    )]
    pub description: Option<String>,
    pub dsl: serde_json::Value,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
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
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ResumeExecutionRequest {
    pub input: serde_json::Value,
}

// Response DTOs for AuthFlow endpoints
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: crate::server::authflow::state::WorkflowStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct WorkflowListResponse {
    pub workflows: Vec<WorkflowSummary>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct WorkflowDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub dsl: serde_json::Value, // Simplified for API response
    pub status: crate::server::authflow::state::WorkflowStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct WorkflowGraphResponse {
    #[serde(rename = "workflowId")]
    pub workflow_id: String,
    pub graphs: std::collections::HashMap<String, serde_json::Value>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ExecutionSummary {
    pub execution_id: String,
    pub workflow_id: String,
    pub flow: String,
    pub status: crate::server::authflow::state::ExecutionStatus,
    pub current_state: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ExecutionListResponse {
    pub executions: Vec<ExecutionSummary>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ExecutionDetail {
    pub execution_id: String,
    pub workflow_id: String,
    pub flow: String,
    pub status: crate::server::authflow::state::ExecutionStatus,
    pub current_state: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub input: serde_json::Value,
    pub context: Option<serde_json::Value>,
    pub error: Option<String>,
    pub pending_info: Option<serde_json::Value>, // For OAuth redirects, etc.
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct StateHistoryEntry {
    pub state: String,
    pub status: String,
    pub entered_at: DateTime<Utc>,
    pub exited_at: Option<DateTime<Utc>>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ExecutionTraceResponse {
    pub execution_id: String,
    pub workflow_id: String,
    pub state_history: Vec<StateHistoryEntry>,
    pub current_state: Option<String>,
    pub status: crate::server::authflow::state::ExecutionStatus,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ExecutionCreatedResponse {
    pub execution_id: String,
    pub workflow_id: String,
    pub status: crate::server::authflow::state::ExecutionStatus,
    pub message: String,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub struct ExecutionActionResponse {
    pub execution_id: String,
    pub status: crate::server::authflow::state::ExecutionStatus,
    pub message: String,
}
