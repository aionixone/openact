//! JSON-RPC 2.0 types and utilities

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const JSONRPC_VERSION: &str = "2.0";

// Error codes (from JSON-RPC 2.0 spec)
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// JSON-RPC 2.0 Request ID (can be string, number, or null)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
    Null,
}

impl RequestId {
    pub fn new_uuid() -> Self {
        RequestId::String(Uuid::new_v4().to_string())
    }
}

/// JSON-RPC 2.0 Request
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Option<RequestId>,
}

/// JSON-RPC 2.0 Error object
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: String) -> Self {
        Self { code, message, data: None }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn parse_error() -> Self {
        Self::new(PARSE_ERROR, "Parse error".to_string())
    }

    pub fn invalid_request() -> Self {
        Self::new(INVALID_REQUEST, "Invalid Request".to_string())
    }

    pub fn method_not_found() -> Self {
        Self::new(METHOD_NOT_FOUND, "Method not found".to_string())
    }

    pub fn invalid_params() -> Self {
        Self::new(INVALID_PARAMS, "Invalid params".to_string())
    }

    pub fn internal_error() -> Self {
        Self::new(INTERNAL_ERROR, "Internal error".to_string())
    }
}

/// Create a successful JSON-RPC response
pub fn success_response(id: Option<RequestId>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: JSONRPC_VERSION.to_string(), result: Some(result), error: None, id }
}

/// Create an error JSON-RPC response
pub fn error_response(id: Option<RequestId>, error: JsonRpcError) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: JSONRPC_VERSION.to_string(), result: None, error: Some(error), id }
}
