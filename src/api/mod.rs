use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stepflow_dsl::WorkflowDSL;

use crate::engine::{RunOutcome, TaskHandler, run_until_pause_or_end};
use crate::store::{Checkpoint, RunStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartObtainResult {
    pub run_id: String,
    pub authorize_url: String,
    pub state: String,
    #[serde(default)]
    pub code_verifier: Option<String>,
}

pub fn start_obtain(
    dsl: &WorkflowDSL,
    handler: &dyn TaskHandler,
    run_store: &impl RunStore,
    mut context: Value,
) -> Result<StartObtainResult> {
    // Run until pause point (oauth2.await_callback)
    let outcome = run_until_pause_or_end(dsl, &dsl.start_at, context.take(), handler, 100)?;
    let pending = match outcome {
        RunOutcome::Pending(p) => p,
        _ => return Err(anyhow!("obtain did not pause at await_callback")),
    };
    // Store checkpoint
    run_store.put(Checkpoint {
        run_id: pending.run_id.clone(),
        paused_state: pending.next_state.clone(),
        context: pending.context.clone(),
        await_meta: pending.await_meta.clone(),
    });

    // Extract authorization parameters from context
    let auth = pending
        .context
        .pointer("/states/Auth/result")
        .ok_or_else(|| anyhow!("states.Auth.result missing"))?;
    let authorize_url = auth
        .get("authorize_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("authorize_url missing"))?
        .to_string();
    let state = auth
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("state missing"))?
        .to_string();
    let code_verifier = auth
        .get("code_verifier")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(StartObtainResult {
        run_id: pending.run_id,
        authorize_url,
        state,
        code_verifier,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeObtainArgs {
    pub run_id: String,
    pub code: String,
    pub state: String,
}

pub fn resume_obtain(
    dsl: &WorkflowDSL,
    handler: &dyn TaskHandler,
    run_store: &impl RunStore,
    args: ResumeObtainArgs,
) -> Result<Value> {
    let cp = run_store
        .get(&args.run_id)
        .ok_or_else(|| anyhow!("run_id not found"))?;
    // Inject code/state into context.input for Await mapping (external mode can use input)
    let mut ctx = cp.context.clone();
    let input = ctx.get_mut("input").and_then(|v| v.as_object_mut());
    if let Some(obj) = input {
        obj.insert("code".into(), Value::String(args.code.clone()));
        obj.insert("state".into(), Value::String(args.state.clone()));
        obj.insert("returned_state".into(), Value::String(args.state.clone()));
    } else {
        ctx.as_object_mut().unwrap().insert(
            "input".into(),
            json!({"code": args.code, "state": args.state, "returned_state": args.state}),
        );
    }

    // Continue execution from next_state until end
    let outcome = run_until_pause_or_end(dsl, &cp.paused_state, ctx, handler, 100)?;
    match outcome {
        RunOutcome::Finished(final_ctx) => {
            // Delete checkpoint after completion
            run_store.del(&args.run_id);
            Ok(final_ctx)
        }
        RunOutcome::Pending(_) => Err(anyhow!("unexpected pending when resuming obtain")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::HttpTaskHandler;
    use crate::actions::{OAuth2AuthorizeRedirectHandler, OAuth2AwaitCallbackHandler};
    use crate::engine::TaskHandler;
    use crate::store::MemoryRunStore;
    use httpmock::prelude::*;

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
    fn start_and_resume_obtain_external_callback() {
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
      auth_state: "{{% result.state %}}"
      auth_code_verifier: "{{% result.code_verifier %}}"
    next: "Await"
  Await:
    type: task
    resource: "oauth2.await_callback"
    parameters:
      state: "{{% input.state %}}"
      expected_state: "{{% $auth_state %}}"
      code: "{{% input.code %}}"
      expected_pkce:
        code_verifier: "{{% $auth_code_verifier %}}"
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
        code: "{{% vars.cb.code %}}"
        code_verifier: "{{% vars.cb.code_verifier %}}"
    output: "{{% result.body.access_token %}}"
    end: true
"#,
            server.base_url(),
            "/token"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        let run_store = MemoryRunStore::default();
        let start = start_obtain(&dsl, &Router, &run_store, json!({})).unwrap();
        assert!(start.authorize_url.contains("response_type=code"));
        // simulate UI callback
        let final_ctx = resume_obtain(
            &dsl,
            &Router,
            &run_store,
            ResumeObtainArgs {
                run_id: start.run_id,
                code: "thecode".into(),
                state: start.state,
            },
        )
        .unwrap();
        m_token.assert();
        assert_eq!(
            final_ctx.pointer("/states/Exchange/result").unwrap(),
            &json!("tok")
        );
    }
}
