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
    records.retain(|r| {
        r.trn
            .parse_action()
            .map(|c| c.tenant == tenant.as_str())
            .unwrap_or(false)
    });

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
    let page_slice = if start < records.len() {
        &records[start..end]
    } else {
        &[]
    };

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
        let err = ServerError::Forbidden(format!("Action not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id));
    }

    // Resolve action to TRN
    let action_trn = if action.starts_with("trn:openact:") {
        Trn::new(action.clone())
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
            r.trn
                .parse_action()
                .map(|c| c.tenant == tenant.as_str())
                .unwrap_or(false)
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
            examples: vec![Example {
                name: "default".to_string(),
                input: json!({}),
            }],
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
                required
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
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
        Trn::new(action.clone())
    } else {
        // Expect formats like "connector.action" or "connector/action"
        let tool = tool_name; // already normalized to connector.action
        let mut parts = tool.splitn(2, '.');
        let connector = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if connector.is_empty() || name.is_empty() {
            let err = ServerError::InvalidInput("Invalid action format".to_string());
            return Err(err.to_http_response(req_id.clone()));
        }

        // List actions by connector and pick latest version for current tenant and name
        let records = ActionRepository::list_by_connector(
            app_state.store.as_ref(),
            &ConnectorKind::new(connector),
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))
        .map_err(|e| e.to_http_response(req_id.clone()))?;

        let mut candidates: Vec<_> = records
            .into_iter()
            .filter(|r| r.name == name)
            .filter(|r| {
                r.trn
                    .parse_action()
                    .map(|c| c.tenant == tenant.as_str())
                    .unwrap_or(false)
            })
            .collect();

        if candidates.is_empty() {
            let err = ServerError::NotFound(format!(
                "Action not found: {}.{} (tenant: {})",
                connector,
                name,
                tenant.as_str()
            ));
            return Err(err.to_http_response(req_id.clone()));
        }

        candidates.sort_by_key(|r| r.version);
        candidates.last().unwrap().trn.clone()
    };

    let registry = app_state.registry.clone();
    let input = req.input.clone();
    let do_validate = query.get("validate").map(|v| v == "true").unwrap_or(false);

    // Optional runtime validation against stored input_schema
    if do_validate {
        if let Some(action_record) = ActionRepository::get(app_state.store.as_ref(), &action_trn)
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))
            .map_err(|e| e.to_http_response(req_id.clone()))?
        {
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
    let fut = async move {
        let ctx = openact_registry::ExecutionContext::new();
        let exec = registry
            .execute(&action_trn, input, Some(ctx))
            .await
            .map_err(map_registry_error)?;
        Ok::<_, ServerError>(ExecuteResponse {
            result: exec.output,
        })
    };

    let exec_response = match timeout(governance.timeout, fut).await {
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
    let input = req.get("input").cloned().unwrap_or(Value::Null);
    let do_validate = query.get("validate").map(|v| v == "true").unwrap_or(false);

    let registry = app_state.registry.clone();
    let store = app_state.store.clone();
    let fut = async move {
        // Optional runtime validation against stored input_schema
        if do_validate {
            if let Some(action_record) =
                openact_core::store::ActionRepository::get(store.as_ref(), &action_trn)
                    .await
                    .map_err(|e| ServerError::Internal(e.to_string()))?
            {
                if let Some(schema) = action_record.config_json.get("input_schema") {
                    if let Ok(compiled) = jsonschema::JSONSchema::compile(schema) {
                        if let Err(errors) = compiled.validate(&input) {
                            let first = errors.into_iter().next();
                            let msg = first
                                .map(|e| e.to_string())
                                .unwrap_or_else(|| "Input does not match schema".to_string());
                            return Err(ServerError::InvalidInput(msg));
                        }
                    }
                }
            }
        }

        let ctx = ExecutionContext::new();
        let exec = registry
            .execute(&action_trn, input, Some(ctx))
            .await
            .map_err(map_registry_error)?;
        Ok::<_, ServerError>(ExecuteResponse {
            result: exec.output,
        })
    };

    let exec_response = match timeout(governance.timeout, fut).await {
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
    if let Some(comp) = trn.parse_action() {
        return format!("{}.{}", comp.connector, comp.name);
    }
    "unknown.unknown".to_string()
}
