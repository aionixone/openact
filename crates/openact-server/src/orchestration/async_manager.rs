use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use jsonpath_lib::Compiled as JsonPathCompiled;
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorRunRecord, OrchestratorRunStatus,
};
use regex::Regex;
use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Map, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::Instrument;

use super::{OutboxService, RunService};
use crate::orchestration::StepflowCommandAdapter;

/// Minimal async task manager that will orchestrate background tracking.
#[derive(Clone)]
pub struct AsyncTaskManager {
    run_service: RunService,
    outbox_service: OutboxService,
    http_client: Client,
}

impl AsyncTaskManager {
    pub fn new(run_service: RunService, outbox_service: OutboxService) -> Self {
        Self { run_service, outbox_service, http_client: Client::new() }
    }

    pub fn submit(&self, run: OrchestratorRunRecord, handle: Value) -> Result<()> {
        let manager = self.clone();
        tokio::spawn(
            async move {
                if let Err(err) = manager.observe(run.clone(), handle.clone()).await {
                    tracing::error!(
                        error = %err,
                        run_id = %run.run_id,
                        "async task manager failed while observing run"
                    );
                }
            }
            .instrument(tracing::info_span!("async_task_register")),
        );
        Ok(())
    }

    pub async fn cancel_run(
        &self,
        run: &OrchestratorRunRecord,
        handle: &Value,
        reason: Option<&str>,
    ) -> Result<()> {
        let Some(plan) = cancel_plan_from_handle(handle)? else {
            tracing::debug!(run_id = %run.run_id, "no cancel plan configured" );
            return Ok(());
        };

        let Some(external_run_id) = handle
            .as_object()
            .and_then(|map| map.get("externalRunId"))
            .and_then(|value| value.as_str())
        else {
            tracing::warn!(run_id = %run.run_id, "cancel plan configured but externalRunId missing");
            return Ok(());
        };

        let ctx = TemplateContext { external_run_id, reason };
        let url = render_template(&plan.url_template, &ctx);
        let mut request = self.http_client.request(plan.method.clone(), url);

        for (key, value) in &plan.headers {
            request = request.header(key, render_template(value, &ctx));
        }

        if let Some(body) = &plan.body {
            request = request.json(&render_value(body, &ctx));
        }

        match request.send().await {
            Ok(response) if response.status().is_success() => {
                tracing::info!(run_id = %run.run_id, "cancel request accepted" );
                Ok(())
            }
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                tracing::warn!(
                    run_id = %run.run_id,
                    status = %status,
                    body = %text,
                    "cancel request responded with non-success"
                );
                Ok(())
            }
            Err(err) => {
                tracing::warn!(run_id = %run.run_id, error = %err, "cancel request failed");
                Ok(())
            }
        }
    }

    async fn observe(&self, run: OrchestratorRunRecord, handle: Value) -> Result<()> {
        let backend = handle
            .as_object()
            .and_then(|map| map.get("backendId"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let external_run_id = handle
            .as_object()
            .and_then(|map| map.get("externalRunId"))
            .and_then(|value| value.as_str())
            .map(|s| s.to_string());

        match tracker_plan_from_handle(&handle)? {
            TrackerPlan::Noop => {
                tracing::info!(
                    run_id = %run.run_id,
                    backend,
                    external_run_id,
                    "async handle registered (noop tracker)"
                );
                Ok(())
            }
            TrackerPlan::MockComplete { delay_ms, result } => {
                if let Some(delay) = delay_ms {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                tracing::info!(
                    run_id = %run.run_id,
                    backend,
                    external_run_id,
                    "mock tracker completing run successfully"
                );
                self.complete_run_success(&run, result).await
            }
            TrackerPlan::MockFail { delay_ms, error } => {
                if let Some(delay) = delay_ms {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                tracing::info!(
                    run_id = %run.run_id,
                    backend,
                    external_run_id,
                    "mock tracker marking run failed"
                );
                self.complete_run_failure(&run, error).await
            }
            TrackerPlan::HttpPoll(plan) => {
                tracing::info!(
                    run_id = %run.run_id,
                    backend,
                    external_run_id,
                    url = %plan.url_template,
                    "starting http polling tracker"
                );
                self.run_http_poll(plan, run, external_run_id).await
            }
            TrackerPlan::Unsupported => {
                tracing::warn!(
                    run_id = %run.run_id,
                    backend,
                    external_run_id,
                    "unsupported tracker configuration; no action taken"
                );
                Ok(())
            }
        }
    }

    async fn complete_run_success(&self, run: &OrchestratorRunRecord, result: Value) -> Result<()> {
        self.run_service
            .update_status(
                &run.run_id,
                OrchestratorRunStatus::Succeeded,
                Some("succeeded".to_string()),
                Some(result.clone()),
                None,
            )
            .await
            .context("failed to mark run succeeded")?;

        let event = StepflowCommandAdapter::build_success_event(run, &result);
        self.outbox_service
            .enqueue(OrchestratorOutboxInsert {
                run_id: Some(run.run_id.clone()),
                protocol: "aionix.event.stepflow".to_string(),
                payload: event,
                next_attempt_at: Utc::now(),
                attempts: 0,
                last_error: None,
            })
            .await
            .context("failed to enqueue success event")?;
        Ok(())
    }

    async fn complete_run_failure(&self, run: &OrchestratorRunRecord, error: Value) -> Result<()> {
        self.run_service
            .update_status(
                &run.run_id,
                OrchestratorRunStatus::Failed,
                Some("failed".to_string()),
                None,
                Some(error.clone()),
            )
            .await
            .context("failed to mark run failed")?;

        let event = StepflowCommandAdapter::build_failure_event(run, &error);
        self.outbox_service
            .enqueue(OrchestratorOutboxInsert {
                run_id: Some(run.run_id.clone()),
                protocol: "aionix.event.stepflow".to_string(),
                payload: event,
                next_attempt_at: Utc::now(),
                attempts: 0,
                last_error: None,
            })
            .await
            .context("failed to enqueue failure event")?;
        Ok(())
    }

    #[allow(unused_assignments)]
    async fn run_http_poll(
        &self,
        plan: HttpPollingPlan,
        run: OrchestratorRunRecord,
        external_run_id: Option<String>,
    ) -> Result<()> {
        let ext_id = external_run_id.unwrap_or_default();
        let base_interval = Duration::from_millis(plan.interval_ms);
        let timeout = plan.timeout_ms.map(Duration::from_millis);
        let start = Instant::now();
        let mut attempts: u32 = 0;
        let mut last_error = json!({
            "message": "http poll exhausted without terminal status"
        });

        loop {
            attempts += 1;
            match self.execute_http_poll_request(&plan, &ext_id).await {
                Ok((status, body)) => {
                    let code = status.as_u16();
                    let body_snapshot = body.clone();

                    if plan.is_success(code) || plan.body_success(&body_snapshot) {
                        let result = plan.extract_result(body_snapshot);
                        return self.complete_run_success(&run, result).await;
                    }

                    if plan.is_failure(code) || plan.body_failure(&body_snapshot) {
                        let error_payload = json!({
                            "message": "http poll returned failure",
                            "status": code,
                            "body": body,
                        });
                        return self.complete_run_failure(&run, error_payload).await;
                    }

                    last_error = json!({
                        "message": "http poll status/body did not match",
                        "status": code,
                        "body": body,
                    });
                }
                Err(err) => {
                    last_error = json!({
                        "message": "http poll request error",
                        "error": err.to_string(),
                    });
                }
            }

            if let Err(err) =
                self.run_service.refresh_heartbeat(&run.run_id, Utc::now(), run.deadline_at).await
            {
                tracing::warn!(error = %err, run_id = %run.run_id, "failed to refresh heartbeat");
            }

            if !plan.should_retry(attempts, start, timeout) {
                return self.complete_run_failure(&run, last_error.clone()).await;
            }

            let sleep = plan.backoff_sleep(attempts, base_interval);
            tokio::time::sleep(sleep).await;
        }
    }

    async fn execute_http_poll_request(
        &self,
        plan: &HttpPollingPlan,
        external_run_id: &str,
    ) -> Result<(StatusCode, Value)> {
        let ctx = TemplateContext { external_run_id, reason: None };
        let url = render_template(&plan.url_template, &ctx);
        let mut request = self.http_client.request(plan.method.clone(), url);

        for (key, value) in &plan.headers {
            request = request.header(key, render_template(value, &ctx));
        }

        if let Some(body) = &plan.body {
            request = request.json(&render_value(body, &ctx));
        }

        let response = request.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        let body_value = serde_json::from_slice::<Value>(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        Ok((status, body_value))
    }
}

#[derive(Debug)]
enum TrackerPlan {
    Noop,
    MockComplete { delay_ms: Option<u64>, result: Value },
    MockFail { delay_ms: Option<u64>, error: Value },
    HttpPoll(HttpPollingPlan),
    Unsupported,
}

fn tracker_plan_from_handle(handle: &Value) -> Result<TrackerPlan> {
    let tracker = handle
        .as_object()
        .and_then(|map| map.get("config"))
        .and_then(|config| config.as_object())
        .and_then(|cfg| cfg.get("tracker"));

    let tracker_map: &Map<String, Value> = match tracker {
        Some(Value::Object(map)) => map,
        None => return Ok(TrackerPlan::Noop),
        _ => return Ok(TrackerPlan::Unsupported),
    };

    let kind = tracker_map.get("kind").and_then(|value| value.as_str()).unwrap_or("unsupported");

    let plan = match kind {
        "noop" => TrackerPlan::Noop,
        "mock_complete" => {
            let delay_ms = tracker_map.get("delay_ms").and_then(|value| value.as_u64());
            let result =
                tracker_map.get("result").cloned().unwrap_or_else(|| Value::Object(Map::new()));
            TrackerPlan::MockComplete { delay_ms, result }
        }
        "mock_fail" => {
            let delay_ms = tracker_map.get("delay_ms").and_then(|value| value.as_u64());
            let error =
                tracker_map.get("error").cloned().unwrap_or_else(|| Value::Object(Map::new()));
            TrackerPlan::MockFail { delay_ms, error }
        }
        "http_poll" => TrackerPlan::HttpPoll(HttpPollingPlan::from_map(tracker_map)?),
        _ => TrackerPlan::Unsupported,
    };

    Ok(plan)
}

fn cancel_plan_from_handle(handle: &Value) -> Result<Option<CancelPlan>> {
    let cancel = handle
        .as_object()
        .and_then(|map| map.get("config"))
        .and_then(|config| config.as_object())
        .and_then(|cfg| cfg.get("cancel"));

    let Some(Value::Object(map)) = cancel else {
        return Ok(None);
    };

    let plan = CancelPlan::from_map(map)?;
    Ok(Some(plan))
}

#[derive(Clone, Debug)]
struct CancelPlan {
    method: Method,
    url_template: String,
    headers: Vec<(String, String)>,
    body: Option<Value>,
}

impl CancelPlan {
    fn from_map(map: &Map<String, Value>) -> Result<Self> {
        let url = map
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("cancel plan requires url"))?
            .to_string();

        let method = map
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("POST")
            .parse::<Method>()
            .map_err(|_| anyhow!("invalid cancel http method"))?;

        let headers = map
            .get("headers")
            .and_then(|value| value.as_object())
            .map(|obj| {
                obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect()
            })
            .unwrap_or_default();

        let body = map.get("body").cloned();

        Ok(Self { method, url_template: url, headers, body })
    }
}

#[derive(Clone, Debug)]
struct HttpPollingPlan {
    method: Method,
    url_template: String,
    headers: Vec<(String, String)>,
    body: Option<Value>,
    interval_ms: u64,
    timeout_ms: Option<u64>,
    max_attempts: Option<u32>,
    backoff_factor: f64,
    max_elapsed_ms: Option<u64>,
    success_status: Vec<u16>,
    failure_status: Vec<u16>,
    success_conditions: Vec<HttpCondition>,
    failure_conditions: Vec<HttpCondition>,
    result_pointer: Option<String>,
}

impl HttpPollingPlan {
    fn from_map(map: &Map<String, Value>) -> Result<Self> {
        let url = map
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("tracker.http_poll requires url"))?
            .to_string();

        let method = map
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("GET")
            .parse::<Method>()
            .map_err(|_| anyhow!("invalid http method for tracker"))?;

        let headers = map
            .get("headers")
            .and_then(|value| value.as_object())
            .map(|obj| {
                obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect()
            })
            .unwrap_or_default();

        let body = map.get("body").cloned();
        let interval_ms = map.get("interval_ms").and_then(|value| value.as_u64()).unwrap_or(1_000);
        let timeout_ms = map.get("timeout_ms").and_then(|value| value.as_u64());
        let max_attempts =
            map.get("max_attempts").and_then(|value| value.as_u64()).map(|v| v as u32);
        let backoff_factor = map
            .get("backoff_factor")
            .and_then(|value| value.as_f64())
            .filter(|v| v.is_finite() && *v >= 1.0)
            .unwrap_or(1.0);
        let max_elapsed_ms = map.get("max_elapsed_ms").and_then(|value| value.as_u64());

        let success_status =
            parse_status_list(map.get("success_status")).unwrap_or_else(|| vec![200]);
        let failure_status = parse_status_list(map.get("failure_status")).unwrap_or_default();
        let success_conditions = parse_conditions(map.get("success_conditions"))?;
        let failure_conditions = parse_conditions(map.get("failure_conditions"))?;

        let result_pointer =
            map.get("result_pointer").and_then(|value| value.as_str()).map(|s| s.to_string());

        Ok(Self {
            method,
            url_template: url,
            headers,
            body,
            interval_ms,
            timeout_ms,
            max_attempts,
            backoff_factor,
            max_elapsed_ms,
            success_status,
            failure_status,
            success_conditions,
            failure_conditions,
            result_pointer,
        })
    }

    fn is_success(&self, status: u16) -> bool {
        if self.success_status.is_empty() {
            status == 200
        } else {
            self.success_status.contains(&status)
        }
    }

    fn is_failure(&self, status: u16) -> bool {
        self.failure_status.contains(&status)
    }

    fn body_success(&self, body: &Value) -> bool {
        if self.success_conditions.is_empty() {
            false
        } else {
            self.success_conditions.iter().any(|cond| cond.matches(body))
        }
    }

    fn body_failure(&self, body: &Value) -> bool {
        if self.failure_conditions.is_empty() {
            false
        } else {
            self.failure_conditions.iter().any(|cond| cond.matches(body))
        }
    }

    fn should_retry(&self, attempts: u32, start: Instant, timeout: Option<Duration>) -> bool {
        if let Some(max) = self.max_attempts {
            if attempts >= max {
                return false;
            }
        }
        if let Some(timeout_ms) = self.timeout_ms {
            if start.elapsed() >= Duration::from_millis(timeout_ms) {
                return false;
            }
        }
        if let Some(max_elapsed) = self.max_elapsed_ms {
            if start.elapsed() >= Duration::from_millis(max_elapsed) {
                return false;
            }
        }
        if let Some(limit) = timeout {
            if start.elapsed() >= limit {
                return false;
            }
        }
        true
    }

    fn backoff_sleep(&self, attempts: u32, base_interval: Duration) -> Duration {
        if self.backoff_factor <= 1.0 {
            return base_interval;
        }
        let exponent = attempts.saturating_sub(1) as i32;
        let factor = self.backoff_factor.powi(exponent);
        let millis = (base_interval.as_millis() as f64 * factor).clamp(0.0, u64::MAX as f64);
        Duration::from_millis(millis as u64)
    }

    fn extract_result(&self, body: Value) -> Value {
        if let Some(pointer) = self.result_pointer.as_deref() {
            match &body {
                Value::Object(_) | Value::Array(_) => {
                    body.pointer(pointer).cloned().unwrap_or(Value::Null)
                }
                _ => Value::Null,
            }
        } else {
            body
        }
    }
}

fn parse_status_list(value: Option<&Value>) -> Option<Vec<u16>> {
    value.and_then(|val| {
        val.as_array()
            .map(|arr| arr.iter().filter_map(|item| item.as_u64().map(|v| v as u16)).collect())
    })
}

#[derive(Clone, Copy)]
struct TemplateContext<'a> {
    external_run_id: &'a str,
    reason: Option<&'a str>,
}

fn render_template(input: &str, ctx: &TemplateContext<'_>) -> String {
    let mut rendered = input.replace("{{externalRunId}}", ctx.external_run_id);
    if let Some(reason) = ctx.reason {
        rendered = rendered.replace("{{reason}}", reason);
    }
    rendered
}

fn render_value(value: &Value, ctx: &TemplateContext<'_>) -> Value {
    match value {
        Value::String(s) => Value::String(render_template(s, ctx)),
        Value::Array(arr) => Value::Array(arr.iter().map(|v| render_value(v, ctx)).collect()),
        Value::Object(map) => {
            let rendered: Map<String, Value> =
                map.iter().map(|(k, v)| (k.clone(), render_value(v, ctx))).collect();
            Value::Object(rendered)
        }
        other => other.clone(),
    }
}

#[derive(Clone, Debug)]
struct HttpCondition {
    pointer: String,
    operator: HttpConditionOperator,
}

impl HttpCondition {
    fn matches(&self, body: &Value) -> bool {
        match body.pointer(&self.pointer) {
            Some(value) => self.operator.compare(value),
            None => self.operator.compare_missing(body),
        }
    }
}

#[derive(Clone, Debug)]
enum HttpConditionOperator {
    Exists,
    Equals(Value),
    NotEquals(Value),
    Contains(String),
    Regex(Regex),
    JsonPath(JsonPathCondition),
    NumberCompare(NumberComparison, f64),
}

#[derive(Clone, Debug)]
struct JsonPathCondition {
    compiled: Arc<JsonPathCompiled>,
    equals: Option<Value>,
    exists: bool,
}

#[derive(Clone, Copy, Debug)]
enum NumberComparison {
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

impl HttpConditionOperator {
    fn compare(&self, actual: &Value) -> bool {
        match self {
            HttpConditionOperator::Exists => !actual.is_null(),
            HttpConditionOperator::Equals(expected) => values_equal(actual, expected),
            HttpConditionOperator::NotEquals(expected) => !values_equal(actual, expected),
            HttpConditionOperator::Contains(needle) => {
                actual.as_str().map(|haystack| haystack.contains(needle)).unwrap_or(false)
            }
            HttpConditionOperator::Regex(pattern) => {
                actual.as_str().map(|haystack| pattern.is_match(haystack)).unwrap_or(false)
            }
            HttpConditionOperator::JsonPath(cond) => match cond.compiled.select(actual) {
                Ok(matches) => {
                    if let Some(expected) = &cond.equals {
                        matches.iter().any(|value| values_equal(value, expected))
                    } else {
                        let exists = !matches.is_empty();
                        if cond.exists {
                            exists
                        } else {
                            !exists
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "jsonpath evaluation failed");
                    false
                }
            },
            HttpConditionOperator::NumberCompare(cmp, expected) => actual
                .as_f64()
                .map(|value| match cmp {
                    NumberComparison::Greater => value > *expected,
                    NumberComparison::GreaterOrEqual => value >= *expected,
                    NumberComparison::Less => value < *expected,
                    NumberComparison::LessOrEqual => value <= *expected,
                })
                .unwrap_or(false),
        }
    }

    fn compare_missing(&self, body: &Value) -> bool {
        match self {
            HttpConditionOperator::JsonPath(cond) => {
                if !cond.exists {
                    matches!(cond.compiled.select(body), Ok(matches) if matches.is_empty())
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

fn parse_conditions(value: Option<&Value>) -> Result<Vec<HttpCondition>> {
    let mut result = Vec::new();
    if let Some(Value::Array(arr)) = value {
        for entry in arr {
            let obj = entry
                .as_object()
                .ok_or_else(|| anyhow!("http_poll condition must be an object"))?;
            let pointer = obj.get("pointer").and_then(|v| v.as_str()).unwrap_or("/").to_string();
            let operator = parse_condition_operator(obj)?;
            result.push(HttpCondition { pointer, operator });
        }
    }
    Ok(result)
}

fn parse_condition_operator(map: &Map<String, Value>) -> Result<HttpConditionOperator> {
    let mut op: Option<HttpConditionOperator> = None;

    if let Some(v) = map.get("equals") {
        op = Some(HttpConditionOperator::Equals(v.clone()));
    }
    if let Some(v) = map.get("not_equals") {
        ensure_single_operator(&op, "not_equals")?;
        op = Some(HttpConditionOperator::NotEquals(v.clone()));
    }
    if let Some(v) = map.get("contains") {
        ensure_single_operator(&op, "contains")?;
        let s = v
            .as_str()
            .ok_or_else(|| anyhow!("http_poll condition.contains must be a string"))?
            .to_string();
        op = Some(HttpConditionOperator::Contains(s));
    }
    if let Some(v) = map.get("regex") {
        ensure_single_operator(&op, "regex")?;
        let pattern =
            v.as_str().ok_or_else(|| anyhow!("http_poll condition.regex must be a string"))?;
        let re = Regex::new(pattern)
            .map_err(|err| anyhow!("invalid regex in http_poll condition: {}", err))?;
        op = Some(HttpConditionOperator::Regex(re));
    }
    if let Some(v) = map.get("jsonpath") {
        ensure_single_operator(&op, "jsonpath")?;
        let expr =
            v.as_str().ok_or_else(|| anyhow!("http_poll condition.jsonpath must be a string"))?;
        let compiled = JsonPathCompiled::compile(expr)
            .map_err(|err| anyhow!("invalid jsonpath expression: {}", err))?;
        let equals = map.get("equals").cloned();
        let exists = map.get("exists").and_then(|v| v.as_bool()).unwrap_or(true);
        op = Some(HttpConditionOperator::JsonPath(JsonPathCondition {
            compiled: Arc::new(compiled),
            equals,
            exists,
        }));
    }

    for (key, cmp) in [
        ("greater_than", NumberComparison::Greater),
        ("greater_or_equal", NumberComparison::GreaterOrEqual),
        ("less_than", NumberComparison::Less),
        ("less_or_equal", NumberComparison::LessOrEqual),
    ] {
        if let Some(v) = map.get(key) {
            ensure_single_operator(&op, key)?;
            let expected = v
                .as_f64()
                .or_else(|| v.as_i64().map(|value| value as f64))
                .ok_or_else(|| anyhow!("http_poll condition.{} must be a number", key))?;
            op = Some(HttpConditionOperator::NumberCompare(cmp, expected));
        }
    }

    Ok(op.unwrap_or(HttpConditionOperator::Exists))
}

fn ensure_single_operator(op: &Option<HttpConditionOperator>, name: &str) -> Result<()> {
    if op.is_some() {
        Err(anyhow!("multiple operators specified in http_poll condition (conflict with {})", name))
    } else {
        Ok(())
    }
}

fn values_equal(lhs: &Value, rhs: &Value) -> bool {
    match (lhs, rhs) {
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => a == b,
        (Value::Object(a), Value::Object(b)) => a == b,
        _ => lhs == rhs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_condition_equals() {
        let cond = HttpCondition {
            pointer: "/state".into(),
            operator: HttpConditionOperator::Equals(Value::String("DONE".into())),
        };
        let payload = json!({"state": "DONE"});
        assert!(cond.matches(&payload));
    }

    #[test]
    fn http_condition_not_equals() {
        let cond = HttpCondition {
            pointer: "/state".into(),
            operator: HttpConditionOperator::NotEquals(Value::String("FAILED".into())),
        };
        let payload = json!({"state": "RUNNING"});
        assert!(cond.matches(&payload));
    }

    #[test]
    fn http_condition_contains() {
        let cond = HttpCondition {
            pointer: "/message".into(),
            operator: HttpConditionOperator::Contains("ready".into()),
        };
        let payload = json!({"message": "job ready"});
        assert!(cond.matches(&payload));
    }

    #[test]
    fn http_condition_regex() {
        let cond = HttpCondition {
            pointer: "/state".into(),
            operator: HttpConditionOperator::Regex(Regex::new("^SUCC.*").unwrap()),
        };
        let payload = json!({"state": "SUCCESS"});
        assert!(cond.matches(&payload));
    }

    #[test]
    fn http_condition_exists() {
        let cond = HttpCondition {
            pointer: "/data/value".into(),
            operator: HttpConditionOperator::Exists,
        };
        let payload = json!({"data": {"value": 10}});
        assert!(cond.matches(&payload));
    }

    #[test]
    fn http_condition_jsonpath() {
        let condition = HttpCondition {
            pointer: "/".into(),
            operator: HttpConditionOperator::JsonPath(JsonPathCondition {
                compiled: Arc::new(
                    JsonPathCompiled::compile("$.items[?(@.state=='DONE')]").unwrap(),
                ),
                equals: None,
                exists: true,
            }),
        };
        let payload = json!({"items": [{"state": "RUNNING"}, {"state": "DONE"}]});
        assert!(condition.matches(&payload));
    }

    #[test]
    fn http_condition_number_compare() {
        let condition = HttpCondition {
            pointer: "/progress".into(),
            operator: HttpConditionOperator::NumberCompare(NumberComparison::GreaterOrEqual, 0.9),
        };
        let payload = json!({"progress": 0.95});
        assert!(condition.matches(&payload));
    }
}
