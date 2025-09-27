use anyhow::{Result, anyhow};
use indexmap::IndexMap;
use serde_json::{Value, json};
use stepflow_dsl::command::{Command, step_once};
use stepflow_dsl::jsonada::WorkflowContext;
use stepflow_dsl::{MappingFacade, State, WorkflowDSL};
pub trait TaskHandler: Send + Sync {
    fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingInfo {
    pub run_id: String,
    pub next_state: String,
    pub context: Value,
    pub await_meta: Value,
}

#[derive(Debug, Clone)]
pub enum RunOutcome {
    Finished(Value),
    Pending(PendingInfo),
}

fn write_state_result(context: &mut Value, state_name: &str, result: Value) {
    let states = context
        .as_object_mut()
        .unwrap()
        .entry("states")
        .or_insert(json!({}));
    if !states.is_object() {
        *states = json!({});
    }
    let obj = states.as_object_mut().unwrap();
    let entry = obj.entry(state_name.to_string()).or_insert(json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let st = entry.as_object_mut().unwrap();
    st.insert("result".to_string(), result);
}

pub fn run_flow(
    dsl: &WorkflowDSL,
    start_at: &str,
    mut context: Value,
    handler: &dyn TaskHandler,
    max_steps: usize,
) -> Result<Value> {
    if !context.is_object() {
        context = json!({});
    }
    let mut current = start_at.to_string();
    for _step in 0..max_steps {
        let cmd = step_once(dsl, &current, &context)
            .map_err(|e| anyhow!("plan step failed at {current}: {e}"))?;
        match cmd {
            Command::ExecuteTask {
                state_name,
                resource,
                next_state,
            } => {
                println!(
                    "[engine] executing state={} resource={}",
                    state_name, resource
                );
                // Use stepflow-dsl TaskState fields: parameters / assign / output
                let mapper = MappingFacade::new();
                // Construct mapping context from current engine context
                let input_root = context.get("input").cloned().unwrap_or_else(|| json!({}));
                let global = context.get("global").cloned().unwrap_or_else(|| json!({}));
                let mut vars: IndexMap<String, Value> = IndexMap::new();
                if let Some(v) = context.get("vars").and_then(|v| v.as_object()) {
                    for (k, val) in v.iter() {
                        vars.insert(k.clone(), val.clone());
                    }
                }
                if let Some(conn) = context.get("connection") {
                    vars.insert("connection".into(), conn.clone());
                }
                if let Some(secrets) = context.get("secrets") {
                    vars.insert("secrets".into(), secrets.clone());
                }
                if let Some(provider) = context.get("provider") {
                    // Keep full provider under an explicit namespace; do not implicitly flatten config
                    vars.insert("provider".into(), provider.clone());
                }

                // Read current state definition to get parameters/assign/output
                let state_def = dsl
                    .states
                    .get(&state_name)
                    .ok_or_else(|| anyhow!("state not found: {}", state_name))?;
                let (parameters, assign, output_mapping) = match state_def {
                    State::Task(t) => (t.parameters.clone(), t.assign.clone(), t.output.clone()),
                    _ => (None, None, None),
                };

                // Build context for parameter/assign evaluation
                let ctx = mapper.build_context(
                    input_root.clone(),
                    global.clone(),
                    vars.clone(),
                    WorkflowContext::default(),
                    None,
                    None,
                );

                // parameters
                let mapped_input = mapper.evaluate_arguments(&parameters, &ctx)?;

                // Execute with context merged with mapped parameters (parameters override)
                let exec_in_owned = if let Some(mi) = mapped_input {
                    let mut merged = context.clone();
                    if let (Some(mobj), Some(cobj)) = (mi.as_object(), merged.as_object_mut()) {
                        for (k, v) in mobj.iter() {
                            cobj.insert(k.clone(), v.clone());
                        }
                    }
                    merged
                } else {
                    context.clone()
                };
                println!(
                    "[engine] mapped params keys: {}",
                    exec_in_owned
                        .as_object()
                        .map(|o| o.keys().cloned().collect::<Vec<_>>().join(","))
                        .unwrap_or_else(|| "<non-object>".to_string())
                );
                let raw_out = handler.execute(&resource, &state_name, &exec_in_owned)?;
                println!("[engine] state={} handler returned", state_name);

                // assign → extend vars after execution (can access result)
                if assign.is_some() {
                    let ctx_assign = mapper.build_context(
                        input_root.clone(),
                        global.clone(),
                        vars.clone(),
                        WorkflowContext::default(),
                        Some(raw_out.clone()),
                        None,
                    );
                    if let Some(assigned) = mapper.evaluate_assign(&assign, &ctx_assign)? {
                        for (k, v) in assigned {
                            vars.insert(k, v);
                        }
                    }
                }

                // Note: Do not auto-store entire state output as $StateName
                // Instead, use DSL assign to explicitly extract needed fields to vars

                // Output mapping
                if output_mapping.is_some() {
                    let mut dest = context.clone();
                    let ctx_out = mapper.build_context(
                        input_root,
                        global,
                        vars.clone(),
                        WorkflowContext::default(),
                        Some(raw_out.clone()),
                        None,
                    );
                    // evaluate_output will write to default path states.<StateName>.result
                    let _ = mapper.evaluate_output(
                        &output_mapping,
                        &raw_out,
                        &state_name,
                        &ctx_out,
                        &mut dest,
                    )?;
                    // Persist vars into context
                    {
                        let obj = dest.as_object_mut().unwrap();
                        let mut varmap = serde_json::Map::new();
                        for (k, v) in vars.iter() {
                            varmap.insert(k.clone(), v.clone());
                        }
                        obj.insert("vars".into(), Value::Object(varmap));
                    }
                    context = dest;
                } else {
                    write_state_result(&mut context, &state_name, raw_out);
                    // Persist vars into context
                    {
                        let obj = context.as_object_mut().unwrap();
                        let mut varmap = serde_json::Map::new();
                        for (k, v) in vars.iter() {
                            varmap.insert(k.clone(), v.clone());
                        }
                        obj.insert("vars".into(), Value::Object(varmap));
                    }
                }
                if let Some(next) = next_state {
                    if max_steps == 1 {
                        // Single-step execution mode: return immediately after executing one state
                        return Ok(context);
                    }
                    current = next;
                } else if dsl.is_end_state(&state_name) {
                    return Ok(context);
                } else {
                    return Err(anyhow!("task state {state_name} has no next and not end"));
                }
            }
            Command::Pass {
                state_name,
                output,
                next_state,
            } => {
                // Evaluate assign for Pass state (Config-first pattern)
                let mapper = MappingFacade::new();
                let input_root = context.get("input").cloned().unwrap_or_else(|| json!({}));
                let global = context.get("global").cloned().unwrap_or_else(|| json!({}));
                let mut vars: IndexMap<String, Value> = IndexMap::new();
                if let Some(v) = context.get("vars").and_then(|v| v.as_object()) {
                    for (k, val) in v.iter() {
                        vars.insert(k.clone(), val.clone());
                    }
                }
                if let Some(conn) = context.get("connection") {
                    vars.insert("connection".into(), conn.clone());
                }
                if let Some(secrets) = context.get("secrets") {
                    vars.insert("secrets".into(), secrets.clone());
                }
                if let Some(provider) = context.get("provider") {
                    vars.insert("provider".into(), provider.clone());
                }
                // Lookup Pass state's assign mapping
                if let Some(State::Pass(p)) = dsl.states.get(&state_name) {
                    if p.assign.is_some() {
                        let ctx_assign = mapper.build_context(
                            input_root,
                            global,
                            vars.clone(),
                            WorkflowContext::default(),
                            None,
                            None,
                        );
                        if let Some(assigned) = mapper.evaluate_assign(&p.assign, &ctx_assign)? {
                            for (k, v) in assigned {
                                vars.insert(k, v);
                            }
                            // Persist vars into context
                            let obj = context.as_object_mut().unwrap();
                            let mut varmap = serde_json::Map::new();
                            for (k, v) in vars.iter() {
                                varmap.insert(k.clone(), v.clone());
                            }
                            obj.insert("vars".into(), Value::Object(varmap));
                        }
                    }
                }
                write_state_result(&mut context, &state_name, output);
                if let Some(next) = next_state {
                    if max_steps == 1 {
                        // Single-step execution mode: return immediately after executing one state
                        return Ok(context);
                    }
                    current = next;
                } else if dsl.is_end_state(&state_name) {
                    return Ok(context);
                } else {
                    return Err(anyhow!("pass state {state_name} has no next and not end"));
                }
            }
            Command::Choice {
                state_name: _,
                next_state,
            } => {
                current = next_state;
            }
            Command::Wait {
                state_name: _,
                seconds: _,
                wait_until: _,
                next_state,
            } => {
                // Skip actual sleeping in engine core; upper layer can handle timing
                if let Some(next) = next_state {
                    current = next;
                } else {
                    return Err(anyhow!("wait state missing next"));
                }
            }
            Command::Succeed { state_name, output } => {
                write_state_result(&mut context, &state_name, output);
                return Ok(context);
            }
            Command::Fail {
                state_name,
                error,
                cause,
            } => {
                return Err(anyhow!(
                    "flow failed at {state_name}: {:?} - {:?}",
                    error,
                    cause
                ));
            }
            Command::Map { state_name, .. } | Command::Parallel { state_name, .. } => {
                return Err(anyhow!(
                    "unsupported state type at {state_name}: map/parallel are not supported"
                ));
            }
        }
    }
    Err(anyhow!("exceeded max_steps without termination"))
}

pub fn run_until_pause_or_end(
    dsl: &WorkflowDSL,
    start_at: &str,
    mut context: Value,
    handler: &dyn TaskHandler,
    max_steps: usize,
) -> Result<RunOutcome> {
    if !context.is_object() {
        context = json!({});
    }
    let mut current = start_at.to_string();
    for _ in 0..max_steps {
        let cmd = step_once(dsl, &current, &context)
            .map_err(|e| anyhow!("plan step failed at {current}: {e}"))?;
        match cmd {
            Command::ExecuteTask {
                state_name,
                resource: _,
                next_state,
            } => {
                // Attempt to execute the task, pause if PAUSE_FOR_CALLBACK error is encountered

                match run_flow(dsl, &state_name, context.clone(), handler, 1) {
                    Ok(out) => {
                        context = out;
                        println!(
                            "[engine] state {} executed ok; next={:?}",
                            state_name, next_state
                        );
                        if let Some(next) = next_state {
                            current = next;
                        } else if dsl.is_end_state(&state_name) {
                            return Ok(RunOutcome::Finished(context));
                        } else {
                            return Err(anyhow!("task state {state_name} has no next and not end"));
                        }
                    }
                    Err(e) if e.to_string().contains("PAUSE_FOR_CALLBACK") => {
                        println!("[engine] state {} paused for callback (await)", state_name);
                        let run_id = uuid::Uuid::new_v4().to_string();
                        let await_meta = json!({
                            "expected_state": context.pointer("/vars/auth/state"),
                            "reason": "oauth_callback"
                        });
                        return Ok(RunOutcome::Pending(PendingInfo {
                            run_id,
                            next_state: state_name.clone(),
                            context,
                            await_meta,
                        }));
                    }
                    Err(e) => {
                        println!("[engine] state {} error: {}", state_name, e);
                        return Err(e);
                    }
                }
            }
            _ => {
                // For non-task, delegate to original run_flow one-step by executing from this state
                let out = run_flow(dsl, &current, context.clone(), handler, 1)?;
                context = out;
                if dsl.is_end_state(&current) {
                    return Ok(RunOutcome::Finished(context));
                }
                let (_, base) = dsl.get_state_and_base(&current);
                if let Some(next) = &base.next {
                    current = next.clone();
                } else {
                    return Ok(RunOutcome::Finished(context));
                }
            }
        }
    }
    Err(anyhow!("exceeded max_steps without termination"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_yaml;

    struct NoopTask;
    impl TaskHandler for NoopTask {
        fn execute(&self, _resource: &str, _state_name: &str, _ctx: &Value) -> Result<Value> {
            Ok(json!({"ok": true}))
        }
    }

    #[test]
    fn pass_then_succeed() {
        let yaml = r#"
comment: "Pass to Succeed"
startAt: "P"
states:
  P:
    type: pass
    output: { v: 1 }
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &NoopTask, 10).unwrap();
        assert_eq!(out["states"]["P"]["result"]["v"], 1);
    }

    #[test]
    fn task_end_true() {
        let yaml = r#"
comment: "Task end"
startAt: "T"
states:
  T:
    type: task
    resource: "test.action"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &NoopTask, 10).unwrap();
        assert_eq!(out["states"]["T"]["result"]["ok"], true);
    }

    #[test]
    fn choice_branch() {
        let yaml = r#"
comment: "Choice"
startAt: "C"
states:
  C:
    type: choice
    choices:
      - condition: "input.x > 1"
        next: "A"
    default: "B"
  A: { type: succeed }
  B: { type: fail, error: "bad" }
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({"x": 2});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &NoopTask, 10).unwrap();
        // Succeed writes full context as output; verify termination
        assert!(out["states"]["A"]["result"].is_object());
    }

    #[test]
    fn wait_then_succeed() {
        let yaml = r#"
comment: "Wait then succeed"
startAt: "W"
states:
  W:
    type: wait
    seconds: 0
    next: "S"
  S:
    type: succeed
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &NoopTask, 10).unwrap();
        assert!(out["states"]["S"]["result"].is_object());
    }

