// Action runner implementation
// Handles execution of actions with TRN integration

use super::auth::{AuthAdapter, AuthContext, RefreshWhen};
use super::expression_context::build_expression_context;
use super::expression_engine::evaluate_mapping;
use super::models::*;
use crate::utils::error::{OpenApiToolError, Result};
use bumpalo::Bump;
use jsonata_rs::JsonAta;
use rand::SeedableRng;
use rand::{rngs::StdRng, Rng};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Client, Method, Url};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Action runner for executing actions
pub struct ActionRunner {
    /// Execution timeout in milliseconds
    timeout_ms: u64,
    /// Maximum retry attempts
    max_retries: u32,
    /// Authentication adapter for handling auth
    auth_adapter: Option<Arc<AuthAdapter>>,
    /// Tenant identifier
    #[allow(dead_code)]
    tenant: String,
}

impl ActionRunner {
    /// Create a new action runner
    pub fn new() -> Self {
        Self {
            timeout_ms: 30000, // 30 seconds default
            max_retries: 3,
            auth_adapter: None,
            tenant: "default".to_string(),
        }
    }

    /// Create a new action runner with tenant
    pub fn with_tenant(tenant: String) -> Self {
        Self {
            timeout_ms: 30000,
            max_retries: 3,
            auth_adapter: None,
            tenant,
        }
    }

    /// Set the authentication adapter
    pub fn set_auth_adapter(&mut self, auth_adapter: Arc<AuthAdapter>) {
        self.auth_adapter = Some(auth_adapter);
    }

