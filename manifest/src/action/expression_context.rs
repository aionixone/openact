use serde_json::json;

use super::auth::AuthContext;
use super::expression_engine::ExpressionContext;
use super::models::{Action, ActionExecutionContext};

pub fn build_expression_context(
    auth: &AuthContext,
    action: &Action,
    exec: &ActionExecutionContext,
) -> ExpressionContext {
    ExpressionContext {
        access_token: Some(auth.access_token.clone()),
        expires_at: auth.expires_at,
        ctx: json!({
            "action": {
                "method": action.method,
                "path": action.path,
                "provider": action.provider,
                "tenant": action.tenant,
            },
            "exec": {
                "action_trn": exec.action_trn,
                "execution_trn": exec.execution_trn,
                "timestamp": exec.timestamp,
            },
            "params": exec.parameters,
        }),
    }
}


