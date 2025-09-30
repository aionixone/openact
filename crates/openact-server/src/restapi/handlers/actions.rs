//! Actions API handlers

use crate::{
    dto::{
        ActionSchemaResponse, ActionSummary, Example, ExecuteRequest, ExecuteResponse, ListQuery,
        ResponseEnvelope, ResponseMeta,
    },
    error::ServerError,
    middleware::{request_id::RequestId, tenant::Tenant},
    AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use openact_core::store::{ActionRepository, ConnectionStore};
use openact_core::types::Trn;
use openact_core::ConnectorKind;
use openact_mcp::GovernanceConfig;
use openact_registry::{ExecutionContext, RegistryError};
use serde_json::{json, Value};
use std::convert::TryFrom;
use std::time::Duration;
use tokio::time::timeout;

/// GET /api/v1/actions
pub async fn get_actions(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Extension(tenant): Extension<Tenant>,
    Query(query): Query<ListQuery>,
) -> Result<
    Json<ResponseEnvelope<Value>>,
    (axum::http::StatusCode, Json<crate::error::ErrorResponse>),
> {
    let req_id = request_id.0.clone();

    // Gather action records according to filters
    let mut records = Vec::new();

    if let Some(conn_str) = &query.connection {
        // Filter by specific connection TRN
        if !conn_str.starts_with("trn:openact:") {
            let err = ServerError::InvalidInput("connection must be a TRN".to_string());
            return Err(err.to_http_response(req_id));
        }
        let conn_trn = Trn::new(conn_str.clone());
        records = app_state
            .store
            .as_ref()
            .list_by_connection(&conn_trn)
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))
            .map_err(|e| e.to_http_response(request_id.0.clone()))?;
    } else if let Some(kind) = &query.kind {
        // Filter by connector kind
        records = ActionRepository::list_by_connector(
            app_state.store.as_ref(),
            &ConnectorKind::new(kind.clone()),
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))
        .map_err(|e| e.to_http_response(request_id.0.clone()))?;
    } else {
        // List all: iterate connector kinds from connections (best-effort)
        let kinds = app_state
            .store
            .as_ref()
            .list_distinct_connectors()
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))
            .map_err(|e| e.to_http_response(request_id.0.clone()))?;
        for k in kinds {
            let mut v = ActionRepository::list_by_connector(app_state.store.as_ref(), &k)
                .await
                .map_err(|e| ServerError::Internal(e.to_string()))
                .map_err(|e| e.to_http_response(request_id.0.clone()))?;
            records.append(&mut v);
        }
    }

    // Tenant-scope filtering
    records
        .retain(|r| parse_action_trn(&r.trn).map(|c| c.tenant == tenant.as_str()).unwrap_or(false));

    // Text query filter
    if let Some(q) = &query.q {
        let ql = q.to_lowercase();
        records.retain(|r| {
            r.name.to_lowercase().contains(&ql) || r.trn.as_str().to_lowercase().contains(&ql)
        });
    }

    // Governance filter (tool allow/deny)
    records.retain(|r| {
        let tool_name = format!("{}.{}", r.connector.as_str(), r.name);
        governance.is_tool_allowed(&tool_name)
    });

    // Sort by TRN/name to have stable ordering
    records.sort_by(|a, b| a.trn.as_str().cmp(b.trn.as_str()));

    // Pagination
    let total = records.len() as u64;
    let page = query.page.max(1);
    let page_size = query.page_size.max(1);
    let start = ((page - 1) as usize) * (page_size as usize);
    let end = (start + page_size as usize).min(records.len());
    let page_slice = if start < records.len() { &records[start..end] } else { &[] };

    // Map to summaries
    let actions: Vec<ActionSummary> = page_slice
        .iter()
        .map(|r| {
            let digest = r.config_json.get("input_schema").and_then(|schema| {
                serde_json::to_vec(schema).ok().map(|bytes| {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(bytes);
                    let out = hasher.finalize();
                    format!("sha256:{:x}", out)
                })
            });

            ActionSummary {
                name: r.name.clone(),
                connector: r.connector.as_str().to_string(),
                connection: r.connection_trn.as_str().to_string(),
                description: r.mcp_overrides.as_ref().and_then(|m| m.description.clone()),
                action_trn: r.trn.as_str().to_string(),
                mcp_enabled: r.mcp_enabled,
                input_schema_digest: digest,
            }
        })
        .collect();

    let response = ResponseEnvelope {
        success: true,
        data: json!({
            "actions": actions,
            "page": page,
            "page_size": page_size,
            "total": total
        }),
        metadata: ResponseMeta {
            request_id: request_id.0,
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Ok(Json(response))
}

/// POST /api/v1/actions/{action}/execute/stream (SSE)
pub async fn execute_action_stream(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Path(action): Path<String>,
    Extension(tenant): Extension<Tenant>,
    Query(query): Query<std::collections::HashMap<String, String>>, // version & options
    Json(req): Json<ExecuteRequest>,
) -> Result<axum::response::sse::Sse<impl futures_util::stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, (axum::http::StatusCode, Json<crate::error::ErrorResponse>)>
{
    use axum::response::sse::{Event, KeepAlive, Sse};
    let req_id = request_id.0.clone();
    let tool_name = normalize_action_to_tool_name(&action);
    if !governance.is_tool_allowed(&tool_name) {
        let err = ServerError::Forbidden(format!("Action not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id));
    }

    // Concurrency gate
    let _permit = governance
        .concurrency_limiter
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| ServerError::Internal(format!("Failed to acquire permit: {}", e)))
        .map_err(|e| e.to_http_response(req_id.clone()))?;

    // Resolve TRN (by TRN or name+version)
    let action_trn = if action.starts_with("trn:openact:") {
        Trn::new(action.clone())
    } else {
        let parsed = openact_core::policy::tools::parse_tool_name(&tool_name)
            .map_err(|msg| ServerError::InvalidInput(msg))
            .map_err(|e| e.to_http_response(req_id.clone()))?;
        let version_sel = match query.get("version").map(|s| s.as_str()) {
            None => {
                let err = ServerError::InvalidInput(
                    openact_core::policy::messages::version_required_message().to_string(),
                );
                return Err(err.to_http_response(req_id.clone()));
            }
            Some("latest") | Some("") => None,
            Some(vs) => vs.parse::<i64>().ok(),
        };
        let kind = ConnectorKind::new(&parsed.connector).canonical();
        openact_core::resolve::resolve_action_trn_by_name(
            app_state.store.as_ref(),
            tenant.as_str(),
            &kind,
            &parsed.action,
            version_sel,
        )
        .await
        .map_err(|e| match e {
            openact_core::CoreError::NotFound(msg) => ServerError::NotFound(msg),
            openact_core::CoreError::Invalid(msg) => ServerError::InvalidInput(msg),
            other => ServerError::Internal(other.to_string()),
        })
        .map_err(|e| e.to_http_response(req_id.clone()))?
    };

    let input = req.input.clone();
    let registry = app_state.registry.clone();
    let timeout_dur = governance.timeout;

    let stream = async_stream::stream! {
        let assembler = openact_core::stream::StreamAssembler::new();
        let fut = async {
            let ctx = openact_registry::ExecutionContext::new();
            registry.execute(&action_trn, input, Some(ctx)).await
        };
        let result = tokio::time::timeout(timeout_dur, fut).await;
        match result {
            Ok(Ok(exec)) => {
                let (text, usage, elapsed) = assembler.finish();
                let final_obj = json!({
                    "event": "done",
                    "result": exec.output,
                    "text": text,
                    "usage": {"prompt_tokens": usage.prompt_tokens, "completion_tokens": usage.completion_tokens, "total": usage.total()},
                    "elapsed_ms": elapsed.as_millis(),
                    "action_trn": action_trn.as_str(),
                });
                let _ = yield Ok(Event::default().event("message").data(final_obj.to_string()));
            }
            Ok(Err(e)) => {
                let err_obj = json!({"event": "error", "message": e.to_string()});
                let _ = yield Ok(Event::default().event("error").data(err_obj.to_string()));
            }
            Err(_) => {
                let err_obj = json!({"event": "error", "message": "timeout"});
                let _ = yield Ok(Event::default().event("error").data(err_obj.to_string()));
            }
        }
    };

    let sse = Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10)).text("keep-alive"));
    Ok(sse)
}

