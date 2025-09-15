use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::utils::error::{OpenApiToolError, Result};
use bumpalo::Bump;
use jsonata_rs::JsonAta;

#[derive(Debug, Clone)]
pub struct ExpressionContext {
    pub access_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub ctx: Value,
}

impl ExpressionContext {
    pub fn empty() -> Self {
        Self {
            access_token: None,
            expires_at: None,
            ctx: Value::Null,
        }
    }
}

/// Evaluate a mapping string with {% %} expressions embedded inside JSON strings.
/// Strategy:
/// - Parse mapping as JSON. Traverse strings; if a string matches ^\{\%(.+)\%\}$, evaluate the inner expr.
/// - Supported expr subset: concatenation with '&' of single-quoted literals and variables
///   ($access_token, $expires_at as ISO8601 string, and simple $ctx.path by dot).
pub fn evaluate_mapping(mapping: &str, context: &ExpressionContext) -> Result<Value> {
    let mut value: Value = serde_json::from_str(mapping).map_err(|e| {
        OpenApiToolError::parse(format!(
            "mapping must be valid JSON with embedded {{% %}} strings: {}",
            e
        ))
    })?;
    substitute_in_value(&mut value, context)?;
    Ok(value)
}

fn substitute_in_value(node: &mut Value, context: &ExpressionContext) -> Result<()> {
    match node {
        Value::Object(map) => {
            for (_k, v) in map.iter_mut() {
                substitute_in_value(v, context)?;
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                substitute_in_value(v, context)?;
            }
        }
        Value::String(s) => {
            if let Some(inner) = strip_expr_markers(s) {
                let evaluated = eval_expr(inner, context)?;
                *node = Value::String(evaluated);
            }
        }
        _ => {}
    }
    Ok(())
}

fn strip_expr_markers(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.starts_with("%}") || trimmed.ends_with("{%") {
        return None;
    }
    if trimmed.starts_with("{%") && trimmed.ends_with("%}") {
        let inner = &trimmed[2..trimmed.len() - 2];
        return Some(inner.trim());
    }
    None
}

fn eval_expr(expr: &str, context: &ExpressionContext) -> Result<String> {
    let arena = Bump::new();
    let engine = JsonAta::new(expr, &arena).map_err(|e| OpenApiToolError::parse(e.to_string()))?;

    let access_token_json = serde_json::Value::String(
        context
            .access_token
            .clone()
            .unwrap_or_else(|| "".to_string()),
    );
    let expires_at_json = match context.expires_at {
        Some(t) => serde_json::Value::String(t.to_rfc3339()),
        None => serde_json::Value::Null,
    };
    let ctx_json: &Value = &context.ctx;

    let mut bindings: std::collections::HashMap<&str, &serde_json::Value> =
        std::collections::HashMap::new();
    bindings.insert("access_token", &access_token_json);
    bindings.insert("expires_at", &expires_at_json);
    bindings.insert("ctx", ctx_json);
    // Provide backwards-compat top-level vars.secrets.* lookup shortcut
    if let Some(secrets) = ctx_json.get("secrets") {
        bindings.insert("vars", secrets);
    }

    let result = engine
        .evaluate(None, Some(&bindings))
        .map_err(|e| OpenApiToolError::parse(e.to_string()))?;

    // Strings vs non-strings
    let out = if result.is_string() {
        result.as_str().to_string()
    } else {
        result.to_string()
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_eval_mapping_headers() {
        let ctx = ExpressionContext {
            access_token: Some("tok123".to_string()),
            expires_at: None,
            ctx: json!({"execution_id": "e1"}),
        };
        let mapping = r#"{
          "headers": {
            "Authorization": "{% 'Bearer ' & $access_token %}",
            "X-Req": "fixed"
          }
        }"#;
        let out = evaluate_mapping(mapping, &ctx).unwrap();
        assert_eq!(out["headers"]["Authorization"], "Bearer tok123");
        assert_eq!(out["headers"]["X-Req"], "fixed");
    }
}