    /// Create a new action runner with custom timeout
    pub fn with_timeout(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            max_retries: 3,
            auth_adapter: None,
            tenant: "default".to_string(),
        }
    }

    /// Execute an action
    pub async fn execute_action(
        &self,
        action: &Action,
        context: ActionExecutionContext,
    ) -> Result<ActionExecutionResult> {
        let start_time = std::time::Instant::now();

        // Create execution result
        let result =
            ActionExecutionResult::new(context.execution_trn.clone(), ExecutionStatus::Running);

        // Validate action
        if let Err(e) = action.validate() {
            return Ok(result
                .set_error_message(format!("Action validation failed: {}", e))
                .set_duration(start_time.elapsed().as_millis() as u64));
        }

        // Validate context
        if let Err(e) = self.validate_context(&context) {
            return Ok(result
                .set_error_message(format!("Context validation failed: {}", e))
                .set_duration(start_time.elapsed().as_millis() as u64));
        }

        // Execute the action (placeholder implementation)
        match self.execute_action_impl(action, context).await {
            Ok(response_data) => Ok(result
                .set_response_data(response_data)
                .set_status_code(200)
                .set_duration(start_time.elapsed().as_millis() as u64)
                .mark_success()),
            Err(e) => Ok(result
                .set_error_message(e.to_string())
                .set_duration(start_time.elapsed().as_millis() as u64)),
        }
    }

    /// Validate execution context
    fn validate_context(&self, context: &ActionExecutionContext) -> Result<()> {
        if context.action_trn.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Action TRN cannot be empty".to_string(),
            ));
        }

        if context.execution_trn.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Execution TRN cannot be empty".to_string(),
            ));
        }

        if context.tenant.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Tenant cannot be empty".to_string(),
            ));
        }

        if context.provider.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Provider cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Execute action implementation with authentication
    async fn execute_action_impl(
        &self,
        action: &Action,
        context: ActionExecutionContext,
    ) -> Result<Value> {
        let mut diagnostics: Vec<Value> = Vec::new();
        let start_time = std::time::Instant::now();
        let mut trace_events: Vec<serde_json::Value> = Vec::new();
        // 1. Get authentication context if needed
        let auth_context = if let Some(auth_config) = &action.auth_config {
            if let Some(adapter) = &self.auth_adapter {
                Some(adapter.get_auth_for_action(auth_config).await?)
            } else {
                return Err(OpenApiToolError::ValidationError(
                    "Authentication required but no auth adapter configured".to_string(),
                ));
            }
        } else {
            None
        };

        // 2. Build HTTP request headers and query via injection mapping
        let mut headers = context.headers.clone();
        let mut query: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut transformed_body: Option<Value> = None;
        if let (Some(auth), Some(auth_cfg)) = (&auth_context, &action.auth_config) {
            // Base Authorization header
            headers.insert("Authorization".to_string(), auth.get_auth_header());

            // Evaluate injection mapping if provided
            let mapping = &auth_cfg.injection.mapping;
            let expr_ctx = build_expression_context(auth, action, &context);
            if !mapping.trim().is_empty() {
                // Diagnostics: enumerate required $vars.secrets.* keys and check availability
                if let Some(missing) = missing_secret_keys_for_mapping(mapping, &expr_ctx) {
                    if !missing.is_empty() {
                        let hints: Vec<String> = missing
                            .iter()
                            .map(|k| {
                                format!(
                                    "{} -> set env: {}",
                                    k,
                                    env_var_candidates_for_key(k).join(" | ")
                                )
                            })
                            .collect();
                        diagnostics.push(serde_json::json!({
                            "type": "missing_secrets",
                            "keys": missing,
                            "hints": hints,
                            "tip": "Or provide OPENACT_SECRETS_FILE (json/yaml) with {key: value}"
                        }));
                    }
                }
                let evaluated = evaluate_mapping(mapping, &expr_ctx)?;
                if let Some(hdrs) = evaluated.get("headers").and_then(|v| v.as_object()) {
                    for (k, v) in hdrs.iter() {
                        headers.insert(
                            k.to_string(),
                            v.as_str().unwrap_or(&v.to_string()).to_string(),
                        );
                    }
                }
                if let Some(qs) = evaluated.get("query").and_then(|v| v.as_object()) {
                    for (k, v) in qs.iter() {
                        query.insert(
                            k.to_string(),
                            v.as_str().unwrap_or(&v.to_string()).to_string(),
                        );
                    }
                }
                if let Some(bv) = evaluated.get("body").cloned() {
                    transformed_body = Some(bv);
                }
            }
            // Merge any additional headers from auth context last (lowest precedence)
            for (key, value) in &auth.headers {
                headers.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }

        // 2.1 Apply x-transform-pre (array of mapping objects or strings)
        if let Some(pre_arr) = action
            .extensions
            .get("x-transform-pre")
            .and_then(|v| v.as_array())
        {
            let expr_ctx = build_expression_context(
                auth_context.as_ref().unwrap_or(&AuthContext::new(
                    "".into(),
                    "".into(),
                    action.provider.clone(),
                )),
                action,
                &context,
            );
            for item in pre_arr {
                let evaluated_map = if let Some(s) = item.as_str() {
                    evaluate_mapping(s, &expr_ctx).ok()
                } else {
                    // Evaluate {% %} inside object by round-tripping to string
                    serde_json::to_string(item)
                        .ok()
                        .and_then(|s| evaluate_mapping(&s, &expr_ctx).ok())
                };
                if let Some(map) = evaluated_map {
                    if let Some(hdrs) = map.get("headers").and_then(|v| v.as_object()) {
                        for (k, v) in hdrs.iter() {
                            headers.insert(
                                k.to_string(),
                                v.as_str().unwrap_or(&v.to_string()).to_string(),
                            );
                        }
                    }
                    if let Some(qs) = map.get("query").and_then(|v| v.as_object()) {
                        for (k, v) in qs.iter() {
                            query.insert(
                                k.to_string(),
                                v.as_str().unwrap_or(&v.to_string()).to_string(),
                            );
                        }
                    }
                    if let Some(bv) = map.get("body").cloned() {
                        transformed_body = Some(bv);
                    }
                }
            }
        }

        // 3. Build the HTTP request (placeholder implementation)
        // In a real implementation, this would use an HTTP client like reqwest
        // Resolve effective timeout: x-timeout-ms on action overrides runner default
        let effective_timeout_ms = action.timeout_ms.unwrap_or(self.timeout_ms);
        // 3.1 Resolve retry settings from extensions (x-retry)
        let retry_settings = resolve_retry_settings(action);

        // 3.2 Compute retry plan (delays) preview
        let mut rng = StdRng::seed_from_u64(context.timestamp.timestamp_millis() as u64);
        let mut attempts_plan: Vec<u64> = Vec::new();
        for attempt in 1..=retry_settings.max_retries {
            attempts_plan.push(compute_backoff_ms(attempt, &retry_settings, Some(&mut rng)));
        }

        // 3.3 Simulated HTTP execution with retry (test-only via extension `x-simulate-statuses`)
        let mut attempted_statuses: Vec<u16> = Vec::new();
        if let Some(sim) = action
            .extensions
            .get("x-simulate-statuses")
            .and_then(|v| v.as_array())
        {
            let mut i = 0usize;
            for attempt in 0..=retry_settings.max_retries {
                // initial + retries
                let status = sim
                    .get(i % sim.len())
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200) as u16;
                attempted_statuses.push(status);
                trace_events.push(serde_json::json!({
                    "type": "http_attempt",
                    "attempt": attempt,
                    "status": status
                }));
                if (200..=299).contains(&status) {
                    break;
                }
                if attempt == retry_settings.max_retries {
                    break;
                }
                if !should_retry_for(status, &retry_settings.retry_on) {
                    break;
                }
                // Here we would sleep backoff when implementing real HTTP
                i += 1;
            }
        }

        let final_status = attempted_statuses.last().copied().unwrap_or(200);
        let last_error_class = if final_status >= 500 { Some("5xx") } else if final_status == 429 { Some("429") } else { None };
        let attempts_total = attempted_statuses.len() as u32;
        let retries_done = attempts_total.saturating_sub(1);

        // 3.4 Real HTTP execution with retries (guarded by extension x-real-http)
        let mut http_result: Option<Value> = None;
        let real_http = action
            .extensions
            .get("x-real-http")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if real_http {
            if let Some(url) = resolve_url(action, &query) {
                let client = Client::builder()
                    .timeout(Duration::from_millis(effective_timeout_ms))
                    .build()
                    .map_err(|e| {
                        OpenApiToolError::network(format!("failed to build client: {}", e))
                    })?;

                let mut attempt: u32 = 0;
                loop {
                    // Proactive refresh before first attempt if configured
                    if let (Some(adapter), Some(auth_cfg), Some(auth)) =
                        (&self.auth_adapter, &action.auth_config, &auth_context)
                    {
                        if let Some(ref refresh_cfg) = auth_cfg.refresh {
                            if matches!(
                                refresh_cfg.when,
                                RefreshWhen::Proactive | RefreshWhen::ProactiveOr401
                            ) {
                                let _ = adapter.refresh_auth_context(auth).await;
                                // ignore error, best-effort
                            }
                        }
                    }
                    // Build request
                    let mut req_builder =
                        client.request(resolve_method(&action.method), url.clone());
                    let mut header_map = HeaderMap::new();
                    for (k, v) in &headers {
                        if let Ok(name) = HeaderName::from_bytes(k.as_bytes()) {
                            if let Ok(val) = HeaderValue::from_str(v) {
                                header_map.insert(name, val);
                            }
                        }
                    }
                    // Attach JSON or multipart body if available and method supports body
                    let method = resolve_method(&action.method);
                    if matches!(method, Method::POST | Method::PUT | Method::PATCH) {
                        // Prefer a unified body source then decide multipart vs JSON
                        let body_to_send = transformed_body
                            .clone()
                            .or_else(|| context.request_body.clone());
                        // Multipart protocol: body = { "_multipart": { "fields": {k:v}, "files": [...] } }
                        let try_multipart = body_to_send
                            .as_ref()
                            .and_then(|b| b.get("_multipart"));
                        if let Some(mp) = try_multipart.and_then(|v| v.as_object()) {
                            let mut form = reqwest::multipart::Form::new();
                            if let Some(fields) = mp.get("fields").and_then(|v| v.as_object()) {
                                for (k, v) in fields {
                                    let s = v.as_str().unwrap_or(&v.to_string()).to_string();
                                    form = form.text(k.clone(), s);
                                }
                            }
                            if let Some(files) = mp.get("files").and_then(|v| v.as_array()) {
                                for f in files {
                                    if let (Some(field), Some(path)) = (
                                        f.get("field").and_then(|v| v.as_str()),
                                        f.get("path").and_then(|v| v.as_str()),
                                    ) {
                                        let filename = f
                                            .get("filename")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| {
                                                std::path::Path::new(path)
                                                    .file_name()
                                                    .and_then(|os| os.to_str())
                                                    .unwrap_or("upload.bin")
                                                    .to_string()
                                            });
                                        let bytes = std::fs::read(path).unwrap_or_default();
                                        let filename_clone = filename.clone();
                                        let mut part =
                                            reqwest::multipart::Part::bytes(bytes.clone())
                                                .file_name(filename_clone);
                                        if let Some(ct) =
                                            f.get("content_type").and_then(|v| v.as_str())
                                        {
                                            if let Ok(p) = reqwest::multipart::Part::bytes(bytes)
                                                .file_name(filename)
                                                .mime_str(ct)
                                            {
                                                part = p;
                                            }
                                        }
                                        form = form.part(field.to_string(), part);
                                    }
                                }
                            }
                            req_builder = req_builder.multipart(form);
                        } else {
                            if let Some(bv) = body_to_send {
                                if !header_map.contains_key(CONTENT_TYPE) {
                                    header_map.insert(
                                        CONTENT_TYPE,
                                        HeaderValue::from_static("application/json"),
                                    );
                                }
                                if let Ok(body_str) = serde_json::to_string(&bv) {
                                    req_builder = req_builder.body(body_str);
                                }
                            }
                        }
                    }

                    // Apply headers after potential content-type insertion for JSON branch
                    req_builder = req_builder.headers(header_map.clone());

                    let before = std::time::Instant::now();
                    let resp = req_builder.send().await;
                    match resp {
                        Ok(r) => {
                            let status = r.status().as_u16();
                            trace_events.push(serde_json::json!({
                                "type": "http_attempt",
                                "attempt": attempt,
                                "status": status,
                                "elapsed_ms": before.elapsed().as_millis() as u64
                            }));
                            if (200..=299).contains(&status) {
                                let content_type = r
                                    .headers()
                                    .get("content-type")
                                    .and_then(|h| h.to_str().ok())
                                    .unwrap_or("")
                                    .to_lowercase();
                                let body_val: Value = if content_type.contains("application/json") {
                                    match r.json::<Value>().await {
                                        Ok(v) => v,
                                        Err(_) => Value::Null,
                                    }
                                } else if content_type.contains("ndjson")
                                    || content_type.contains("application/x-ndjson")
                                {
                                    match r.text().await {
                                        Ok(t) => {
                                            let items: Vec<Value> = t
                                                .lines()
                                                .filter(|ln| !ln.trim().is_empty())
                                                .map(|ln| {
                                                    serde_json::from_str::<Value>(ln)
                                                        .unwrap_or(Value::String(ln.to_string()))
                                                })
                                                .collect();
                                            Value::Array(items)
                                        }
                                        Err(_) => Value::Null,
                                    }
                                } else {
                                    match r.text().await {
                                        Ok(t) => Value::String(t),
                                        Err(_) => Value::Null,
                                    }
                                };
                                http_result = Some(
                                    serde_json::json!({"url": url.as_str(), "status": status, "body": body_val }),
                                );
                                break;
                            } else {
                                // Non-success status
                                let mut handled = false;
                                // Refresh on 401 if configured
                                if status == 401 {
                                    if let (Some(adapter), Some(auth_cfg), Some(auth)) =
                                        (&self.auth_adapter, &action.auth_config, &auth_context)
                                    {
                                        if let Some(ref refresh_cfg) = auth_cfg.refresh {
                                            if matches!(
                                                refresh_cfg.when,
                                                RefreshWhen::On401 | RefreshWhen::ProactiveOr401
                                            ) {
                                                let _ = adapter.refresh_auth_context(auth).await;
                                                handled = true;
                                            }
                                        }
                                    }
                                }
                                if handled {
                                    // immediate retry without consuming a retry slot
                                    continue;
                                }
                                if attempt >= retry_settings.max_retries
                                    || !should_retry_for(status, &retry_settings.retry_on)
                                {
                                    let text = r.text().await.unwrap_or_default();
                                    http_result = Some(
                                        serde_json::json!({"url": url.as_str(), "status": status, "body": text }),
                                    );
                                    break;
                                }
                                // Respect Retry-After if present
                                if retry_settings.respect_retry_after {
                                    if let Some(wait) = parse_retry_after(r.headers()) {
                                        trace_events.push(serde_json::json!({
                                            "type": "retry_sleep",
                                            "attempt": attempt,
                                            "wait_ms": wait.as_millis() as u64,
                                            "reason": "retry-after"
                                        }));
                                        sleep(wait).await;
                                        attempt += 1;
                                        continue;
                                    }
                                }
                                // Backoff
                                let delay_ms =
                                    compute_backoff_ms(attempt + 1, &retry_settings, None);
                                trace_events.push(serde_json::json!({
                                    "type": "retry_sleep",
                                    "attempt": attempt,
                                    "wait_ms": delay_ms,
                                    "reason": "backoff"
                                }));
                                sleep(Duration::from_millis(delay_ms)).await;
                                attempt += 1;
                                continue;
                            }
                        }
                        Err(e) => {
                            let err_s = e.to_string();
                            trace_events.push(serde_json::json!({
                                "type": "http_attempt_error",
                                "attempt": attempt,
                                "error": err_s
                            }));
                            if attempt >= retry_settings.max_retries {
                                http_result =
                                    Some(serde_json::json!({"url": url.as_str(), "error": err_s }));
                                break;
                            }
                            let delay_ms = compute_backoff_ms(attempt + 1, &retry_settings, None);
                            trace_events.push(serde_json::json!({
                                "type": "retry_sleep",
                                "attempt": attempt,
                                "wait_ms": delay_ms,
                                "reason": "backoff"
                            }));
                            sleep(Duration::from_millis(delay_ms)).await;
                            attempt += 1;
                            continue;
                        }
                    }
                }
            } else {
                http_result =
                    Some(serde_json::json!({"error": "missing x-url or x-base-url+path"}));
            }
        }

        // 3.5 Pagination (cursor/page/link) when real_http and x-pagination provided
        if real_http {
            // Prefer typed pagination, fallback to x-pagination extension
            let has_pagination =
                action.pagination.is_some() || action.extensions.get("x-pagination").is_some();
            if has_pagination {
                if let Some(mut url) = resolve_url(action, &query) {
                    let client = Client::builder()
                        .timeout(Duration::from_millis(effective_timeout_ms))
                        .build()
                        .map_err(|e| {
                            OpenApiToolError::network(format!("failed to build client: {}", e))
                        })?;

                    let (
                        mode,
                        param,
                        limit,
                        next_expr_raw,
                        stop_expr_raw,
                        items_expr_raw,
                        link_expr_raw,
                    ) = if let Some(p) = &action.pagination {
                        (
                            p.mode.as_str(),
                            p.param.as_str(),
                            p.limit,
                            p.next_expr.as_deref(),
                            p.stop_expr.as_deref(),
                            p.items_expr.as_deref(),
                            p.link_expr.as_deref(),
                        )
                    } else if let Some(obj) = action
                        .extensions
                        .get("x-pagination")
                        .and_then(|v| v.as_object())
                    {
                        (
                            obj.get("mode").and_then(|v| v.as_str()).unwrap_or("cursor"),
                            obj.get("param").and_then(|v| v.as_str()).unwrap_or("page"),
                            obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(5),
                            obj.get("next_expr").and_then(|v| v.as_str()),
                            obj.get("stop_expr").and_then(|v| v.as_str()),
                            obj.get("items_expr").and_then(|v| v.as_str()),
                            obj.get("link_expr").and_then(|v| v.as_str()),
                        )
                    } else {
                        ("cursor", "page", 5, None, None, None, None)
                    };

                    let mut pages: Vec<Value> = Vec::new();
                    let mut items: Vec<Value> = Vec::new();
                    let mut token: Option<String> = None;

                    for _ in 0..limit {
                        // Add token as query param for cursor/pageToken modes
                        if let Some(tk) = &token {
                            if mode == "cursor" || mode == "pageToken" {
                                let mut qp = url.query_pairs_mut();
                                qp.append_pair(param, tk);
                            }
                        }

                        let mut req = client.get(url.clone());
                        let mut header_map = HeaderMap::new();
                        for (k, v) in &headers {
                            if let (Ok(n), Ok(val)) = (
                                HeaderName::from_bytes(k.as_bytes()),
                                HeaderValue::from_str(v),
                            ) {
                                header_map.insert(n, val);
                            }
                        }
                        req = req.headers(header_map);

                        let resp = match req.send().await {
                            Ok(r) => r,
                            Err(_) => break,
                        };
                        let status = resp.status().as_u16();
                        let body_text = match resp.text().await {
                            Ok(t) => t,
                            Err(_) => String::new(),
                        };
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::Null);
                        pages.push(body_json.clone());

                        // items_expr projection
                        if let Some(expr_raw) = items_expr_raw {
                            if let Some(val) = eval_jsonata(expr_raw, status, &body_json) {
                                if let Value::Array(arr) = val {
                                    items.extend(arr);
                                }
                            }
                        }
                        // stop condition
                        if let Some(expr_raw) = stop_expr_raw {
                            if let Some(val) = eval_jsonata(expr_raw, status, &body_json) {
                                if val.as_bool().unwrap_or(false) {
                                    break;
                                }
                            }
                        }

                        // Advance to next page
                        if mode == "link" {
                            if let Some(expr_raw) = link_expr_raw {
                                if let Some(val) = eval_jsonata(expr_raw, status, &body_json) {
                                    if let Some(u) = val.as_str() {
                                        if let Ok(new_url) = Url::parse(u) {
                                            url = new_url;
                                            token = None;
                                            continue;
                                        }
                                    }
                                }
                            }
                            break; // no link available
                        } else if let Some(expr_raw) = next_expr_raw {
                            if let Some(val) = eval_jsonata(expr_raw, status, &body_json) {
                                token = val.as_str().map(|s| s.to_string());
                            }
                            if token.is_none() {
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    // attach pagination results
                    let agg = serde_json::json!({"pages": pages, "items": items});
                    http_result = Some(match http_result {
                        Some(mut h) => {
                            h.as_object_mut().map(|o| {
                                o.insert("pagination".to_string(), agg.clone());
                            });
                            h
                        }
                        None => agg,
                    });
                }
            }
        }

        // 3.6 Evaluate x-transform-post to compute flexible output
        let mut post_output: Option<Value> = None;
        if let Some(post_arr) = action
            .extensions
            .get("x-transform-post")
            .and_then(|v| v.as_array())
        {
            let body_json = if let Some(http) = &http_result {
                http.get("body").cloned().unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            for item in post_arr {
                if let Some(s) = item.as_str() {
                    if let Some(v) =
                        crate::action::runner::eval_jsonata(s, final_status, &body_json)
                    {
                        post_output = Some(v);
                    }
                } else if let Some(expr) = item.get("output_expr").and_then(|v| v.as_str()) {
                    if let Some(v) =
                        crate::action::runner::eval_jsonata(expr, final_status, &body_json)
                    {
                        post_output = Some(v);
                    }
                }
            }
        }

        // 3.7 Evaluate x-ok-path if provided
        let mut ok_flag: Option<bool> = None;
        if let Some(ok_expr_raw) = action
            .ok_path
            .as_deref()
            .or_else(|| action.extensions.get("x-ok-path").and_then(|v| v.as_str()))
        {
            let ok_expr = strip_markers(ok_expr_raw).to_string();
            let arena = Bump::new();
            if let Ok(engine) = JsonAta::new(&ok_expr, &arena) {
                // Build bindings: status and body (from http_result if any)
                let status_json = serde_json::Value::Number(serde_json::Number::from(final_status));
                let body_json = if let Some(http) = &http_result {
                    http.get("body").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                let mut bindings = std::collections::HashMap::new();
                bindings.insert("status", &status_json);
                bindings.insert("body", &body_json);
                if let Ok(val) = engine.evaluate(None, Some(&bindings)) {
                    ok_flag = Some(val.as_bool());
                }
            }
        }

        // 3.8 Evaluate x-error-path to extract standardized error
        let mut mapped_error: Option<Value> = None;
        if let Some(err_expr_raw) = action.error_path.as_deref().or_else(|| {
            action
                .extensions
                .get("x-error-path")
                .and_then(|v| v.as_str())
        }) {
            let err_expr = strip_markers(err_expr_raw).to_string();
            let arena = Bump::new();
            if let Ok(engine) = JsonAta::new(&err_expr, &arena) {
                let status_json = serde_json::Value::Number(serde_json::Number::from(final_status));
                let body_json = if let Some(http) = &http_result {
                    http.get("body").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                let mut bindings = std::collections::HashMap::new();
                bindings.insert("status", &status_json);
                bindings.insert("body", &body_json);
                if let Ok(val) = engine.evaluate(None, Some(&bindings)) {
                    mapped_error = Some(jsonata_to_serde(val));
                }
            }
        }

        // 3.9 Apply x-output-pick on success payload, overridden by x-transform-post if present
        let mut output_pick: Option<Value> = None;
        if let Some(pick_expr_raw) = action.output_pick.as_deref().or_else(|| {
            action
                .extensions
                .get("x-output-pick")
                .and_then(|v| v.as_str())
        }) {
            let pick_expr = strip_markers(pick_expr_raw).to_string();
            let arena = Bump::new();
            if let Ok(engine) = JsonAta::new(&pick_expr, &arena) {
                let body_json = if let Some(http) = &http_result {
                    http.get("body").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                let mut bindings = std::collections::HashMap::new();
                bindings.insert("body", &body_json);
                if let Ok(val) = engine.evaluate(None, Some(&bindings)) {
                    output_pick = Some(jsonata_to_serde(val));
                }
            }
        }

        if post_output.is_some() {
            output_pick = post_output;
        }

        let request_info = serde_json::json!({
            "method": action.method,
            "path": action.path,
            "headers": headers,
            "query": query,
            "timeout_ms": effective_timeout_ms,
            "retry": {
                "max_retries": retry_settings.max_retries,
                "base_delay_ms": retry_settings.base_delay_ms,
                "max_delay_ms": retry_settings.max_delay_ms,
                "retry_on": retry_settings.retry_on,
                "respect_retry_after": retry_settings.respect_retry_after,
                "attempts_plan": attempts_plan,
                "attempts_total": attempts_total,
                "retries": retries_done,
                "last_status": final_status,
                "last_error_class": last_error_class
            },
            "attempted_statuses": attempted_statuses,
            "final_status": final_status,
            "ok": ok_flag,
            "error": mapped_error,
            "output": output_pick,
            "auth_provider": auth_context.as_ref().map(|a| &a.provider),
            "auth_scheme": action.auth_config.as_ref().and_then(|a| a.scheme.as_ref()),
            "parameters": context.parameters,
            "timestamp": context.timestamp,
            "status": "executed"
        ,
            "http": http_result
        ,
            "diagnostics": diagnostics
        ,
            "transformed_body": transformed_body,
            "trace": { "events": trace_events, "duration_ms": start_time.elapsed().as_millis() as u64 }
        });

        Ok(request_info)
    }

    /// Set execution timeout
    pub fn set_timeout(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// Set maximum retry attempts
    pub fn set_max_retries(&mut self, max_retries: u32) {
        self.max_retries = max_retries;
    }

    /// Get execution timeout
    pub fn get_timeout(&self) -> u64 {
        self.timeout_ms
    }

    /// Get maximum retry attempts
    pub fn get_max_retries(&self) -> u32 {
        self.max_retries
    }
}

impl Default for ActionRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use serde_json::json;

    fn create_test_action() -> Action {
        let mut action = Action::new(
            "get_user".to_string(),
            "GET".to_string(),
            "/users/{id}".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
        );
        // Ensure validation passes: add required path parameter {id}
        action.add_parameter(
            ActionParameter::new("id".to_string(), ParameterLocation::Path).required(),
        );
        // Attach auth config with mapping to inject headers and query
        action.auth_config = Some(crate::action::auth::AuthConfig {
            connection_trn: "trn:authflow:tenant:connection/github".to_string(),
            scheme: Some("oauth2".to_string()),
            injection: crate::action::auth::InjectionConfig {
                r#type: "jsonada".to_string(),
                mapping: r#"{
                    "headers": {
                        "Authorization": "{% 'Bearer ' & $access_token %}",
                        "X-Static": "fixed"
                    },
                    "query": {
                        "t": "{% $access_token %}"
                    }
                }"#
                .to_string(),
            },
            expiry: None,
            refresh: None,
            failure: None,
        });
        action
    }

    fn create_test_context() -> ActionExecutionContext {
        ActionExecutionContext::new(
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
            "trn:stepflow:tenant123:execution:action-execution:exec-123".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        )
    }

    #[tokio::test]
    async fn test_action_runner_creation() {
        let runner = ActionRunner::new();
        assert_eq!(runner.get_timeout(), 30000);
        assert_eq!(runner.get_max_retries(), 3);
    }

    #[tokio::test]
    async fn test_action_runner_with_timeout() {
        let runner = ActionRunner::with_timeout(60000);
        assert_eq!(runner.get_timeout(), 60000);
        assert_eq!(runner.get_max_retries(), 3);
    }

    #[tokio::test]
    async fn test_execute_action() {
        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new(
            "tenant123".to_string(),
        )));
        let action = create_test_action();
        let context = create_test_context();
        // simulate retry statuses and real http disabled by default
        let mut action = action;
        action
            .extensions
            .insert("x-simulate-statuses".to_string(), json!([503, 503, 200]));

        let result = runner.execute_action(&action, context).await.unwrap();

        assert_eq!(
            result.execution_trn,
            "trn:stepflow:tenant123:execution:action-execution:exec-123"
        );
        assert!(matches!(result.status, ExecutionStatus::Success));
        assert!(result.response_data.is_some());
        let data = result.response_data.unwrap();
        assert_eq!(
            data["headers"]["Authorization"],
            "Bearer ghp_mock_token_12345"
        );
        assert_eq!(data["headers"]["X-Static"], "fixed");
        assert_eq!(data["query"]["t"], "ghp_mock_token_12345");
        assert!(data["attempted_statuses"].is_array());
        assert!(result.duration_ms.is_some());
    }

    #[tokio::test]
    async fn test_context_validation() {
        let runner = ActionRunner::new();
        let action = create_test_action();

        // Test empty action TRN
        let mut context = create_test_context();
        context.action_trn = "".to_string();

        let result = runner.execute_action(&action, context).await.unwrap();
        assert!(matches!(result.status, ExecutionStatus::Failed));
        assert!(result.error_message.is_some());
    }

    #[tokio::test]
    async fn test_runner_configuration() {
        let mut runner = ActionRunner::new();

        runner.set_timeout(45000);
        runner.set_max_retries(5);

        assert_eq!(runner.get_timeout(), 45000);
        assert_eq!(runner.get_max_retries(), 5);
    }

    #[test]
    fn test_retry_should_retry() {
        let retry_on = vec!["5xx".to_string(), "429".to_string(), "408".to_string()];
        assert!(should_retry_for(500, &retry_on));
        assert!(should_retry_for(503, &retry_on));
        assert!(should_retry_for(429, &retry_on));
        assert!(should_retry_for(408, &retry_on));
        assert!(!should_retry_for(404, &retry_on));
        assert!(!should_retry_for(401, &retry_on));
    }

    #[test]
    fn test_retry_backoff() {
        let settings = RetrySettings {
            max_retries: 3,
            base_delay_ms: 200,
            max_delay_ms: 2000,
            retry_on: vec!["5xx".to_string()],
            respect_retry_after: true,
        };
        // Use deterministic RNG by seeding
        let mut rng = StdRng::seed_from_u64(42);
        let b1 = compute_backoff_ms(1, &settings, Some(&mut rng));
        let b2 = compute_backoff_ms(2, &settings, Some(&mut rng));
        let b3 = compute_backoff_ms(3, &settings, Some(&mut rng));
        assert!(b1 >= 180 && b1 <= 220);
        assert!(b2 >= 360 && b2 <= 440);
        assert!(b3 >= 720 && b3 <= 880);
    }

    #[tokio::test]
    async fn test_ok_error_output_mapping_expressions() {
        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new(
            "tenant123".to_string(),
        )));
        let mut action = create_test_action();
        // Simulate final status 400 and map ok/error
        action
            .extensions
            .insert("x-simulate-statuses".to_string(), json!([400]));
        action.extensions.insert(
            "x-ok-path".to_string(),
            json!("$status >= 200 and $status < 300"),
        );
        action.extensions.insert(
            "x-error-path".to_string(),
            json!("{'code': 'E_BAD', 'status': $status}"),
        );
        let context = create_test_context();
        let result = runner.execute_action(&action, context).await.unwrap();
        let data = result.response_data.unwrap();
        assert_eq!(data["ok"], false);
        assert_eq!(data["error"]["code"], "E_BAD");
        assert_eq!(data["error"]["status"].as_f64().unwrap(), 400.0);
    }

    #[test]
    fn test_eval_jsonata_for_pagination_helpers() {
        let body = json!({
            "data": [1,2,3],
            "next": "abc",
            "done": false
        });
        let next = super::eval_jsonata("$body.next", 200, &body).unwrap();
        assert_eq!(next, json!("abc"));
        let items = super::eval_jsonata("$body.data", 200, &body).unwrap();
        assert_eq!(
            items
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as i64)
                .collect::<Vec<i64>>(),
            vec![1, 2, 3]
        );
        let stop = super::eval_jsonata("$body.done", 200, &body).unwrap();
        assert_eq!(stop, json!(false));
    }

    #[tokio::test]
    async fn test_pagination_link_mode() {
        use axum::{routing::get, Router};
        use tokio::task::JoinHandle;

        // Bind to an ephemeral port first to know the actual address
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        // simple pages: /p1 -> items [1], next /p2; /p2 -> items [2], next /p3; /p3 -> items [3], next null
        let app = {
            let b1 = base.clone();
            let b2 = base.clone();
            Router::new()
                .route(
                    "/p1",
                    get(move || {
                        let b1 = b1.clone();
                        async move {
                            axum::Json(
                                serde_json::json!({"items":[1], "next": format!("{}/p2", b1)}),
                            )
                        }
                    }),
                )
                .route(
                    "/p2",
                    get(move || {
                        let b2 = b2.clone();
                        async move {
                            axum::Json(
                                serde_json::json!({"items":[2], "next": format!("{}/p3", b2)}),
                            )
                        }
                    }),
                )
                .route(
                    "/p3",
                    get(|| async { axum::Json(serde_json::json!({"items":[3], "next": null})) }),
                )
        };

        let server: JoinHandle<()> = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Build action
        let mut action = create_test_action();
        action.path = "/p1".to_string();
        // set base url to our server
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base));
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));
        action.extensions.insert(
            "x-pagination".to_string(),
            serde_json::json!({
                "mode": "link",
                "limit": 5,
                "items_expr": "{% $body.items %}",
                "link_expr": "{% $body.next %}"
            }),
        );

        // Execute
        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new(
            "tenant123".to_string(),
        )));
        let context = create_test_context();
        let result = runner.execute_action(&action, context).await.unwrap();
        let data = result.response_data.unwrap();
        assert!(data["http"]["pagination"]["pages"].is_array());
        assert!(data["http"]["pagination"]["items"].is_array());
        let items = data["http"]["pagination"]["items"].as_array().unwrap();
        let nums: Vec<i64> = items
            .iter()
            .filter_map(|v| v.as_f64())
            .map(|f| f as i64)
            .collect();
        assert_eq!(nums, vec![1, 2, 3]);

        // Cleanup
        server.abort();
    }

    #[tokio::test]
    async fn test_pre_post_transformers_and_http_body() {
        use axum::{http::StatusCode, routing::post, Router};
        use tokio::task::JoinHandle;

        // Bind to an ephemeral port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        // Echo endpoint: returns headers/query/body
        async fn echo(
            req: axum::http::Request<axum::body::Body>,
        ) -> (StatusCode, axum::Json<serde_json::Value>) {
            use axum::body::to_bytes;
            use axum::http::HeaderMap;

            let (parts, body) = req.into_parts();
            let headers: HeaderMap = parts.headers.clone();
            let qs = parts.uri.query().unwrap_or("").to_string();
            let body_bytes = to_bytes(body, 1024 * 1024).await.unwrap_or_default();
            let body_text = String::from_utf8(body_bytes.to_vec()).unwrap_or_default();
            let body_json: serde_json::Value =
                serde_json::from_str(&body_text).unwrap_or(serde_json::json!({ "raw": body_text }));

            let mut hdr_map = serde_json::Map::new();
            if let Some(v) = headers.get("X-Pre") {
                hdr_map.insert(
                    "X-Pre".to_string(),
                    serde_json::json!(v.to_str().unwrap_or("")),
                );
            }

            let query_params: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(qs.as_bytes())
                    .into_owned()
                    .collect();

            let resp = serde_json::json!({
                "headers": hdr_map,
                "query": query_params,
                "body": body_json,
                "message": "ok"
            });
            (StatusCode::OK, axum::Json(resp))
        }

        let app = Router::new().route("/echo", post(echo));
        let server: JoinHandle<()> = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Build action
        let mut action = Action::new(
            "post_echo".to_string(),
            "POST".to_string(),
            "/echo".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/post_echo:provider/example".to_string(),
        );
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base));
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));
        // pre: set header/query/body
        let pre_mapping = r#"{\n  \"headers\": { \"X-Pre\": \"A\" },\n  \"query\": { \"q\": \"1\" },\n  \"body\": { \"k\": \"v\" }\n}"#;
        action.extensions.insert(
            "x-transform-pre".to_string(),
            serde_json::json!([pre_mapping]),
        );
        // post: rename output
        let post_expr = "{% {'renamed_body': $body.body, 'pre_header': $body.headers['X-Pre'], 'pre_query': $body.query.q} %}";
        action.extensions.insert(
            "x-transform-post".to_string(),
            serde_json::json!([post_expr]),
        );

        // Run
        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new(
            "tenant123".to_string(),
        )));
        let ctx = ActionExecutionContext::new(
            action.trn.clone(),
            "trn:stepflow:tenant123:execution:action-execution:exec-prepost".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        );
        let result = runner.execute_action(&action, ctx).await.unwrap();
        assert!(matches!(result.status, ExecutionStatus::Success));
        let data = result.response_data.unwrap();
        // verify http body observed
        assert_eq!(
            data["http"]["body"]["headers"]["X-Pre"],
            serde_json::json!("A")
        );
        assert_eq!(data["http"]["body"]["query"]["q"], serde_json::json!("1"));
        assert_eq!(data["http"]["body"]["body"]["k"], serde_json::json!("v"));
        // verify post output mapping
        assert_eq!(data["output"]["renamed_body"]["k"], serde_json::json!("v"));
        assert_eq!(data["output"]["pre_header"], serde_json::json!("A"));
        assert_eq!(data["output"]["pre_query"], serde_json::json!("1"));

        server.abort();
    }

    #[tokio::test]
    async fn test_pagination_page_mode() {
        use axum::{routing::get, Router};
        use tokio::task::JoinHandle;

        // Bind ephemeral
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        // /items?page=N&per_page=M -> returns { items: [start..start+M-1] } until 3 pages
        #[derive(serde::Deserialize)]
        struct Q { page: Option<u64>, per_page: Option<u64> }

        let app = Router::new().route(
            "/items",
            get(|axum::extract::Query(q): axum::extract::Query<Q>| async move {
                let page = q.page.unwrap_or(1);
                let per = q.per_page.unwrap_or(2);
                let start = (page - 1) * per;
                let items: Vec<u64> = (start..start + per).collect();
                axum::Json(serde_json::json!({"items": items}))
            }),
        );
        let server: JoinHandle<()> = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

        // Build action with page mode pagination config
        let mut action = Action::new(
            "list_items".to_string(),
            "GET".to_string(),
            "/items".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/list_items:provider/example".to_string(),
        );
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base));
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));
        action.extensions.insert(
            "x-pagination".to_string(),
            serde_json::json!({"mode": "page", "param": "page", "limit": 3}),
        );

        // Execute with all_pages and per_page=2
        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new("tenant123".to_string())));
        let mut ctx = ActionExecutionContext::new(
            action.trn.clone(),
            "trn:stepflow:tenant123:execution:action-execution:exec-page".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        );
        ctx.add_parameter("page".to_string(), serde_json::json!(1));
        ctx.add_parameter("per_page".to_string(), serde_json::json!(2));
        let result = runner.execute_action(&action, ctx).await.unwrap();
        // page mode aggregation is handled in core when --all-pages; here runner single-call returns page 1
        assert!(result.response_data.is_some());

        server.abort();
    }

    #[tokio::test]
    async fn test_ndjson_stream() {
        use axum::{routing::get, Router};
        use tokio::task::JoinHandle;
        use axum::response::{IntoResponse, Response};

        // Bind ephemeral
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        async fn stream_ndjson() -> Response {
            let body = "{\"a\":1}\n{\"b\":2}\n";
            ([("content-type", "application/x-ndjson")], body).into_response()
        }

        let app = Router::new().route("/stream", get(stream_ndjson));
        let server: JoinHandle<()> = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

        let mut action = Action::new(
            "ndjson".to_string(),
            "GET".to_string(),
            "/stream".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/ndjson:provider/example".to_string(),
        );
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base));
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));

        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new("tenant123".to_string())));
        let ctx = ActionExecutionContext::new(
            action.trn.clone(),
            "trn:stepflow:tenant123:execution:action-execution:exec-ndjson".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        );
        let result = runner.execute_action(&action, ctx).await.unwrap();
        let data = result.response_data.unwrap();
        assert!(data["http"]["body"].is_array());
        assert_eq!(data["http"]["body"].as_array().unwrap().len(), 2);

        server.abort();
    }

    #[tokio::test]
    async fn test_multipart_upload() {
        use axum::{routing::post, Router};
        use tokio::task::JoinHandle;
        use axum::http::StatusCode;
        use axum::extract::Request;
        use axum::body::to_bytes;

        // Bind ephemeral
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        async fn upload(req: Request) -> (StatusCode, axum::Json<serde_json::Value>) {
            let (parts, body) = req.into_parts();
            let ct = parts.headers.get("content-type").and_then(|h| h.to_str().ok()).unwrap_or("").to_string();
            let bytes = to_bytes(body, 1024 * 1024).await.unwrap_or_default();
            let text = String::from_utf8(bytes.to_vec()).unwrap_or_default();
            let ok = ct.contains("multipart/form-data") && text.contains("name=\"file\"") && text.contains("name=\"a\"");
            (StatusCode::OK, axum::Json(serde_json::json!({"ok": ok, "content_type": ct })))
        }

        let app = Router::new().route("/upload", post(upload));
        let server: JoinHandle<()> = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

        // Prepare temp file
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("demo.txt");
        std::fs::write(&file_path, b"hello").unwrap();

        // Build action with pre transform to construct multipart body
        let mut action = Action::new(
            "upload".to_string(),
            "POST".to_string(),
            "/upload".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/upload:provider/example".to_string(),
        );
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base));
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));
        let mp_mapping = format!(
            "{{ \"body\": {{ \"_multipart\": {{ \"fields\": {{\"a\": \"b\"}}, \"files\": [{{\"field\": \"file\", \"path\": \"{}\"}}] }} }} }}",
            file_path.display()
        );
        action.extensions.insert(
            "x-transform-pre".to_string(),
            serde_json::json!([mp_mapping]),
        );

        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(std::sync::Arc::new(AuthAdapter::new("tenant123".to_string())));
        let ctx = ActionExecutionContext::new(
            action.trn.clone(),
            "trn:stepflow:tenant123:execution:action-execution:exec-upload".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        );
        let result = runner.execute_action(&action, ctx).await.unwrap();
        let data = result.response_data.unwrap();
        assert_eq!(data["http"]["status"], serde_json::json!(200));
        assert_eq!(data["http"]["body"]["ok"], serde_json::json!(true));

        server.abort();
    }
}

