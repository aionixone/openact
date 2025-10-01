//! Helpers to map protocol-agnostic DTOs to the official MCP Rust SDK types.

use crate::dto::{InvokeResult, ToolSpec};
use rmcp::model as m;
use std::borrow::Cow;
use std::sync::Arc;

fn to_json_object(val: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match val {
        serde_json::Value::Object(map) => map.clone(),
        _ => {
            // Wrap as minimal object schema if only properties were provided as non-object
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String("object".into()));
            obj
        }
    }
}

pub fn to_mcp_tool(spec: &ToolSpec) -> m::Tool {
    // Build input schema as a full JSON object schema with properties
    let input_obj = {
        let mut obj = serde_json::Map::new();
        obj.insert("type".into(), serde_json::Value::String("object".into()));
        // If input_schema already contains an object with properties/type, merge it; else treat as properties
        match &spec.input_schema {
            serde_json::Value::Object(mo) => {
                for (k, v) in mo.iter() {
                    obj.insert(k.clone(), v.clone());
                }
                if !obj.contains_key("type") {
                    obj.insert("type".into(), serde_json::Value::String("object".into()));
                }
            }
            other => {
                obj.insert("properties".into(), other.clone());
            }
        }
        obj
    };

    let output_obj = spec.output_schema.as_ref().and_then(|v| match v {
        serde_json::Value::Object(map) => Some(map.clone()),
        _ => None,
    });

    m::Tool {
        name: Cow::Owned(spec.name.clone()),
        title: spec.title.clone(),
        description: spec.description.as_ref().map(|s| Cow::Owned(s.clone())),
        input_schema: Arc::new(input_obj),
        output_schema: output_obj.map(|m| Arc::new(m)),
        annotations: spec
            .annotations
            .as_ref()
            .and_then(|v| serde_json::from_value::<m::ToolAnnotations>(v.clone()).ok()),
        icons: None,
    }
}

pub fn to_mcp_call_result(res: &InvokeResult) -> m::CallToolResult {
    // Prefer structured result; rmcp::CallToolResult::structured will also include a text content
    m::CallToolResult::structured(res.structured.clone())
}
