#[cfg(feature = "server")]
use crate::engine::{run_until_pause_or_end, RunOutcome};
#[cfg(feature = "server")]
use crate::server::state::{ExecutionEvent, ExecutionStatus, ServerState};
#[cfg(feature = "server")]
use serde_json::json;
#[cfg(feature = "server")]
use std::time::SystemTime;

#[cfg(feature = "server")]
pub async fn execute_workflow(state: ServerState, execution_id: String) {
    let (workflow, flow_name, input, context) = {
        let executions = state.executions.read().unwrap();
        let execution = match executions.get(&execution_id) {
            Some(e) => e,
            None => return,
        };
        let workflows = state.workflows.read().unwrap();
        let workflow = match workflows.get(&execution.workflow_id) {
            Some(w) => w.clone(),
            None => return,
        };
        (workflow, execution.flow.clone(), execution.input.clone(), execution.context.clone())
    };

    let mut exec_context = serde_json::Map::new();
    exec_context.insert("input".to_string(), input);
    exec_context.insert("provider".to_string(), json!({ "config": workflow.dsl.provider.config }));
    if let Some(ctx) = context {
        if let serde_json::Value::Object(ctx_map) = ctx {
            for (k, v) in ctx_map {
                exec_context.insert(k, v);
            }
        } else {
            exec_context.insert("context".to_string(), ctx);
        }
    }
    let exec_context = serde_json::Value::Object(exec_context);

    let flow = match workflow.dsl.provider.flows.get(&flow_name) {
        Some(f) => f,
        None => {
            let mut executions = state.executions.write().unwrap();
            if let Some(execution) = executions.get_mut(&execution_id) {
                execution.status = ExecutionStatus::Failed;
                execution.error = Some(format!("Flow '{}' not found", flow_name));
                execution.updated_at = SystemTime::now();
                execution.completed_at = Some(SystemTime::now());
            }
            return;
        }
    };

    let start_state = {
        let executions = state.executions.read().unwrap();
        if let Some(execution) = executions.get(&execution_id) {
            execution.current_state.as_deref().unwrap_or(&flow.start_at).to_string()
        } else {
            flow.start_at.clone()
        }
    };

    let result =
        run_until_pause_or_end(flow, &start_state, exec_context, state.task_handler.as_ref(), 100);

    let mut executions = state.executions.write().unwrap();
    if let Some(execution) = executions.get_mut(&execution_id) {
        let now = SystemTime::now();
        execution.updated_at = now;
        match result {
            Ok(RunOutcome::Finished(final_context)) => {
                execution.status = ExecutionStatus::Completed;
                execution.completed_at = Some(now);
                execution.context = Some(final_context.clone());
                println!("[server] execution {} finished", execution_id);
                state.broadcast_event(ExecutionEvent {
                    event_type: "execution_completed".to_string(),
                    execution_id: execution_id.clone(),
                    timestamp: now,
                    data: json!({ "status": "completed" }),
                });
            }
            Ok(RunOutcome::Pending(pending)) => {
                execution.status = ExecutionStatus::Paused;
                // Save pending info to context for later retrieval
                execution.context = Some(pending.context.clone());
                // Set current_state to the paused state's next_state so we resume correctly
                execution.current_state = Some(pending.next_state.clone());
                println!("[server] execution {} paused at {}", execution_id, pending.next_state);
                state.broadcast_event(ExecutionEvent {
                    event_type: "execution_paused".to_string(),
                    execution_id: execution_id.clone(),
                    timestamp: now,
                    data: json!({ "status": "paused", "pending_info": pending }),
                });
            }
            Err(e) => {
                execution.status = ExecutionStatus::Failed;
                execution.error = Some(e.to_string());
                execution.completed_at = Some(now);
                println!("[server] execution {} failed: {}", execution_id, e);
                state.broadcast_event(ExecutionEvent {
                    event_type: "execution_failed".to_string(),
                    execution_id: execution_id.clone(),
                    timestamp: now,
                    data: json!({ "status": "failed", "error": format!("{}", e) }),
                });
            }
        }
    }
}