// ----------------------
// Retry helpers
// ----------------------

#[derive(Clone, Debug)]
struct RetrySettings {
    max_retries: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
    retry_on: Vec<String>,
    respect_retry_after: bool,
}

fn resolve_retry_settings(action: &Action) -> RetrySettings {
    if let Some(typed) = &action.retry {
        return RetrySettings {
            max_retries: typed.max_retries,
            base_delay_ms: typed.base_delay_ms,
            max_delay_ms: typed.max_delay_ms,
            retry_on: typed.retry_on.clone(),
            respect_retry_after: typed.respect_retry_after,
        };
    }
    let x = action.extensions.get("x-retry").and_then(|v| v.as_object());
    RetrySettings {
        max_retries: x
            .and_then(|m| m.get("max_retries").and_then(|v| v.as_u64()))
            .unwrap_or(3) as u32,
        base_delay_ms: x
            .and_then(|m| m.get("base_delay_ms").and_then(|v| v.as_u64()))
            .unwrap_or(500),
        max_delay_ms: x
            .and_then(|m| m.get("max_delay_ms").and_then(|v| v.as_u64()))
            .unwrap_or(10_000),
        retry_on: x
            .and_then(|m| {
                m.get("retry_on").and_then(|v| v.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
            })
            .unwrap_or_else(|| vec!["5xx".to_string(), "429".to_string()]),
        respect_retry_after: x
            .and_then(|m| m.get("respect_retry_after").and_then(|v| v.as_bool()))
            .unwrap_or(true),
    }
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    if let Some(val) = headers.get(RETRY_AFTER) {
        if let Ok(s) = val.to_str() {
            if let Ok(sec) = s.parse::<u64>() {
                return Some(Duration::from_secs(sec));
            }
        }
    }
    None
}

#[allow(dead_code)]
fn should_retry_for(status_code: u16, retry_on: &Vec<String>) -> bool {
    if retry_on.iter().any(|s| s == "5xx") && (500..=599).contains(&status_code) {
        return true;
    }
    if retry_on.iter().any(|s| s == "429") && status_code == 429 {
        return true;
    }
    if retry_on.iter().any(|s| s == "408") && status_code == 408 {
        return true;
    }
    // Specific codes
    retry_on.iter().any(|s| {
        s.parse::<u16>()
            .ok()
            .map(|c| c == status_code)
            .unwrap_or(false)
    })
}

#[allow(dead_code)]
fn compute_backoff_ms(attempt: u32, settings: &RetrySettings, mut rng: Option<&mut StdRng>) -> u64 {
    // Exponential backoff: base * 2^(attempt-1), capped at max
    let shift = (attempt.saturating_sub(1)).min(30); // prevent overflow
    let pow = 1u64 << shift;
    let mut delay = settings.base_delay_ms.saturating_mul(pow);
    if delay > settings.max_delay_ms {
        delay = settings.max_delay_ms;
    }
    // Small jitter +/-10% if rng provided
    if let Some(r) = rng.as_deref_mut() {
        let jitter = (delay as f64 * 0.1) as u64; // 10%
        let delta: i64 = r.gen_range(-(jitter as i64)..=(jitter as i64));
        let with = if delta < 0 {
            delay.saturating_sub((-delta) as u64)
        } else {
            delay.saturating_add(delta as u64)
        };
        return with;
    }
    delay
}

fn resolve_method(method: &str) -> Method {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "PATCH" => Method::PATCH,
        "HEAD" => Method::HEAD,
        "OPTIONS" => Method::OPTIONS,
        _ => Method::GET,
    }
}

