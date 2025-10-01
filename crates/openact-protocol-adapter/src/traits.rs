use async_trait::async_trait;

use crate::dto::{InvokeRequest, InvokeResult, ProtocolError, ToolSpec};

/// List available tools for a given tenant (or global)
#[async_trait]
pub trait ToolCatalog: Send + Sync {
    async fn list_tools(&self, tenant: Option<&str>) -> Result<Vec<ToolSpec>, ProtocolError>;
}

/// Invoke a tool with structured arguments
#[async_trait]
pub trait ToolInvoker: Send + Sync {
    async fn invoke(&self, req: InvokeRequest) -> Result<InvokeResult, ProtocolError>;
}

