//! Helpers to map protocol-agnostic DTOs to the official MCP Rust SDK types.
//! Enabled with the `mcp` feature.

use crate::dto::{InvokeResult, ToolSpec};

#[cfg(feature = "mcp")]
use rmcp::model as m;

#[cfg(feature = "mcp")]
pub fn to_mcp_tool(spec: &ToolSpec) -> m::Tool {
    m::Tool {
        name: spec.name.clone(),
        description: spec.description.clone(),
        title: spec.title.clone(),
        annotations: spec
            .annotations
            .as_ref()
            .and_then(|v| serde_json::from_value::<m::ToolAnnotations>(v.clone()).ok()),
        input_schema: m::ToolInputSchema {
            r#type: "object".into(),
            properties: Some(spec.input_schema.clone()),
            required: None,
        },
        output_schema: spec
            .output_schema
            .as_ref()
            .and_then(|v| serde_json::from_value::<m::ToolOutputSchema>(v.clone()).ok()),
    }
}

#[cfg(feature = "mcp")]
pub fn to_mcp_call_result(res: &InvokeResult) -> m::CallToolResult {
    let text = res
        .text_fallback
        .clone()
        .unwrap_or_else(|| serde_json::to_string(&res.structured).unwrap_or_else(|_| "{}".into()));
    let block = m::Content::Text(m::TextContent { r#type: "text".into(), text, annotations: None });
    m::CallToolResult::success(vec![block]).with_structured_content(res.structured.clone())
}