fn resolve_url(action: &Action, query: &std::collections::HashMap<String, String>) -> Option<Url> {
    // Prefer explicit x-url
    if let Some(u) = action.extensions.get("x-url").and_then(|v| v.as_str()) {
        if let Ok(mut url) = Url::parse(u) {
            if !query.is_empty() {
                let mut pairs = url.query_pairs_mut();
                for (k, v) in query.iter() {
                    pairs.append_pair(k, v);
                }
            }
            return Some(url);
        }
    }
    // Fallback to x-base-url + action.path
    if let Some(base) = action.extensions.get("x-base-url").and_then(|v| v.as_str()) {
        if let Ok(mut url) = Url::parse(base) {
            // combine path
            let base_path = url.path().trim_end_matches('/');
            let act_path = action.path.trim_start_matches('/');
            let combined = format!("{}/{}", base_path, act_path);
            url.set_path(&combined);
            if !query.is_empty() {
                let mut pairs = url.query_pairs_mut();
                for (k, v) in query.iter() {
                    pairs.append_pair(k, v);
                }
            }
            return Some(url);
        }
    }
    None
}

fn strip_markers(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.starts_with("%}") || trimmed.ends_with("{%") {
        return s;
    }
    if trimmed.starts_with("{%") && trimmed.ends_with("%}") {
        let inner = &trimmed[2..trimmed.len() - 2];
        return inner.trim();
    }
    s
}