/// GET /api/v1/actions/{action}/schema
pub async fn get_action_schema(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Path(action): Path<String>,
    Extension(tenant): Extension<Tenant>,
) -> Result<
    Json<ResponseEnvelope<ActionSchemaResponse>>,
    (axum::http::StatusCode, Json<crate::error::ErrorResponse>),
> {
    let req_id = request_id.0.clone();
    // Governance allow/deny check
    let tool_name = normalize_action_to_tool_name(&action);
    if !governance.is_tool_allowed(&tool_name) {
        tracing::warn!(
            request_id = %req_id,
            tenant = %tenant.as_str(),
            tool = %tool_name,
            "governance denied action schema"
        );
        let err = ServerError::Forbidden(format!("Action not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id));
    }

    // Resolve action to TRN
    let action_trn = if action.starts_with("trn:openact:") {
        let action_trn = Trn::new(action.clone());
        let parsed = parse_action_trn(&action_trn)
            .ok_or_else(|| {
                tracing::warn!(
                    request_id = %request_id.0,
                    tenant = %tenant.as_str(),
                    action = %action_trn.as_str(),
                    "invalid action TRN supplied"
                );
                ServerError::InvalidInput("Invalid action TRN".to_string())
            })
            .map_err(|e| e.to_http_response(request_id.0.clone()))?;
        if parsed.tenant != tenant.as_str() {
            tracing::warn!(
                request_id = %request_id.0,
                expected_tenant = %tenant.as_str(),
                action = %action_trn.as_str(),
                "tenant mismatch for action schema lookup"
            );
            let err = ServerError::NotFound("Action not found".to_string());
            return Err(err.to_http_response(request_id.0.clone()));
        }
        action_trn
    } else {
        let tool = tool_name;
        let mut parts = tool.splitn(2, '.');
        let connector = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if connector.is_empty() || name.is_empty() {
            let err = ServerError::InvalidInput("Invalid action format".to_string());
            return Err(err.to_http_response(request_id.0));
        }
        // Find latest version for tenant
        let mut records = ActionRepository::list_by_connector(
            app_state.store.as_ref(),
            &ConnectorKind::new(connector),
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))
        .map_err(|e| e.to_http_response(request_id.0.clone()))?;
        records.retain(|r| r.name == name);
        records.retain(|r| {
            parse_action_trn(&r.trn).map(|c| c.tenant == tenant.as_str()).unwrap_or(false)
        });
        if records.is_empty() {
            let err = ServerError::NotFound(format!(
                "Action not found: {}.{} (tenant: {})",
                connector,
                name,
                tenant.as_str()
            ));
            return Err(err.to_http_response(request_id.0));
        }
        records.sort_by_key(|r| r.version);
        records.last().unwrap().trn.clone()
    };

    // Load action record
    let action_record = ActionRepository::get(app_state.store.as_ref(), &action_trn)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))
        .map_err(|e| e.to_http_response(request_id.0.clone()))?
        .ok_or_else(|| ServerError::NotFound(format!("Action not found: {}", action_trn)))
        .map_err(|e| e.to_http_response(request_id.0.clone()))?;

    // Prefer explicit schema in config, else translate parameters, else fallback
    let schema_response = if let Some(explicit) = schema_from_config(&action_record) {
        explicit
    } else {
        ActionSchemaResponse {
            input_schema: json!({ "type": "object", "additionalProperties": true }),
            output_schema: json!({ "type": "object", "additionalProperties": true }),
            examples: vec![Example { name: "default".to_string(), input: json!({}) }],
        }
    };

    // Compute digest for metadata
    let schema_digest = {
        use sha2::{Digest, Sha256};
        if let Ok(bytes) = serde_json::to_vec(&schema_response.input_schema) {
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let out = hasher.finalize();
            Some(format!("input_schema_digest=sha256:{:x}", out))
        } else {
            None
        }
    };

    let response = ResponseEnvelope {
        success: true,
        data: schema_response,
        metadata: ResponseMeta {
            request_id: request_id.0,
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: schema_digest.map(|d| vec![d]),
        },
    };

    Ok(Json(response))
}

