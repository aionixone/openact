use anyhow::{Context, Result};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use std::time::Duration;

use crate::engine::TaskHandler;

#[derive(Default)]
pub struct HttpTaskHandler;

impl HttpTaskHandler {
    fn build_client(timeout_ms: Option<u64>) -> Result<Client> {
        let mut builder = ClientBuilder::new();
        if let Some(ms) = timeout_ms {
            builder = builder.timeout(Duration::from_millis(ms));
        }
        Ok(builder.build().context("failed to build reqwest client")?)
    }

    fn build_headers(map: Option<&Value>) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        if let Some(Value::Object(obj)) = map {
            for (k, v) in obj.iter() {
                let name = HeaderName::from_bytes(k.as_bytes()).context("invalid header name")?;
                let val_owned;
                let val_str = if let Some(s) = v.as_str() {
                    s
                } else {
                    val_owned = v.to_string();
                    &val_owned
                };
                let val = HeaderValue::from_str(val_str).context("invalid header value")?;
                headers.insert(name, val);
            }
        }
        // Default User-Agent, compatible with services like GitHub
        if !headers.contains_key("user-agent") {
            headers.insert(
                HeaderName::from_static("user-agent"),
                HeaderValue::from_static("openact/0.1"),
            );
        }
        Ok(headers)
    }

    fn content_type(headers: &HeaderMap) -> Option<String> {
        headers.get("Content-Type").and_then(|v| v.to_str().ok()).map(|s| s.to_ascii_lowercase())
    }
}

impl HttpTaskHandler {
    fn execute_sync(&self, ctx: &Value) -> Result<Value> {
        // ctx is provided by mapping.input
        let method = ctx.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let url = ctx.get("url").and_then(|v| v.as_str()).context("http.request requires url")?;
        let mut headers = Self::build_headers(ctx.get("headers"))?;
        let timeout_ms = ctx.get("timeoutMs").and_then(|v| v.as_u64());
        let want_trace = ctx.get("trace").and_then(|v| v.as_bool()).unwrap_or(false);
        if want_trace {
            println!(
                "[http] prepare request method={} url={} timeoutMs={:?}",
                method, url, timeout_ms
            );
        }

        let client = Self::build_client(timeout_ms)?;
        let mut req = client.request(method.parse().unwrap_or(reqwest::Method::GET), url);
        // Handle query parameters
        if let Some(q) = ctx.get("query").and_then(|v| v.as_object()) {
            let mut qp: Vec<(String, String)> = Vec::new();
            for (k, v) in q.iter() {
                qp.push((
                    k.clone(),
                    v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()),
                ));
            }
            req = req.query(&qp);
        }
        // Determine body encoding based on Content-Type
        let mut request_body_repr: Option<Value> = None;
        if let Some(body) = ctx.get("body") {
            let ct = Self::content_type(&headers);
            match ct.as_deref() {
                Some(ct) if ct.contains("application/x-www-form-urlencoded") => {
                    match body {
                        Value::String(s) => {
                            req = req.body(s.clone());
                            request_body_repr =
                                Some(Value::String(format!("<form-urlencoded:{} bytes>", s.len())));
                        }
                        Value::Object(obj) => {
                            // Convert to key=value pairs (ignore null values)
                            let mut form_pairs: Vec<(String, String)> = Vec::new();
                            for (k, v) in obj.iter() {
                                if v.is_null() {
                                    continue;
                                }
                                form_pairs.push((
                                    k.clone(),
                                    v.as_str()
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| v.to_string()),
                                ));
                            }
                            req = req.form(&form_pairs);
                            request_body_repr = Some(json!("<form-urlencoded:object>"));
                        }
                        _ => {
                            let s = body.to_string();
                            req = req.body(s.clone());
                            request_body_repr =
                                Some(Value::String(format!("<form-urlencoded:{} bytes>", s.len())));
                        }
                    }
                    if !headers.contains_key("Content-Type") {
                        headers.insert(
                            HeaderName::from_static("content-type"),
                            HeaderValue::from_static("application/x-www-form-urlencoded"),
                        );
                    }
                }
                Some(ct) if ct.contains("application/json") => {
                    req = req.json(body);
                    request_body_repr = Some(json!("<json>"));
                }
                Some(ct) if ct.starts_with("text/") => {
                    let s =
                        body.as_str().map(|s| s.to_string()).unwrap_or_else(|| body.to_string());
                    req = req.body(s.clone());
                    request_body_repr = Some(Value::String(format!("<text:{} bytes>", s.len())));
                }
                _ => match body {
                    Value::Object(_) | Value::Array(_) => {
                        req = req.json(body);
                        request_body_repr = Some(json!("<json>"));
                    }
                    Value::String(s) => {
                        req = req.body(s.clone());
                        request_body_repr = Some(Value::String(format!("<raw:{} bytes>", s.len())));
                    }
                    _ => {
                        let s = body.to_string();
                        req = req.body(s.clone());
                        request_body_repr = Some(Value::String(format!("<raw:{} bytes>", s.len())));
                    }
                },
            }
        }
        if !headers.is_empty() {
            req = req.headers(headers.clone());
        }

        if want_trace {
            println!("[http] sending request to {}", url);
        }
        let resp = req.send().map_err(|e| {
            if want_trace {
                println!("[http] send error: {}", e);
            }
            anyhow::anyhow!("http request failed: {}", e)
        })?;
        let status = resp.status().as_u16();
        let mut hdrs_out = serde_json::Map::new();
        for (k, v) in resp.headers().iter() {
            hdrs_out.insert(k.to_string(), json!(v.to_str().unwrap_or("")));
        }
        let text = resp.text().unwrap_or_default();
        let body_json = serde_json::from_str::<Value>(&text).unwrap_or(json!(text));
        if !(200..=299).contains(&status) {
            let kind = if (400..=499).contains(&status) {
                "Http.4xx"
            } else if (500..=599).contains(&status) {
                "Http.5xx"
            } else {
                "Http.Error"
            };
            let mut sent_headers = serde_json::Map::new();
            for (k, v) in headers.iter() {
                sent_headers.insert(k.to_string(), json!(v.to_str().unwrap_or("")));
            }
            let trace = json!({
                "request": { "method": method, "url": url, "headers": sent_headers, "body": request_body_repr.unwrap_or(Value::Null) }
            });
            let detail = json!({ "status": status, "kind": kind, "headers": hdrs_out, "body": body_json, "trace": trace });
            if want_trace {
                println!("[http] error status={} detail={}", status, detail);
            }
            println!(
                "[http] HTTP error {}: {}",
                status,
                serde_json::to_string(&detail).unwrap_or_else(|_| "invalid json".to_string())
            );
            anyhow::bail!("{} {}", kind, detail);
        }

        let mut out = json!({ "status": status, "headers": hdrs_out, "body": body_json });
        if want_trace {
            let mut sent_headers = serde_json::Map::new();
            for (k, v) in headers.iter() {
                sent_headers.insert(k.to_string(), json!(v.to_str().unwrap_or("")));
            }
            let trace = json!({ "request": { "method": method, "url": url, "headers": sent_headers, "body": request_body_repr.unwrap_or(Value::Null) } });
            println!("[http] success status={} url={}", status, url);
            if let Value::Object(ref mut map) = out {
                map.insert("trace".into(), trace);
            }
        }
        Ok(out)
    }
}

impl TaskHandler for HttpTaskHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // Always use block_in_place to avoid runtime conflicts
        let ctx_clone = ctx.clone();
        let result = tokio::task::block_in_place(|| self.execute_sync(&ctx_clone));
        result
    }
}