fn jsonata_to_serde<'a>(v: &'a jsonata_rs::Value<'a>) -> Value {
    if v.is_null() {
        return Value::Null;
    }
    if v.is_bool() {
        return Value::Bool(v.as_bool());
    }
    if v.is_number() {
        return serde_json::json!(v.as_f64());
    }
    if v.is_string() {
        return Value::String(v.as_str().to_string());
    }
    if v.is_array() {
        let items: Vec<Value> = v.members().map(|vv| jsonata_to_serde(vv)).collect();
        return Value::Array(items);
    }
    if v.is_object() {
        let mut map = serde_json::Map::new();
        for (k, vv) in v.entries() {
            map.insert(k.to_string(), jsonata_to_serde(vv));
        }
        return Value::Object(map);
    }
    Value::Null
}

fn eval_jsonata(expr_raw: &str, status: u16, body: &Value) -> Option<Value> {
    let arena = Bump::new();
    let expr = strip_markers(expr_raw);
    let engine = JsonAta::new(expr, &arena).ok()?;
    let status_json = serde_json::Value::Number(serde_json::Number::from(status));
    let mut bindings = std::collections::HashMap::new();
    bindings.insert("status", &status_json);
    bindings.insert("body", body);
    let v = engine.evaluate(None, Some(&bindings)).ok()?;
    Some(jsonata_to_serde(v))
}