fn schema_from_config(
    action_record: &openact_core::types::ActionRecord,
) -> Option<ActionSchemaResponse> {
    let cfg = &action_record.config_json;
    let obj = cfg.as_object()?;
    // 1) Prefer explicit input_schema or schema
    if let Some(schema) = obj.get("input_schema").or_else(|| obj.get("schema")) {
        if schema.is_object() {
            return Some(ActionSchemaResponse {
                input_schema: schema.clone(),
                output_schema: json!({ "type": "object", "additionalProperties": true }),
                examples: vec![],
            });
        }
    }
    // 2) Translate parameters -> schema
    if let Some(params) = obj.get("parameters").and_then(|v| v.as_array()) {
        use serde_json::Map;
        let mut properties = Map::new();
        let mut required: Vec<String> = Vec::new();
        for p in params {
            if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                let typ = p.get("type").and_then(|v| v.as_str()).unwrap_or("string");
                let desc = p.get("description").and_then(|v| v.as_str());
                let req = p.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                let mut prop = serde_json::json!({ "type": typ });
                if let Some(d) = desc {
                    prop["description"] = serde_json::Value::String(d.to_string());
                }
                properties.insert(name.to_string(), prop);
                if req {
                    required.push(name.to_string());
                }
            }
        }
        let mut out = serde_json::json!({ "type": "object", "properties": properties });
        if !required.is_empty() {
            out["required"] = serde_json::Value::Array(
                required.into_iter().map(serde_json::Value::String).collect(),
            );
        }
        return Some(ActionSchemaResponse {
            input_schema: out,
            output_schema: json!({ "type": "object", "additionalProperties": true }),
            examples: vec![],
        });
    }
    None
}

