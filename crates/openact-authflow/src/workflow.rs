use anyhow::{anyhow, Result};
// serde derive not needed after removing legacy wrappers
use serde_json::{json, Value};
use stepflow_dsl::WorkflowDSL;

use crate::engine::{run_until_pause_or_end, PendingInfo, RunOutcome, TaskHandler};
use openact_core::{store::RunStore, Checkpoint};

/// Start a workflow until the first pause (generic, no DSL assumptions).
/// Stores a checkpoint and returns the pending info.
pub async fn start_until_pause(
    dsl: &WorkflowDSL,
    handler: &dyn TaskHandler,
    run_store: &dyn RunStore,
    mut context: Value,
) -> Result<PendingInfo> {
    // Run until pause point (oauth2.await_callback)
    let outcome = run_until_pause_or_end(dsl, &dsl.start_at, context.take(), handler, 100, None)?;
    let pending = match outcome {
        RunOutcome::Pending(p) => p,
        _ => return Err(anyhow!("workflow did not pause")),
    };
    // Store checkpoint
    run_store
        .put(Checkpoint {
            run_id: pending.run_id.clone(),
            paused_state: pending.next_state.clone(),
            context_json: pending.context.clone(),
            await_meta_json: Some(pending.await_meta.clone()),
        })
        .await?;
    Ok(pending)
}

/// Resume a workflow from a stored pause, applying an input patch under `input`.
/// Returns the final context or a pending outcome depending on the flow.
pub async fn resume_from_pause(
    dsl: &WorkflowDSL,
    handler: &dyn TaskHandler,
    run_store: &dyn RunStore,
    run_id: &str,
    input_patch: Value,
) -> Result<RunOutcome> {
    let cp = run_store.get(run_id).await?.ok_or_else(|| anyhow!("run_id not found"))?;
    // Merge input_patch into context.input (object-merge)
    let mut ctx = cp.context_json.clone();
    match (ctx.get_mut("input"), input_patch) {
        (Some(Value::Object(existing)), Value::Object(patch)) => {
            for (k, v) in patch.into_iter() {
                existing.insert(k, v);
            }
        }
        (_, Value::Object(patch)) => {
            ctx.as_object_mut().unwrap().insert("input".into(), Value::Object(patch));
        }
        // Non-object patch: set as a single field under `input.value`
        (Some(Value::Object(existing)), other) => {
            existing.insert("value".into(), other);
        }
        (_, other) => {
            ctx.as_object_mut().unwrap().insert("input".into(), json!({"value": other}));
        }
    }

    // Continue execution from next_state until end
    let outcome =
        run_until_pause_or_end(dsl, &cp.paused_state, ctx, handler, 100, Some(&cp.run_id))?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::HttpTaskHandler;
    use crate::actions::{OAuth2AuthorizeRedirectHandler, OAuth2AwaitCallbackHandler};
    use crate::engine::TaskHandler;
    use httpmock::prelude::*;
    use openact_store::memory::MemoryRunStore;

    struct Router;
    impl TaskHandler for Router {
        fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> anyhow::Result<Value> {
            match resource {
                "oauth2.authorize_redirect" => {
                    OAuth2AuthorizeRedirectHandler.execute(resource, state_name, ctx)
                }
                "oauth2.await_callback" => {
                    OAuth2AwaitCallbackHandler.execute(resource, state_name, ctx)
                }
                "http.request" => HttpTaskHandler.execute(resource, state_name, ctx),
                _ => anyhow::bail!("unknown resource {resource}"),
            }
        }
    }

    #[test]
    fn start_and_resume_external_pause_resume() {
        let server = MockServer::start();
        let m_token = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"access_token":"tok","token_type":"bearer","expires_in":3600}));
        });

        let yaml = format!(
            r#"
comment: "auth code external"
startAt: "Auth"
states:
  Auth:
    type: task
    resource: "oauth2.authorize_redirect"
    parameters:
      authorizeUrl: "https://auth.example.com/oauth/authorize"
      clientId: "cid"
      redirectUri: "https://app/cb"
      scope: "read"
      usePKCE: true
    assign:
      auth:
        authorize_url: "{{% result.authorize_url %}}"
        state: "{{% result.state %}}"
        code_verifier: "{{% result.code_verifier %}}"
    next: "Await"
  Await:
    type: task
    resource: "oauth2.await_callback"
    parameters:
      state: "{{% input.state %}}"
      expected_state: "{{% $auth.state %}}"
      code: "{{% input.code %}}"
      expected_pkce:
        code_verifier: "{{% $auth.code_verifier %}}"
    next: "Exchange"
  Exchange:
    type: task
    resource: "http.request"
    parameters:
      method: "POST"
      url: "{}{}"
      headers:
        Content-Type: "application/x-www-form-urlencoded"
      body:
        grant_type: "authorization_code"
        client_id: "cid"
        redirect_uri: "https://app/cb"
        code: "{{% $auth.code %}}"
        code_verifier: "{{% $auth.code_verifier %}}"
    output: "{{% result.body.access_token %}}"
    end: true
"#,
            server.base_url(),
            "/token"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        let run_store = MemoryRunStore::default();
        // Start until pause
        let pending =
            futures::executor::block_on(start_until_pause(&dsl, &Router, &run_store, json!({})))
                .unwrap();
        // Simulate callback: read state from vars.auth in pending context and apply input patch
        let state = pending.context.pointer("/vars/auth/state").and_then(|v| v.as_str()).unwrap();
        let outcome = futures::executor::block_on(resume_from_pause(
            &dsl,
            &Router,
            &run_store,
            &pending.run_id,
            json!({"code": "thecode", "state": state}),
        ))
        .unwrap();
        match outcome {
            RunOutcome::Finished(final_ctx) => {
                m_token.assert();
                assert_eq!(final_ctx.pointer("/states/Exchange/result").unwrap(), &json!("tok"));
            }
            RunOutcome::Pending(_) => panic!("unexpected pending after resume"),
        }
    }
}