fn env_var_candidates_for_key(key: &str) -> Vec<String> {
    let upper = key.replace('-', "_").to_uppercase();
    vec![upper]
}

fn missing_secret_keys_for_mapping(
    mapping: &str,
    ctx: &super::expression_engine::ExpressionContext,
) -> Option<Vec<String>> {
    // naive scan for vars.secrets.<key>
    let mut keys: Vec<String> = Vec::new();
    let needle = "vars.secrets.";
    let bytes = mapping.as_bytes();
    let mut i: usize = 0;
    while let Some(pos) = mapping[i..].find(needle) {
        let start = i + pos + needle.len();
        let mut end = start;
        while end < bytes.len() {
            let c = bytes[end] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                end += 1;
            } else {
                break;
            }
        }
        if end > start {
            let key = mapping[start..end].to_string();
            if !keys.iter().any(|k| k == &key) {
                keys.push(key);
            }
        }
        i = end;
        if i >= bytes.len() {
            break;
        }
    }
    if keys.is_empty() {
        return Some(Vec::new());
    }
    // check availability in ctx.secrets or env/file
    let mut missing: Vec<String> = Vec::new();
    for k in keys {
        let mut has = false;
        if let Some(obj) = ctx.ctx.get("secrets").and_then(|v| v.as_object()) {
            if obj.contains_key(&k) {
                has = true;
            }
        }
        if !has {
            for cand in env_var_candidates_for_key(&k) {
                if std::env::var(&cand).is_ok() {
                    has = true;
                    break;
                }
            }
        }
        if !has {
            missing.push(k);
        }
    }
    Some(missing)
}