/// POST /api/v1/actions/{action}/execute
pub async fn execute_action(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Path(action): Path<String>,
    Extension(tenant): Extension<Tenant>,
    Query(query): Query<std::collections::HashMap<String, String>>, // for validate flag
    Json(req): Json<ExecuteRequest>,
) -> Result<
    Json<ResponseEnvelope<ExecuteResponse>>,
    (axum::http::StatusCode, Json<crate::error::ErrorResponse>),
> {
    let req_id = request_id.0.clone();
    // Governance: allow/deny
    let tool_name = normalize_action_to_tool_name(&action);
    if !governance.is_tool_allowed(&tool_name) {
        tracing::warn!(
            request_id = %req_id,
            tenant = %tenant.as_str(),
            tool = %tool_name,
            "governance denied action execute"
        );
        let err = ServerError::Forbidden(format!("Action not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id.clone()));
    }

    // Concurrency limit
    let _permit = governance
        .concurrency_limiter
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| ServerError::Internal(format!("Failed to acquire permit: {}", e)))
        .map_err(|e| e.to_http_response(req_id.clone()))?;

    // Resolve action to TRN if not given in TRN form
    let action_trn = if action.starts_with("trn:openact:") {
        let action_trn = Trn::new(action.clone());
        let parsed = parse_action_trn(&action_trn)
            .ok_or_else(|| {
                tracing::warn!(
                    request_id = %req_id,
                    tenant = %tenant.as_str(),
                    action = %action_trn.as_str(),
                    "invalid action TRN supplied"
                );
                ServerError::InvalidInput("Invalid action TRN".to_string())
            })
            .map_err(|e| e.to_http_response(req_id.clone()))?;
        if parsed.tenant != tenant.as_str() {
            tracing::warn!(
                request_id = %req_id,
                expected_tenant = %tenant.as_str(),
                action = %action_trn.as_str(),
                "tenant mismatch for action execution"
            );
            let err = ServerError::NotFound("Action not found".to_string());
            return Err(err.to_http_response(req_id.clone()));
        }
        action_trn
    } else {
        // Expect formats like "connector.action" or "connector/action"
        let parsed = match openact_core::policy::tools::parse_tool_name(&tool_name) {
            Ok(p) => p,
            Err(msg) => {
                let err = ServerError::InvalidInput(msg);
                return Err(err.to_http_response(req_id.clone()));
            }
        };

        // Require explicit version selection for name-based execution
        let version_sel = match query.get("version").map(|s| s.as_str()) {
            None => {
                let err = ServerError::InvalidInput(
                    openact_core::policy::messages::version_required_message().to_string(),
                );
                return Err(err.to_http_response(req_id.clone()));
            }
            Some("latest") | Some("") => None,
            Some(vs) => match vs.parse::<i64>() {
                Ok(v) => Some(v),
                Err(_) => {
                    let err = ServerError::InvalidInput(
                        "Invalid 'version' query param: expected integer or 'latest'".to_string(),
                    );
                    return Err(err.to_http_response(req_id.clone()));
                }
            },
        };

        let kind = ConnectorKind::new(&parsed.connector).canonical();
        let trn = openact_core::resolve::resolve_action_trn_by_name(
            app_state.store.as_ref(),
            tenant.as_str(),
            &kind,
            &parsed.action,
            version_sel,
        )
        .await
        .map_err(|e| match e {
            openact_core::CoreError::NotFound(msg) => ServerError::NotFound(msg),
            openact_core::CoreError::Invalid(msg) => ServerError::InvalidInput(msg),
            other => ServerError::Internal(other.to_string()),
        })
        .map_err(|e| e.to_http_response(req_id.clone()))?;
        trn
    };

    let registry = app_state.registry.clone();
    let input = req.input.clone();
    let do_validate = query.get("validate").map(|v| v == "true").unwrap_or(false);
    let options = req.options.as_ref();
    let dry_run = options.and_then(|o| o.dry_run).unwrap_or(false);
    let effective_timeout = options
        .and_then(|o| o.timeout_ms)
        .map(Duration::from_millis)
        .map(
            |requested| {
                if requested < governance.timeout {
                    requested
                } else {
                    governance.timeout
                }
            },
        )
        .unwrap_or(governance.timeout);
    let mut warnings: Option<Vec<String>> = None;
    if dry_run {
        warnings = Some(vec!["dry_run=true".to_string()]);
    }

    // Optional runtime validation against stored input_schema
    let action_record_for_validation = if do_validate || dry_run {
        let record = ActionRepository::get(app_state.store.as_ref(), &action_trn)
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))
            .map_err(|e| e.to_http_response(req_id.clone()))?
            .ok_or_else(|| ServerError::NotFound(format!("Action not found: {}", action_trn)))
            .map_err(|e| e.to_http_response(req_id.clone()))?;
        Some(record)
    } else {
        None
    };

    if do_validate {
        if let Some(action_record) = action_record_for_validation.as_ref() {
            if let Some(schema) = action_record.config_json.get("input_schema") {
                if let Ok(compiled) = jsonschema::JSONSchema::compile(schema) {
                    if let Err(errors) = compiled.validate(&input) {
                        let first = errors.into_iter().next();
                        let msg = first
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "Input does not match schema".to_string());
                        let err = ServerError::InvalidInput(msg);
                        return Err(err.to_http_response(req_id.clone()));
                    }
                }
            }
        }
    }

    if dry_run {
        let version_meta = action_record_for_validation
            .as_ref()
            .and_then(|record| u32::try_from(record.version).ok());
        let response = ResponseEnvelope {
            success: true,
            data: ExecuteResponse {
                result: json!({
                    "dry_run": true,
                    "input": input,
                }),
            },
            metadata: ResponseMeta {
                request_id: req_id,
                execution_time_ms: None,
                action_trn: Some(action_trn.as_str().to_string()),
                version: version_meta,
                warnings,
            },
        };

        return Ok(Json(response));
    }

    drop(action_record_for_validation);
    let fut = async move {
        let ctx = openact_registry::ExecutionContext::new();
        let exec =
            registry.execute(&action_trn, input, Some(ctx)).await.map_err(map_registry_error)?;
        Ok::<_, ServerError>(ExecuteResponse { result: exec.output })
    };

    let exec_response = match timeout(effective_timeout, fut).await {
        Ok(res) => res.map_err(|e| e.to_http_response(req_id.clone()))?,
        Err(_) => {
            let err = ServerError::Timeout;
            return Err(err.to_http_response(req_id.clone()));
        }
    };

    let response = ResponseEnvelope {
        success: true,
        data: exec_response,
        metadata: ResponseMeta {
            request_id: req_id,
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Ok(Json(response))
}

/// POST /api/v1/execute (by TRN)
pub async fn execute_by_trn(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Extension(tenant): Extension<Tenant>,
    Query(query): Query<std::collections::HashMap<String, String>>, // validate flag
    Json(req): Json<Value>, // { action_trn, input, options }
) -> Result<
    Json<ResponseEnvelope<ExecuteResponse>>,
    (axum::http::StatusCode, Json<crate::error::ErrorResponse>),
> {
    let req_id = request_id.0.clone();
    // Extract action_trn text for governance tool name check
    let tool_name = req
        .get("action_trn")
        .and_then(|v| v.as_str())
        .map(|s| trn_to_tool_name(s))
        .unwrap_or_else(|| "unknown.unknown".to_string());

    if !governance.is_tool_allowed(&tool_name) {
        tracing::warn!(
            request_id = %req_id,
            tenant = %tenant.as_str(),
            tool = %tool_name,
            "governance denied execute_by_trn"
        );
        let err = ServerError::Forbidden(format!("Action not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id.clone()));
    }

    // Concurrency + timeout around real execution
    let _permit = governance
        .concurrency_limiter
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| ServerError::Internal(format!("Failed to acquire permit: {}", e)))
        .map_err(|e| e.to_http_response(req_id.clone()))?;

    // Parse inputs
    let trn_str = req
        .get("action_trn")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServerError::InvalidInput("Missing field: action_trn".to_string()))
        .map_err(|e| e.to_http_response(req_id.clone()))?;
    let action_trn = Trn::new(trn_str.to_string());
    let parsed = parse_action_trn(&action_trn)
        .ok_or_else(|| {
            tracing::warn!(
                request_id = %req_id,
                tenant = %tenant.as_str(),
                action = %action_trn.as_str(),
                "invalid action TRN supplied"
            );
            ServerError::InvalidInput("Invalid action TRN".to_string())
        })
        .map_err(|e| e.to_http_response(req_id.clone()))?;
    if parsed.tenant != tenant.as_str() {
        tracing::warn!(
            request_id = %req_id,
            expected_tenant = %tenant.as_str(),
            action = %action_trn.as_str(),
            "tenant mismatch for execute_by_trn"
        );
        let err = ServerError::NotFound("Action not found".to_string());
        return Err(err.to_http_response(req_id.clone()));
    }
    let input = req.get("input").cloned().unwrap_or(Value::Null);
    let do_validate = query.get("validate").map(|v| v == "true").unwrap_or(false);
    let options_value = req.get("options");
    let dry_run = options_value
        .and_then(|opts| opts.get("dry_run"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let effective_timeout = options_value
        .and_then(|opts| opts.get("timeout_ms"))
        .and_then(|v| v.as_u64())
        .map(Duration::from_millis)
        .map(
            |requested| {
                if requested < governance.timeout {
                    requested
                } else {
                    governance.timeout
                }
            },
        )
        .unwrap_or(governance.timeout);
    let mut warnings: Option<Vec<String>> = None;
    if dry_run {
        warnings = Some(vec!["dry_run=true".to_string()]);
    }

    let registry = app_state.registry.clone();
    if dry_run || do_validate {
        let action_record = ActionRepository::get(app_state.store.as_ref(), &action_trn)
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))
            .map_err(|e| e.to_http_response(req_id.clone()))?
            .ok_or_else(|| ServerError::NotFound(format!("Action not found: {}", action_trn)))
            .map_err(|e| e.to_http_response(req_id.clone()))?;

        if do_validate {
            if let Some(schema) = action_record.config_json.get("input_schema") {
                if let Ok(compiled) = jsonschema::JSONSchema::compile(schema) {
                    if let Err(errors) = compiled.validate(&input) {
                        let first = errors.into_iter().next();
                        let msg = first
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "Input does not match schema".to_string());
                        let err = ServerError::InvalidInput(msg);
                        return Err(err.to_http_response(req_id.clone()));
                    }
                }
            }
        }

        if dry_run {
            let response = ResponseEnvelope {
                success: true,
                data: ExecuteResponse {
                    result: json!({
                        "dry_run": true,
                        "input": input,
                    }),
                },
                metadata: ResponseMeta {
                    request_id: req_id,
                    execution_time_ms: None,
                    action_trn: Some(action_trn.as_str().to_string()),
                    version: Some(u32::try_from(action_record.version).unwrap_or_default()),
                    warnings,
                },
            };

            return Ok(Json(response));
        }
    }

    let fut = async move {
        let ctx = ExecutionContext::new();
        let exec =
            registry.execute(&action_trn, input, Some(ctx)).await.map_err(map_registry_error)?;
        Ok::<_, ServerError>(ExecuteResponse { result: exec.output })
    };

    let exec_response = match timeout(effective_timeout, fut).await {
        Ok(res) => res.map_err(|e| e.to_http_response(req_id.clone()))?,
        Err(_) => {
            let err = ServerError::Timeout;
            return Err(err.to_http_response(req_id.clone()));
        }
    };

    let response = ResponseEnvelope {
        success: true,
        data: exec_response,
        metadata: ResponseMeta {
            request_id: req_id,
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Ok(Json(response))
}

fn map_registry_error(err: openact_registry::RegistryError) -> ServerError {
    match err {
        RegistryError::ActionNotFound(trn) => {
            ServerError::NotFound(format!("Action not found: {}", trn))
        }
        RegistryError::ConnectionNotFound(trn) => {
            ServerError::NotFound(format!("Connection not found: {}", trn))
        }
        RegistryError::InvalidInput(msg) => ServerError::InvalidInput(msg),
        _ => ServerError::Internal(err.to_string()),
    }
}

/// Normalize action segment to tool name like "connector.action"
fn normalize_action_to_tool_name(action: &str) -> String {
    if action.starts_with("trn:openact:") {
        return trn_to_tool_name(action);
    }
    if action.contains('.') {
        return action.to_string();
    }
    if action.contains('/') {
        return action.replace('/', ".");
    }
    action.to_string()
}

/// Convert TRN to tool name "connector.action"
fn trn_to_tool_name(trn_str: &str) -> String {
    let trn = Trn::new(trn_str.to_string());
    if let Some(comp) = parse_action_trn(&trn) {
        return format!("{}.{}", comp.connector, comp.name);
    }
    "unknown.unknown".to_string()
}

struct ParsedActionTrn<'a> {
    tenant: &'a str,
    connector: &'a str,
    name: &'a str,
    _version: i64,
}

fn parse_action_trn(trn: &Trn) -> Option<ParsedActionTrn<'_>> {
    let mut parts = trn.as_str().splitn(4, ':');
    if parts.next()? != "trn" {
        return None;
    }
    if parts.next()? != "openact" {
        return None;
    }
    let tenant = parts.next()?;
    let resource_part = parts.next()?;
    let (resource, version_part) = resource_part.split_once('@')?;
    let version = version_part.strip_prefix('v')?.parse().ok()?;
    if !resource.starts_with("action/") {
        return None;
    }
    let remainder = &resource["action/".len()..];
    let mut segments = remainder.split('/');
    let connector = segments.next()?;
    let name = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    Some(ParsedActionTrn { tenant, connector, name, _version: version })
}
