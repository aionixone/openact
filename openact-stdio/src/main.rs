use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};
use tokio::runtime::Runtime;
use tracing::{error, info, warn};

mod rpc;
mod handler;

use handler::RpcHandler;
use rpc::{JsonRpcRequest, JsonRpcResponse, JsonRpcError};

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

fn main() -> Result<()> {
    // Initialize tracing (logs to stderr to avoid interfering with stdout)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("OpenAct STDIO-RPC Server starting...");

    // Create async runtime
    let rt = Runtime::new()?;
    let handler = rt.block_on(async {
        RpcHandler::new().await
    })?;

    info!("STDIO-RPC Server ready, waiting for requests...");

    // Read from stdin line by line
    let stdin = io::stdin();
    let reader = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l.trim().to_string(),
            Err(e) => {
                error!("Failed to read from stdin: {}", e);
                continue;
            }
        };

        if line.is_empty() {
            continue;
        }

        // Parse JSON-RPC request
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => {
                rt.block_on(async {
                    handler.handle_request(request).await
                })
            }
            Err(e) => {
                warn!("Invalid JSON-RPC request: {}", e);
                JsonRpcResponse::error(
                    None,
                    JsonRpcError {
                        code: -32700,
                        message: "Parse error".to_string(),
                        data: Some(serde_json::json!({ "details": e.to_string() })),
                    }
                )
            }
        };

        // Write response to stdout
        match serde_json::to_string(&response) {
            Ok(response_json) => {
                if let Err(e) = writeln!(stdout, "{}", response_json) {
                    error!("Failed to write response to stdout: {}", e);
                    break;
                }
                if let Err(e) = stdout.flush() {
                    error!("Failed to flush stdout: {}", e);
                    break;
                }
            }
            Err(e) => {
                error!("Failed to serialize response: {}", e);
                // Try to send a basic error response
                let error_response = JsonRpcResponse::error(
                    None,
                    JsonRpcError {
                        code: -32603,
                        message: "Internal error".to_string(),
                        data: Some(serde_json::json!({ "details": e.to_string() })),
                    }
                );
                if let Ok(error_json) = serde_json::to_string(&error_response) {
                    let _ = writeln!(stdout, "{}", error_json);
                    let _ = stdout.flush();
                }
            }
        }
    }

    info!("STDIO-RPC Server shutting down...");
    Ok(())
}
