use anyhow::{Result, anyhow};
use crate::app::service::OpenActService;
use crate::cli::commands::{Cli, OauthCmd};
use crate::store::ConnectionStore;

pub async fn handle_oauth_command(cli: &Cli, service: &OpenActService, cmd: &OauthCmd) -> Result<()> {
    match cmd {
        OauthCmd::Start { dsl, open_browser } => {
            let dsl_path = dsl.clone();
            let yaml = std::fs::read_to_string(dsl)?;
            let wf: stepflow_dsl::WorkflowDSL = serde_yaml::from_str(&yaml)?;
            let run_store = crate::store::MemoryRunStore::default();
            let router = crate::authflow::actions::DefaultRouter; // not Default
            let res =
                crate::authflow::workflow::start_obtain(&wf, &router, &run_store, serde_json::json!({}))?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::to_value(res)?)?
                );
            } else {
                println!("run_id: {}", res.run_id);
                println!("authorize_url: {}", res.authorize_url);
                println!("state: {}", res.state);
                if let Some(v) = res.code_verifier {
                    println!("code_verifier: {}", v);
                }
                if *open_browser {
                    let _ = opener::open(res.authorize_url.as_str());
                }
                println!();
                println!("Next:");
                println!(
                    "  1) Open the authorize_url above in a browser and complete consent"
                );
                println!(
                    "  2) Copy the code and state provided by the provider after redirect"
                );
                println!("  3) Resume the flow with:");
                println!(
                    "     openact-cli oauth resume --dsl {} --run-id {} --code <code> --state <state> [--bind-connection <connection_trn>]",
                    dsl_path.display(),
                    res.run_id
                );
                println!(
                    "Tip: You can pass --bind-connection to immediately bind credentials to a connection."
                );
            }
        }
        OauthCmd::Resume {
            dsl,
            run_id,
            code,
            state,
            bind_connection,
        } => {
            let yaml = std::fs::read_to_string(dsl)?;
            let dsl: stepflow_dsl::WorkflowDSL = serde_yaml::from_str(&yaml)?;
            let run_store = crate::store::MemoryRunStore::default();
            let router = crate::authflow::actions::DefaultRouter; // not Default
            let out = crate::authflow::workflow::resume_obtain(
                &dsl,
                &router,
                &run_store,
                crate::authflow::workflow::ResumeObtainArgs {
                    run_id: run_id.clone(),
                    code: code.clone(),
                    state: state.clone(),
                },
            )?;
            // Optionally bind
            if let Some(conn_trn) = bind_connection {
                // Expect the flow to output an auth connection TRN at /states/Exchange/result or similar
                // Here we allow either direct string or nested field `auth_trn`
                let auth_trn = out
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        out.get("auth_trn")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                if !auth_trn.is_empty() {
                    let manager = service.database();
                    let repo = manager.connection_repository();
                    let mut conn = repo
                        .get_by_trn(&conn_trn)
                        .await?
                        .ok_or_else(|| anyhow!("connection not found: {}", conn_trn))?;
                    conn.auth_ref = Some(auth_trn.clone());
                    repo.upsert(&conn).await?;
                    println!("bound: connection={} -> auth_ref={}", conn_trn, auth_trn);
                    println!("Next: check status and test the connection:");
                    println!("  openact-cli connection status {}", conn_trn);
                    println!("  openact-cli connection test {}", conn_trn);
                } else {
                    println!("[warn] cannot detect auth_trn from flow output; skip bind");
                    println!(
                        "Hint: Provide --bind-connection again after you locate the auth_trn in the output."
                    );
                }
            }
            if cli.json {
            println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("✅ Authorization flow completed.");
                // Try to detect auth_trn for guidance
                let auth_trn = out.as_str().map(|s| s.to_string()).or_else(|| {
                    out.get("auth_trn")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
                if let Some(trn_str) = &auth_trn {
                    println!("auth_trn: {}", trn_str);
                    println!("Next: bind to a connection if not yet bound:");
                    println!(
                        "  openact-cli oauth bind --connection-trn <connection_trn> --auth-trn {}",
                        trn_str
                    );
                }
                println!("Full output:");
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        }
        OauthCmd::Bind {
            connection_trn,
            auth_trn,
        } => {
            let manager = service.database();
            let repo = manager.connection_repository();
            let mut conn = repo
                .get_by_trn(connection_trn)
                .await?
                .ok_or_else(|| anyhow!("connection not found: {}", connection_trn))?;
            conn.auth_ref = Some(auth_trn.clone());
            repo.upsert(&conn).await?;
            println!(
                "bound: connection={} -> auth_ref={}",
                connection_trn, auth_trn
            );
        }
        OauthCmd::DeviceCode {
            token_url,
            device_code_url,
            client_id,
            client_secret,
            scope,
            tenant,
            provider,
            user_id,
            bind_connection,
        } => {
            // Step 1: device authorization request
            let mut form = vec![("client_id", client_id.as_str())];
            if let Some(s) = scope {
                form.push(("scope", s.as_str()));
            }
            let resp = reqwest::Client::new()
                .post(device_code_url)
                .form(&form)
                .send()
                .await?;
            if !resp.status().is_success() {
                return Err(anyhow!("device_code request failed: {}", resp.status()));
            }
            let payload: serde_json::Value = resp.json().await?;
            let device_code = payload
                .get("device_code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing device_code"))?;
            let user_code = payload
                .get("user_code")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let verification_uri = payload
                .get("verification_uri_complete")
                .and_then(|v| v.as_str())
                .or_else(|| payload.get("verification_uri").and_then(|v| v.as_str()))
                .ok_or_else(|| anyhow!("missing verification_uri"))?;
            let interval = payload
                .get("interval")
                .and_then(|v| v.as_u64())
                .unwrap_or(5);
            if !cli.json {
                println!("Please open the URL and enter the code:");
                println!("  {}", verification_uri);
                if !user_code.is_empty() {
                    println!("User Code: {}", user_code);
                }
                println!("Polling token endpoint every {}s...", interval);
            }

            // Step 2: poll token endpoint
            let token_resp = loop {
                let mut form = vec![
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device_code),
                    ("client_id", client_id.as_str()),
                ];
                if let Some(cs) = client_secret {
                    form.push(("client_secret", cs.as_str()));
                }

                let r = reqwest::Client::new()
                    .post(token_url)
                    .form(&form)
                    .send()
                    .await?;
                if r.status().is_success() {
                    break r;
                } else {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    if body.contains("authorization_pending") || body.contains("slow_down")
                    {
                        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                        continue;
                    }
                    return Err(anyhow!("token polling failed: {} - {}", status, body));
                }
            };
            let token_json: serde_json::Value = token_resp.json().await?;
            let access_token = token_json
                .get("access_token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing access_token"))?
                .to_string();
            let refresh_token = token_json
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let expires_in = token_json
                .get("expires_in")
                .and_then(|v| v.as_i64())
                .unwrap_or(3600);
            let scope_val = token_json
                .get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

            // Step 3: persist as AuthConnection
            let ac = crate::models::AuthConnection::new_with_params(
                tenant.clone(),
                provider.clone(),
                user_id.clone(),
                access_token,
                refresh_token,
                Some(expires_at),
                Some("Bearer".to_string()),
                scope_val,
                None,
            )?;
            let trn_str = ac.trn.to_string();
            let storage = service.storage();
            storage.put(&trn_str, &ac).await?;
            if !cli.json {
                println!("✅ Device code flow completed. auth_trn: {}", trn_str);
            }

            // Optional bind to connection
            if let Some(conn_trn) = bind_connection {
                let manager = service.database();
                let repo = manager.connection_repository();
                let mut conn = repo
                    .get_by_trn(&conn_trn)
                    .await?
                    .ok_or_else(|| anyhow!("connection not found: {}", conn_trn))?;
                conn.auth_ref = Some(trn_str.clone());
                repo.upsert(&conn).await?;
                println!("bound: connection={} -> auth_ref={}", conn_trn, trn_str);
                println!("Next: test and check status:");
                println!("  openact-cli connection status {}", conn_trn);
                println!("  openact-cli connection test {}", conn_trn);
            }

            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "auth_trn": trn_str
                    }))?
                );
            }
        }
    }
    Ok(())
}
