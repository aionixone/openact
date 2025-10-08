use crate::dto::{InvokeRequest, InvokeResult, ProtocolError, ToolSpec};

/// List available tools for a given tenant (or global)
pub trait ToolCatalog: Send + Sync {
    fn list_tools<'a>(
        &'a self,
        tenant: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<ToolSpec>, ProtocolError>> + Send + 'a>,
    >;
}

/// Invoke a tool with structured arguments
pub trait ToolInvoker: Send + Sync {
    fn invoke<'a>(
        &'a self,
        req: InvokeRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<InvokeResult, ProtocolError>> + Send + 'a>,
    >;
}
