use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 Request
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse {
    Success {
        jsonrpc: String,
        result: Value,
        id: Option<Value>,
    },
    Error {
        jsonrpc: String,
        error: JsonRpcError,
        id: Option<Value>,
    },
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self::Success {
            jsonrpc: "2.0".to_string(),
            result,
            id,
        }
    }

    pub fn error(id: Option<Value>, error: JsonRpcError) -> Self {
        Self::Error {
            jsonrpc: "2.0".to_string(),
            error,
            id,
        }
    }
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// Standard JSON-RPC error codes
#[allow(dead_code)]
pub const PARSE_ERROR: i32 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i32 = -32600;
#[allow(dead_code)]
pub const METHOD_NOT_FOUND: i32 = -32601;
#[allow(dead_code)]
pub const INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
pub const INTERNAL_ERROR: i32 = -32603;

// Custom application error codes (starting from -32000)
#[allow(dead_code)]
pub const AUTH_ERROR: i32 = -32000;
#[allow(dead_code)]
pub const CONFIG_ERROR: i32 = -32001;
#[allow(dead_code)]
pub const EXECUTION_ERROR: i32 = -32002;
#[allow(dead_code)]
pub const NOT_FOUND_ERROR: i32 = -32003;
#[allow(dead_code)]
pub const VALIDATION_ERROR: i32 = -32004;