    #[test]
    fn fail_terminates() {
        let yaml = r#"
comment: "Fail"
startAt: "F"
states:
  F:
    type: fail
    error: "boom"
    cause: "x"
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({});
        let err = run_flow(&dsl, &dsl.start_at, ctx, &NoopTask, 5)
            .err()
            .unwrap();
        assert!(format!("{}", err).contains("flow failed at F"));
    }

    struct EchoTask;
    impl TaskHandler for EchoTask {
        fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
            Ok(json!({"echo": ctx.clone()}))
        }
    }

    #[test]
    fn mapping_input_and_output_default_result() {
        let yaml = r#"
comment: "Mapping IO"
startAt: "T"
states:
  T:
    type: task
    resource: "echo"
    parameters:
      userId: "{% input.user.id %}"
    output: "{% result.echo.userId %}"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({"input": {"user": {"id": "u1"}}});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &EchoTask, 10).unwrap();
        assert_eq!(out["states"]["T"]["result"], json!("u1"));
    }

    #[test]
    fn mapping_output_with_result_path() {
        let yaml = r#"
comment: "Mapping with resultPath"
startAt: "T"
states:
  T:
    type: task
    resource: "echo"
    parameters:
      v: "{% input.n %}"
    output: "{% result.echo.v %}"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let ctx = json!({"input": {"n": 42}});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &EchoTask, 10).unwrap();
        assert_eq!(out["states"]["T"]["result"], json!(42));
    }

    #[test]
    fn compute_hmac_rfc4231_case2_hex() {
        // RFC 4231 Test Case 2 (SHA256): key = "Jefe", msg = "what do ya want for nothing?"
        let key = "Jefe";
        let yaml = r#"
comment: "HMAC test"
startAt: "H"
states:
  H:
    type: task
    resource: "compute.hmac"
    parameters:
      algorithm: "SHA256"
      key: "{KEY}"
      message: "what do ya want for nothing?"
      encoding: "hex"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml.replace("{KEY}", key)).unwrap();
        let out = run_flow(
            &dsl,
            &dsl.start_at,
            json!({}),
            &crate::authflow::actions::DefaultRouter,
            5,
        )
        .unwrap();
        let got = out["states"]["H"]["result"]["signature"].as_str().unwrap();
        assert_eq!(
            got,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn compute_jwt_hs256_basic() {
        let yaml = r#"
comment: "JWT HS256"
startAt: "J"
states:
  J:
    type: task
    resource: "compute.jwt_sign"
    parameters:
      alg: "HS256"
      key: "secret"
      header: { kid: "k1" }
      claims: { sub: "123", name: "Alice" }
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();
        let out = run_flow(
            &dsl,
            &dsl.start_at,
            json!({}),
            &crate::authflow::actions::DefaultRouter,
            5,
        )
        .unwrap();
        let tok = out["states"]["J"]["result"]["token"].as_str().unwrap();
        assert!(tok.split('.').count() == 3);
    }

    #[test]
    fn secrets_resolve_memory_provider() {
        let yaml = r#"
comment: "Secrets resolve"
startAt: "S"
states:
  S:
    type: task
    resource: "secrets.resolve"
    parameters:
      uri: "vault://kv/api_key"
    output: "{% result.value %}"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();

        // Router wires secrets.resolve to MemorySecretsProvider backed handler
        struct Router;
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "secrets.resolve" => {
                        let provider = crate::authflow::actions::MemorySecretsProvider::from_pairs(
                            vec![("vault://kv/api_key", "XYZ")],
                        );
                        crate::authflow::actions::SecretsResolveHandler::new(provider)
                            .execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }

        let out = run_flow(&dsl, &dsl.start_at, json!({}), &Router, 5).unwrap();
        assert_eq!(out["states"]["S"]["result"], json!("XYZ"));
    }

    #[test]
    fn secrets_resolve_with_json_pointer() {
        let yaml = r#"
comment: "Secrets resolve with pointer"
startAt: "S"
states:
  S:
    type: task
    resource: "secrets.resolve"
    parameters:
      uri: "vault://kv/json#/_/nested/value"
    output: "{% result.value %}"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();

        let json_secret = serde_json::to_string(&json!({"_": {"nested": {"value": 123}}})).unwrap();

        struct Router {
            secret: String,
        }
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "secrets.resolve" => {
                        let provider = crate::authflow::actions::MemorySecretsProvider::from_pairs(
                            vec![("vault://kv/json", self.secret.as_str())],
                        );
                        crate::authflow::actions::SecretsResolveHandler::new(provider)
                            .execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }

        let out = run_flow(
            &dsl,
            &dsl.start_at,
            json!({}),
            &Router {
                secret: json_secret,
            },
            5,
        )
        .unwrap();
        assert_eq!(out["states"]["S"]["result"], json!(123));
    }

    #[test]
    fn secrets_resolve_many_items() {
        let yaml = r#"
comment: "Secrets resolve many"
startAt: "S"
states:
  S:
    type: task
    resource: "secrets.resolve_many"
    parameters:
      items:
        apiKey: "vault://kv/api_key"
        nested: "vault://kv/json#/_/value"
    output: "{% result.values %}"
    end: true
"#;
        let dsl: WorkflowDSL = serde_yaml::from_str(yaml).unwrap();

        let json_secret = serde_json::to_string(&json!({"_": {"value": 321}})).unwrap();

        struct Router {
            s1: String,
            s2: String,
        }
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "secrets.resolve_many" => {
                        let provider =
                            crate::authflow::actions::MemorySecretsProvider::from_pairs(vec![
                                ("vault://kv/api_key", self.s1.as_str()),
                                ("vault://kv/json", self.s2.as_str()),
                            ]);
                        crate::authflow::actions::SecretsResolveManyHandler::new(provider)
                            .execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }

        let out = run_flow(
            &dsl,
            &dsl.start_at,
            json!({}),
            &Router {
                s1: "XYZ".into(),
                s2: json_secret,
            },
            5,
        )
        .unwrap();
        assert_eq!(out["states"]["S"]["result"]["apiKey"], json!("XYZ"));
        assert_eq!(out["states"]["S"]["result"]["nested"], json!(321));
    }

    #[test]
    fn http_request_get_and_map() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(GET)
                .path("/hello")
                .query_param("q", "1")
                .header("X-Api-Key", "k");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"ok": true, "value": 7}));
        });

        let yaml = format!(
            r#"
comment: "HTTP"
startAt: "T"
states:
  T:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers:
        X-Api-Key: "k"
      query:
        q: "1"
    output: "{{% result.body.value %}}"
    end: true
"#,
            server.base_url(),
            "/hello"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        let ctx = json!({});
        use crate::authflow::actions::HttpTaskHandler;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let out = run_flow(&dsl, &dsl.start_at, ctx, &HttpTaskHandler, 10).unwrap();
        m.assert();
        assert_eq!(out["states"]["T"]["result"], json!(7));
    }

    #[test]
    fn oauth2_client_credentials_maps_access_token() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "access_token": "abc123",
                    "token_type": "bearer",
                    "expires_in": 3600
                }));
        });

        let yaml = format!(
            r#"
comment: "OAuth2 CC"
startAt: "T"
states:
  T:
    type: task
    resource: "oauth2.client_credentials"
    parameters:
      tokenUrl: "{}{}"
      clientId: "cid"
      clientSecret: "sec"
    output: "{{% result.access_token %}}"
    end: true
"#,
            server.base_url(),
            "/token"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();

        // route resource names in handler
        struct Router;
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "oauth2.client_credentials" => {
                        crate::authflow::actions::OAuth2ClientCredentialsHandler
                            .execute(resource, state_name, ctx)
                    }
                    _ => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                }
            }
        }

        let ctx = json!({});
        let out = run_flow(&dsl, &dsl.start_at, ctx, &Router, 10).unwrap();
        m.assert();
        assert_eq!(out["states"]["T"]["result"], json!("abc123"));
    }

    #[test]
    fn end_to_end_oauth2_cc_then_http_request() {
        let server = MockServer::start();
        // token endpoint
        let m_token = server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "access_token": "abc123",
                    "token_type": "bearer",
                    "expires_in": 3600
                }));
        });
        // api endpoint requiring bearer
        let m_api = server.mock(|when, then| {
            when.method(GET)
                .path("/v1/me")
                .header("authorization", "Bearer abc123");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"id": "u1", "name": "Alice"}));
        });

        let yaml = format!(
            r#"
comment: "OAuth2 CC -> API"
startAt: "GetToken"
states:
  GetToken:
    type: task
    resource: "oauth2.client_credentials"
    parameters:
      tokenUrl: "{}{}"
      clientId: "cid"
      clientSecret: "sec"
    assign:
      access_token: "{{% result.access_token %}}"
    next: "CallAPI"

  CallAPI:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers:
        Authorization: "{{% 'Bearer ' & $access_token %}}"
    output: "{{% result.body.id %}}"
    end: true
"#,
            server.base_url(),
            "/oauth/token",
            server.base_url(),
            "/v1/me"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();

        struct Router;
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "oauth2.client_credentials" => {
                        crate::authflow::actions::OAuth2ClientCredentialsHandler
                            .execute(resource, state_name, ctx)
                    }
                    "http.request" => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }

        let ctx = json!({});
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let out = run_flow(&dsl, &dsl.start_at, ctx, &Router, 20).unwrap();
        m_token.assert();
        m_api.assert();
        assert_eq!(out["states"]["CallAPI"]["result"], json!("u1"));
    }

    #[test]
    fn notion_get_page_via_http_request_with_mock() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(GET)
                .path("/v1/pages/pg_123")
                .header("authorization", "Bearer test_token")
                .header("notion-version", "2022-06-28");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "object": "page",
                    "id": "pg_123",
                    "archived": false
                }));
        });

        let base = server.base_url();
        let yaml = format!(
            r#"
comment: "Notion get page"
startAt: "GetPage"
states:
  GetPage:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers:
        Authorization: "{{% 'Bearer ' & input.token %}}"
        Notion-Version: "{{% input.notionVersion ? input.notionVersion : '2022-06-28' %}}"
    output: "{{% result.body.id %}}"
    end: true
"#,
            base, "/v1/pages/pg_123"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        let ctx = json!({ "input": { "token": "test_token" } });
        use crate::authflow::actions::HttpTaskHandler;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let out = run_flow(&dsl, &dsl.start_at, ctx, &HttpTaskHandler, 10).unwrap();
        m.assert();
        assert_eq!(out["states"]["GetPage"]["result"], json!("pg_123"));
    }

    #[test]
    fn inject_bearer_and_call_api() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(GET)
                .path("/whoami")
                .header("authorization", "Bearer tok");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"ok": true}));
        });

        let yaml = format!(
            r#"
comment: "inject bearer then http"
startAt: "Inject"
states:
  Inject:
    type: task
    resource: "inject.bearer"
    parameters:
      token: "tok"
    assign:
      headers: "{{% result.headers %}}"
    next: "Call"
  Call:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers: "{{% $headers %}}"
    output: "{{% result.status %}}"
    end: true
"#,
            server.base_url(),
            "/whoami"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        struct Router;
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "inject.bearer" => crate::authflow::actions::InjectBearerHandler
                        .execute(resource, state_name, ctx),
                    "http.request" => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let out = run_flow(&dsl, &dsl.start_at, json!({}), &Router, 10).unwrap();
        m.assert();
        assert_eq!(out["states"]["Call"]["result"], json!(200));
    }

    #[test]
    fn inject_api_key_query_and_call_api() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(GET).path("/ping").query_param("api_key", "XYZ");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"ok": true}));
        });

        let yaml = format!(
            r#"
comment: "inject api key query then http"
startAt: "Inject"
states:
  Inject:
    type: task
    resource: "inject.api_key"
    parameters:
      key: "XYZ"
      location: "query"
      name: "api_key"
    assign:
      query: "{{% result.query %}}"
    next: "Call"
  Call:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      query: "{{% $query %}}"
    output: "{{% result.status %}}"
    end: true
"#,
            server.base_url(),
            "/ping"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        struct Router;
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "inject.api_key" => crate::authflow::actions::InjectApiKeyHandler
                        .execute(resource, state_name, ctx),
                    "http.request" => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let out = run_flow(&dsl, &dsl.start_at, json!({}), &Router, 10).unwrap();
        m.assert();
        assert_eq!(out["states"]["Call"]["result"], json!(200));
    }

    #[test]
    fn connection_read_inject_and_refresh_update_flow() {
        use crate::store::{ConnectionStore, MemoryConnectionStore};
        let server = MockServer::start();
        // token refresh endpoint
        let m_token = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "access_token": "new_tok",
                    "token_type": "bearer",
                    "expires_in": 3600,
                    "refresh_token": "new_rt"
                }));
        });
        // API requiring bearer
        let m_api = server.mock(|when, then| {
            when.method(GET)
                .path("/secure")
                .header("authorization", "Bearer new_tok");
            then.status(200).json_body(json!({"ok": true}));
        });

        // Router with store-backed connection handlers
        #[derive(Clone)]
        struct Router {
            store: MemoryConnectionStore,
        }
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "connection.read" => crate::authflow::actions::ConnectionReadHandler {
                        ctx: crate::authflow::actions::ConnectionContext::new(self.store.clone()),
                    }
                    .execute(resource, state_name, ctx),
                    "connection.update" => crate::authflow::actions::ConnectionUpdateHandler {
                        ctx: crate::authflow::actions::ConnectionContext::new(self.store.clone()),
                    }
                    .execute(resource, state_name, ctx),
                    "oauth2.refresh_token" => crate::authflow::actions::OAuth2RefreshTokenHandler
                        .execute(resource, state_name, ctx),
                    "inject.bearer" => crate::authflow::actions::InjectBearerHandler
                        .execute(resource, state_name, ctx),
                    "http.request" => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }

        let store = MemoryConnectionStore::default();
        // Seed a typed Connection instead of raw JSON
        let mut conn1 =
            crate::models::AuthConnection::new("tenant1", "prov", "user1", "old").unwrap();
        conn1.update_refresh_token(Some("rt".to_string()));
        conn1.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        futures::executor::block_on(store.put("c1", &conn1)).unwrap();

        let yaml = format!(
            r#"
comment: "read -> refresh -> update -> inject -> http"
startAt: "Read"
states:
  Read:
    type: task
    resource: "connection.read"
    parameters:
      connection_ref: "c1"
    assign:
      refresh_token: "{{% result.refreshToken %}}"
    next: "Refresh"
  Refresh:
    type: task
    resource: "oauth2.refresh_token"
    parameters:
      tokenUrl: "{}{}"
      clientId: "cid"
      clientSecret: "sec"
      refresh_token: "{{% $refresh_token %}}"
    assign:
      new_access_token: "{{% result.access_token %}}"
      new_refresh_token: "{{% result.refresh_token %}}"
    next: "Update"
  Update:
    type: task
    resource: "connection.update"
    parameters:
      connection_ref: "c1"
      access_token: "{{% $new_access_token %}}"
      refresh_token: "{{% $new_refresh_token %}}"
      expires: 999999
    assign:
      updated_access_token: "{{% result.accessToken %}}"
    next: "Inject"
  Inject:
    type: task
    resource: "inject.bearer"
    parameters:
      token: "{{% $updated_access_token %}}"
    assign:
      headers: "{{% result.headers %}}"
    next: "Call"
  Call:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers: "{{% $headers %}}"
    output: "{{% result.status %}}"
    end: true
"#,
            server.base_url(),
            "/token",
            server.base_url(),
            "/secure"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        let router = Router {
            store: store.clone(),
        };
        let out = run_flow(&dsl, &dsl.start_at, json!({}), &router, 50).unwrap();
        m_token.assert();
        m_api.assert();
        assert_eq!(out["states"]["Call"]["result"], json!(200));
    }

    #[test]
    fn ensure_auto_refresh_before_inject() {
        use crate::store::MemoryConnectionStore;
        let server = MockServer::start();
        let m_token = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200).header("Content-Type", "application/json")
                .json_body(json!({"access_token": "new_auto", "token_type": "bearer", "expires_in": 3600, "refresh_token": "nrt"}));
        });
        let m_api = server.mock(|when, then| {
            when.method(GET)
                .path("/secure")
                .header("authorization", "Bearer new_auto");
            then.status(200).json_body(json!({"ok": true}));
        });

        #[derive(Clone)]
        struct Router {
            store: std::sync::Arc<dyn crate::store::ConnectionStore>,
        }
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "ensure.fresh_token" => crate::authflow::actions::EnsureFreshTokenHandler {
                        store: self.store.clone(),
                    }
                    .execute(resource, state_name, ctx),
                    "inject.bearer" => crate::authflow::actions::InjectBearerHandler
                        .execute(resource, state_name, ctx),
                    "http.request" => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }

        let store = std::sync::Arc::new(MemoryConnectionStore::default())
            as std::sync::Arc<dyn crate::store::ConnectionStore>;
        // expired token
        let mut conn2 =
            crate::models::AuthConnection::new("tenant2", "prov", "user2", "old").unwrap();
        conn2.update_refresh_token(Some("rt2".to_string()));
        conn2.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        futures::executor::block_on(store.put("c2", &conn2)).unwrap();

        let yaml = format!(
            r#"
comment: "ensure->inject->call"
startAt: "Ensure"
states:
  Ensure:
    type: task
    resource: "ensure.fresh_token"
    parameters:
      connection_ref: "c2"
      tokenUrl: "{}{}"
      clientId: "cid"
      clientSecret: "sec"
      skewSeconds: 60
    assign:
      ensured_access_token: "{{% result.accessToken %}}"
    next: "Inject"
  Inject:
    type: task
    resource: "inject.bearer"
    parameters:
      token: "{{% $ensured_access_token %}}"
    assign:
      headers: "{{% result.headers %}}"
    next: "Call"
  Call:
    type: task
    resource: "http.request"
    parameters:
      method: "GET"
      url: "{}{}"
      headers: "{{% $headers %}}"
    output: "{{% result.status %}}"
    end: true
"#,
            server.base_url(),
            "/token",
            server.base_url(),
            "/secure"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        let router = Router {
            store: store.clone(),
        };
        let out = run_flow(&dsl, &dsl.start_at, json!({}), &router, 50).unwrap();
        m_token.assert();
        m_api.assert();
        assert_eq!(out["states"]["Call"]["result"], json!(200));
    }

    #[test]
    fn oauth2_authorization_code_with_pkce_and_token_exchange() {
        let server = MockServer::start();
        let m_token = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"access_token":"tok","token_type":"bearer","expires_in":3600}));
        });

        let yaml = format!(
            r#"
comment: "auth code + pkce"
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
      returned_state: "{{% $auth_state %}}"
      expected_state: "{{% $auth_state %}}"
      code: "the_code"
      expected_pkce:
        code_verifier: "{{% $auth_code_verifier %}}"
    assign:
      callback_code: "{{% result.code %}}"
      callback_code_verifier: "{{% result.code_verifier %}}"
    next: "Exchange"
  Exchange:
    type: task
    resource: "http.request"
    parameters:
      method: "POST"
      url: "{}{}"
      headers:
        Content-Type: "application/x-www-form-urlencoded"
      query: {{}}
      body:
        grant_type: "authorization_code"
        client_id: "cid"
        redirect_uri: "https://app/cb"
        code: "{{% $callback_code %}}"
        code_verifier: "{{% $callback_code_verifier %}}"
    output: "{{% result.body.access_token %}}"
    end: true
"#,
            server.base_url(),
            "/token"
        );

        let dsl: WorkflowDSL = serde_yaml::from_str(&yaml).unwrap();
        struct Router;
        impl TaskHandler for Router {
            fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
                match resource {
                    "oauth2.authorize_redirect" => {
                        crate::authflow::actions::OAuth2AuthorizeRedirectHandler
                            .execute(resource, state_name, ctx)
                    }
                    "oauth2.await_callback" => crate::authflow::actions::OAuth2AwaitCallbackHandler
                        .execute(resource, state_name, ctx),
                    "http.request" => {
                        crate::authflow::actions::HttpTaskHandler.execute(resource, state_name, ctx)
                    }
                    _ => anyhow::bail!("unknown resource {resource}"),
                }
            }
        }
        let out = run_flow(&dsl, &dsl.start_at, json!({}), &Router, 30).unwrap();
        m_token.assert();
        assert_eq!(out["states"]["Exchange"]["result"], json!("tok"));
    }

    #[test]
    fn oauth2_refresh_token_success_and_invalid_grant() {
        let server = MockServer::start();
        // success
        let m_ok = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "access_token": "new_abc",
                    "token_type": "bearer",
                    "expires_in": 1800,
                    "refresh_token": "new_refresh"
                }));
        });
        // invalid_grant
        let m_err = server.mock(|when, then| {
            when.method(POST).path("/bad");
            then.status(400)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "error": "invalid_grant"
                }));
        });

        // success flow
        let yaml_ok = format!(
            r#"
comment: "Refresh OK"
startAt: "R"
states:
  R:
    type: task
    resource: "oauth2.refresh_token"
    parameters:
      tokenUrl: "{}{}"
      clientId: "cid"
      clientSecret: "sec"
      refresh_token: "old_refresh"
    output:
      at: "{{% result.access_token %}}"
      rt: "{{% result.refresh_token %}}"
      exp: "{{% result.expires_in %}}"
    end: true
"#,
            server.base_url(),
            "/token"
        );

        let dsl_ok: WorkflowDSL = serde_yaml::from_str(&yaml_ok).unwrap();
        use crate::authflow::actions::OAuth2RefreshTokenHandler;
        let out_ok = run_flow(
            &dsl_ok,
            &dsl_ok.start_at,
            json!({}),
            &OAuth2RefreshTokenHandler,
            10,
        )
        .unwrap();
        println!(
            "OAuth2 refresh output: {}",
            serde_json::to_string_pretty(&out_ok).unwrap()
        );
        m_ok.assert();
        assert_eq!(out_ok["states"]["R"]["result"]["at"], json!("new_abc"));
        assert_eq!(out_ok["states"]["R"]["result"]["rt"], json!("new_refresh"));

        // invalid_grant flow
        let yaml_err = format!(
            r#"
comment: "Refresh invalid_grant"
startAt: "R"
states:
  R:
    type: task
    resource: "oauth2.refresh_token"
    parameters:
      tokenUrl: "{}{}"
      clientId: "cid"
      clientSecret: "sec"
      refresh_token: "bad"
    end: true
"#,
            server.base_url(),
            "/bad"
        );

        let dsl_err: WorkflowDSL = serde_yaml::from_str(&yaml_err).unwrap();
        let err = run_flow(
            &dsl_err,
            &dsl_err.start_at,
            json!({}),
            &OAuth2RefreshTokenHandler,
            10,
        )
        .err()
        .unwrap();
        m_err.assert();
        assert!(format!("{}", err).contains("oauth2 refresh_token request failed"));
    }

    #[cfg(feature = "vault")]
    #[test]
    fn vault_kv2_resolve_with_httpmock() {
        use httpmock::prelude::*;
        // Mock Vault KV v2 read: GET /v1/<mount>/data/<path>
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(GET).path("/v1/kv/data/myapp/config");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "request_id": "req-1",
                    "lease_id": "",
                    "renewable": false,
                    "lease_duration": 0,
                    "data": {
                        "data": {
                            "db": { "password": "s3cr3t" }
                        },
                        "metadata": {
                            "created_time": "2020-01-01T00:00:00Z",
                            "deletion_time": "",
                            "destroyed": false,
                            "version": 1
                        }
                    },
                    "wrap_info": null,
                    "warnings": null,
                    "auth": null
                }));
        });

        // Build Vault client pointing to mock server
        let settings = vaultrs::client::VaultClientSettingsBuilder::default()
            .address(&server.base_url())
            .token("devtoken")
            .build()
            .unwrap();
        let client = vaultrs::client::VaultClient::new(settings).unwrap();

        // Use VaultSecretsProvider via SecretsResolveHandler
        use crate::authflow::actions::{SecretsResolveHandler, VaultSecretsProvider};
        let handler = SecretsResolveHandler::new(VaultSecretsProvider::new(client));
        let out = handler
            .execute(
                "secrets.resolve",
                "S",
                &json!({"uri": "vault://kv/myapp/config#/db/password"}),
            )
            .unwrap();

        m.assert();
        assert_eq!(out["value"], json!("s3cr3t"));
    }
}

// helper to allow custom handler in tests
