use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use openact_core::action_registry::ActionRegistry;
use openact_core::{
    AuthManager, AuthOrchestrator, BindingManager, CoreConfig, CoreContext, ExecutionEngine,
};

#[derive(Parser)]
#[command(name = "openact")]
#[command(about = "OpenAct CLI", version)]
struct Cli {
    /// Global: only print machine-readable JSON to stdout; send other logs to stderr
    #[arg(long, global = true)]
    json_only: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show health and stats
    Status {
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
    },
    /// OAuth2 login from DSL config
    AuthLogin(AuthLoginArgs),
    /// List auth connections
    AuthList {
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
    },
    /// Delete an auth connection
    AuthDelete { trn: String },
    /// Create a PAT-based auth connection
    AuthCreatePat {
        tenant: String,
        provider: String,
        user_id: String,
        token_env: Option<String>,
    },
    /// Create a PAT-based auth connection from config file
    AuthPat { tenant: String, config: String },
    /// Inspect an auth connection by TRN (mask sensitive fields)
    AuthInspect {
        trn: String,
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
    },
    /// Refresh an auth connection by TRN (if provider supports refresh)
    AuthRefresh { trn: String },
    /// Begin OAuth and output authorize URL; save session to file
    AuthBegin {
        tenant: String,
        config: String,
        #[arg(help = "Flow name", default_value = "OAuth")]
        flow: String,
        #[arg(help = "Redirect URI", required = false)]
        redirect: Option<String>,
        #[arg(help = "Scope", required = false)]
        scope: Option<String>,
        #[arg(long, help = "Wait for callback and auto-complete (no manual copy)")]
        wait: bool,
        #[arg(
            long,
            help = "Do not delete session file after success when --wait is used"
        )]
        keep_session: bool,
        #[arg(long, help = "Open browser after printing URL")]
        open_browser: bool,
    },
    /// Complete OAuth using saved session and callback URL
    AuthComplete {
        tenant: String,
        config: String,
        #[arg(help = "Path to session file created by auth-begin")]
        session: String,
        #[arg(help = "Callback URL containing code/state")]
        callback_url: String,
    },
    /// List saved OAuth sessions (~/.openact/sessions)
    AuthSessionList,
    /// Clean saved OAuth sessions by id or all
    AuthSessionClean {
        /// Delete all sessions
        #[arg(long, default_value_t = false)]
        all: bool,
        /// Session ids (uuid without extension) or paths to delete
        ids: Vec<String>,
    },
    /// Repair historical auth key versions (set NULL/0 to 1 or 0)
    AuthRepairKeys {
        /// tenant filter (optional)
        #[arg(long)]
        tenant: Option<String>,
        /// provider filter (optional)
        #[arg(long)]
        provider: Option<String>,
        /// target key version (0 for no-encryption, 1 for encrypted)
        #[arg(long, default_value_t = 1)]
        to_version: i32,
        /// dry run only
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Bind auth to action
    BindingBind {
        tenant: String,
        auth_trn: String,
        action_trn: String,
    },
    /// Unbind auth and action
    BindingUnbind {
        tenant: String,
        auth_trn: String,
        action_trn: String,
    },
    /// List bindings by tenant
    BindingList {
        tenant: String,
        #[arg(long)]
        auth_trn: Option<String>,
        #[arg(long)]
        action_trn: Option<String>,
        #[arg(long, default_value_t = false)]
        verbose: bool,
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
    },
    /// Run an action with an execution TRN
    Run {
        tenant: String,
        action_trn: String,
        exec_trn: String,
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        #[arg(long, default_value_t = false)]
        trace: bool,
        #[arg(
            long,
            help = "Path to input JSON file (path_params/query/headers/body)"
        )]
        input_json: Option<String>,
        #[arg(
            long,
            help = "Path parameter k=v; repeatable",
            value_parser,
            number_of_values = 1
        )]
        path: Vec<String>,
        #[arg(
            long,
            help = "Query parameter k=v; repeatable",
            value_parser,
            number_of_values = 1
        )]
        query: Vec<String>,
        #[arg(
            long,
            help = "Header K=V; repeatable",
            value_parser,
            number_of_values = 1
        )]
        header: Vec<String>,
        #[arg(long, help = "Inline JSON body string", conflicts_with = "body")]
        body_json: Option<String>,
        #[arg(
            long,
            help = "Body file prefixed with @, e.g. --body @file.json",
            conflicts_with = "body_json"
        )]
        body: Option<String>,
        #[arg(long, default_value_t = false, help = "Fetch all pages if action supports pagination")]
        all_pages: bool,
        #[arg(long, help = "Maximum number of pages to fetch")]
        max_pages: Option<u64>,
        #[arg(long, help = "Per-page size hint if supported")]
        per_page: Option<u64>,
        #[arg(long)]
        save: Option<String>,
        #[arg(
            long,
            help = "Form fields k=v for multipart upload; repeatable",
            value_parser,
            number_of_values = 1
        )]
        form: Vec<String>,
        #[arg(
            long,
            help = "File parts field=@path; repeatable",
            value_parser,
            number_of_values = 1
        )]
        file: Vec<String>,
        #[arg(
            long,
            help = "Download non-JSON responses to file path"
        )]
        download_to: Option<String>,
        #[arg(long, default_value_t = false, help = "Treat application/x-ndjson as stream and aggregate lines")]
        stream: bool,
        #[arg(long, default_value_t = false, help = "Print retry summary (attempts_total, retries, last_status, last_error_class)")]
        retry_summary: bool,
    },
    /// Register an action from a YAML file into DB
    ActionRegister {
        tenant: String,
        provider: String,
        name: String,
        trn: String,
        yaml_path: String,
    },
    /// Delete an action by TRN
    ActionDelete { trn: String },
    /// Inspect an action by TRN
    ActionInspect {
        trn: String,
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
    },
    /// List actions by tenant
    ActionList {
        tenant: String,
        #[arg(long, value_parser = ["text","json"], default_value = "text")]
        output: String,
    },
    /// Update an action's YAML by TRN
    ActionUpdate { trn: String, yaml_path: String },
    /// Export an action's YAML by TRN to stdout
    ActionExport { trn: String },
    /// Diagnose environment and configuration
    Doctor {
        #[arg(long, help = "Optional DSL path to validate secrets mapping")]
        dsl: Option<String>,
        #[arg(long, default_value_t = 8080)]
        port_start: u16,
        #[arg(long, default_value_t = 8099)]
        port_end: u16,
    },
    /// Inspect a saved OAuth session file and print details
    AuthSessionInspect { session: String },
}

#[derive(Args)]
struct AuthLoginArgs {
    #[arg(help = "Tenant to login to")]
    tenant: String,
    #[arg(help = "Path to DSL config file")]
    config: String,
    #[arg(help = "Flow to use (e.g., 'code')")]
    flow: Option<String>,
    #[arg(help = "Redirect URI to use")]
    redirect: Option<String>,
    #[arg(help = "Scope to request")]
    scope: Option<String>,
    #[arg(long, help = "Wait for callback (default: false)")]
    wait: bool,
    #[arg(long = "open-browser", help = "Open browser (default: false)")]
    open_browser: bool,
    #[arg(help = "Callback URL to use (overrides wait)")]
    callback_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Route tracing logs to stderr so stdout remains clean for JSON outputs
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let cfg = CoreConfig::from_env();
    let ctx = CoreContext::initialize(&cfg).await?;

    let json_only = cli.json_only;

    match cli.command {
        Commands::Status { output } => {
            let force_json = json_only || output == "json";
            if force_json {
                let s = ctx.stats().await?;
                let env = {
                    let master_from = if std::env::var("AUTHFLOW_MASTER_KEY").is_ok() {
                        Some("AUTHFLOW_MASTER_KEY")
                    } else if std::env::var("OPENACT_MASTER_KEY").is_ok() {
                        Some("OPENACT_MASTER_KEY")
                    } else {
                        None
                    };
                    serde_json::json!({
                        "master_key": master_from.unwrap_or("(not set)"),
                    })
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "ok": true,
                        "data": { "stats": {"bindings": s.bindings, "actions": s.actions, "auth_connections": s.auth_connections}, "env": env }
                    }))?
                );
            } else {
                ctx.health().await?;
                let s = ctx.stats().await?;
                println!("ok");
                println!(
                    "bindings={} actions={} auth_connections={}",
                    s.bindings, s.actions, s.auth_connections
                );
            }
        }
        Commands::AuthList { output } => {
            let force_json = json_only || output == "json";
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            let refs = am.list().await?;
            if force_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "ok": true, "data": { "connections": refs } })
                    )?
                );
            } else {
                for r in refs {
                    println!("{}", r);
                }
            }
        }
        Commands::ActionList { tenant, output } => {
            let force_json = json_only || output == "json";
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            let list = registry.list_by_tenant(&tenant).await?;
            if force_json {
                let items: Vec<serde_json::Value> = list.into_iter().map(|a| serde_json::json!({
                    "trn": a.trn, "tenant": a.tenant, "provider": a.provider, "name": a.name, "active": a.is_active
                })).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "ok": true, "data": { "actions": items } })
                    )?
                );
            } else {
                for a in list {
                    println!("{} {} {} {}", a.trn, a.tenant, a.provider, a.name);
                }
            }
        }
        Commands::BindingList {
            tenant,
            auth_trn,
            action_trn,
            verbose,
            output,
        } => {
            let force_json = json_only || output == "json";
            let bm = BindingManager::new(ctx.db.pool().clone());
            let rows = bm.list_by_tenant(&tenant).await?;
            let rows = rows
                .into_iter()
                .filter(|b| {
                    (auth_trn.as_ref().map(|x| &b.auth_trn == x).unwrap_or(true))
                        && (action_trn
                            .as_ref()
                            .map(|x| &b.action_trn == x)
                            .unwrap_or(true))
                })
                .collect::<Vec<_>>();
            if force_json {
                let items: Vec<serde_json::Value> = rows
                    .into_iter()
                    .map(|b| {
                        let mut o = serde_json::Map::new();
                        o.insert("tenant".to_string(), serde_json::json!(b.tenant));
                        o.insert("auth_trn".to_string(), serde_json::json!(b.auth_trn));
                        o.insert("action_trn".to_string(), serde_json::json!(b.action_trn));
                        if verbose {
                            o.insert(
                                "created_by".to_string(),
                                serde_json::to_value(&b.created_by)
                                    .unwrap_or(serde_json::Value::Null),
                            );
                            o.insert(
                                "created_at".to_string(),
                                serde_json::to_value(&b.created_at)
                                    .unwrap_or(serde_json::Value::Null),
                            );
                        }
                        serde_json::Value::Object(o)
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "ok": true, "data": { "bindings": items } })
                    )?
                );
            } else {
                for b in rows {
                    if verbose {
                        println!(
                            "tenant={} auth={} action={} created_by={:?} created_at={:?}",
                            b.tenant, b.auth_trn, b.action_trn, b.created_by, b.created_at
                        );
                    } else {
                        println!("{} -> {}", b.auth_trn, b.action_trn);
                    }
                }
            }
        }
        Commands::AuthLogin(args) => {
            let orch = AuthOrchestrator::new(ctx.db.pool().clone());
            let config_path = std::path::Path::new(&args.config);
            // Determine redirect URI: use provided one, otherwise auto-pick an available port when waiting
            fn find_available_port(start: u16, end: u16) -> Option<u16> {
                // Prefer common callback ports first
                let preferred = [8080u16, 8081, 8082, 8083, 8084, 8085];
                for &p in &preferred {
                    if p >= start
                        && p <= end
                        && std::net::TcpListener::bind(("127.0.0.1", p)).is_ok()
                    {
                        return Some(p);
                    }
                }
                // Fallback to full range scan
                for port in start..=end {
                    if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                        return Some(port);
                    }
                }
                None
            }
            let chosen_redirect: Option<String> = if let Some(r) = args.redirect.clone() {
                Some(r)
            } else if args.wait {
                let port = find_available_port(8080, 8099).unwrap_or(8080);
                let uri = format!("http://localhost:{}/oauth/callback", port);
                println!("🔧 自动选择端口: {} -> {}", port, uri);
                Some(uri)
            } else {
                None
            };
            if let Some(ref r) = chosen_redirect {
                println!("redirect_uri={}", r);
            }
            let (auth_url, pending) = orch
                .begin_oauth_from_config(
                    &args.tenant,
                    config_path,
                    args.flow.as_deref(),
                    chosen_redirect.as_deref(),
                    args.scope.as_deref(),
                )
                .await?;
            println!("🔗 授权链接: {}", auth_url);
            if args.open_browser {
                print!("🌐 正在打开浏览器...");
                let result = if cfg!(target_os = "macos") {
                    std::process::Command::new("open").arg(&auth_url).status()
                } else if cfg!(target_os = "linux") {
                    std::process::Command::new("xdg-open")
                        .arg(&auth_url)
                        .status()
                } else {
                    Ok(Default::default())
                };
                if result.is_ok() {
                    println!(" ✅");
                } else {
                    println!(" ❌ 自动打开失败，请手动复制上面的链接");
                }
            }
            let callback_url = if args.wait {
                let redirect = chosen_redirect
                    .clone()
                    .unwrap_or_else(|| "http://localhost:8080/oauth/callback".to_string());
                let url = url::Url::parse(&redirect)
                    .map_err(|e| anyhow::anyhow!(format!("bad redirect: {}", e)))?;
                let host = url.host_str().unwrap_or("127.0.0.1");
                let port = url.port().unwrap_or(8085);
                let path = url.path().to_string();
                println!("⏳ 等待授权回调: http://{}:{}{}", host, port, path);
                println!("📝 请在浏览器中完成授权...");
                let listener = std::net::TcpListener::bind((host, port))?;
                listener.set_nonblocking(false)?;
                let (mut stream, _addr) = listener.accept()?;
                use std::io::{Read, Write};
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf)?;
                let req = String::from_utf8_lossy(&buf[..n]);
                let first = req.lines().next().unwrap_or("");
                let mut got = String::new();
                if let Some(rest) = first.strip_prefix("GET ") {
                    if let Some(p) = rest.strip_suffix(" HTTP/1.1") {
                        got = format!("http://{}:{}{}", host, port, p);
                    }
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nOK",
                );
                got
            } else if let Some(cb) = args.callback_url {
                cb
            } else {
                println!("paste_callback_url_and_press_enter:");
                let mut s = String::new();
                use std::io::Read;
                std::io::stdin().read_to_string(&mut s)?;
                s.trim().to_string()
            };
            let trn = orch.complete_oauth_with_callback(pending, &callback_url)?;
            println!("🎉 认证成功! TRN: {}", trn);
        }
        Commands::AuthDelete { trn } => {
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            let ok = am.delete(&trn).await?;
            if !json_only {
                println!("{}", if ok { "deleted" } else { "not found" });
            }
        }
        Commands::AuthCreatePat {
            tenant,
            provider,
            user_id,
            token_env,
        } => {
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            let env_key = token_env.unwrap_or_else(|| "GITHUB_PAT".to_string());
            let token = std::env::var(&env_key)
                .or_else(|_| std::env::var("GITHUB_TOKEN"))
                .map_err(|_| anyhow::anyhow!(format!("{} or GITHUB_TOKEN not set", env_key)))?;
            let trn = am
                .create_pat_connection(&tenant, &provider, &user_id, &token)
                .await?;
            if !json_only {
                println!("created: {}", trn);
            }
        }
        Commands::AuthPat { tenant, config } => {
            #[derive(serde::Deserialize)]
            struct PatCfg {
                provider: String,
                user_id: String,
                #[serde(default)]
                token_from: Option<TokenFrom>,
                #[serde(default)]
                token: Option<String>,
            }
            #[derive(serde::Deserialize)]
            struct TokenFrom {
                #[serde(default)]
                env: Option<String>,
                #[serde(default)]
                token: Option<String>,
            }
            let text = std::fs::read_to_string(&config)?;
            let cfgf: PatCfg = if config.ends_with(".json") {
                serde_json::from_str(&text)?
            } else {
                serde_yaml::from_str(&text)?
            };
            let token = if let Some(tf) = cfgf.token_from {
                if let Some(e) = tf.env {
                    std::env::var(&e)?
                } else if let Some(t) = tf.token {
                    t
                } else {
                    anyhow::bail!("tokenFrom invalid")
                }
            } else if let Some(t) = cfgf.token {
                t
            } else {
                anyhow::bail!("no token provided (tokenFrom/token)")
            };
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            let trn = am
                .create_pat_connection(&tenant, &cfgf.provider, &cfgf.user_id, &token)
                .await?;
            if !json_only {
                println!("created: {}", trn);
            }
        }
        Commands::AuthInspect { trn, output } => {
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            if let Some(conn) = am.get(&trn).await? {
                let mut extra_masked = conn.extra.clone();
                if let serde_json::Value::Object(ref mut obj) = extra_masked {
                    if obj.contains_key("access_token") {
                        obj.insert("access_token".to_string(), serde_json::json!("***"));
                    }
                    if obj.contains_key("refresh_token") {
                        obj.insert("refresh_token".to_string(), serde_json::json!("***"));
                    }
                }
                let force_json = json_only || output == "json";
                if force_json {
                    let data = serde_json::json!({
                        "trn": conn.trn.to_trn_string().unwrap_or_default(),
                        "tenant": conn.trn.tenant,
                        "provider": conn.trn.provider,
                        "user_id": conn.trn.user_id,
                        "token_type": conn.token_type,
                        "scope": conn.scope,
                        "extra": extra_masked,
                    });
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &serde_json::json!({"ok": true, "data": data})
                        )?
                    );
                } else {
                    println!(
                        "tenant={} provider={} user_id={} token_type={} scope={} trn={} extra={}",
                        conn.trn.tenant,
                        conn.trn.provider,
                        conn.trn.user_id,
                        conn.token_type,
                        conn.scope.unwrap_or_default(),
                        conn.trn.to_trn_string().unwrap_or_default(),
                        extra_masked
                    );
                }
            } else {
                anyhow::bail!(format!("auth not found: {}", trn));
            }
        }
        Commands::AuthRefresh { trn } => {
            // TODO: if provider supports refresh, invoke orchestrator/flow to refresh
            // For now, report not supported
            println!("refresh not supported yet for: {}", trn);
        }
        Commands::AuthBegin {
            tenant,
            config,
            flow,
            redirect,
            scope,
            wait,
            keep_session,
            open_browser,
        } => {
            use std::fs;
            let orch = AuthOrchestrator::new(ctx.db.pool().clone());
            let config_path = std::path::Path::new(&config);
            // Reuse auto-port logic: if no redirect provided, pick a free one
            fn find_available_port(start: u16, end: u16) -> Option<u16> {
                let preferred = [8080u16, 8081, 8082, 8083, 8084, 8085];
                for &p in &preferred {
                    if p >= start
                        && p <= end
                        && std::net::TcpListener::bind(("127.0.0.1", p)).is_ok()
                    {
                        return Some(p);
                    }
                }
                for port in start..=end {
                    if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                        return Some(port);
                    }
                }
                None
            }
            let redirect_final = if let Some(r) = redirect.clone() {
                Some(r)
            } else {
                find_available_port(8080, 8099).map(|p| {
                    let uri = format!("http://localhost:{}/oauth/callback", p);
                    println!("🔧 自动选择端口: {} -> {}", p, uri);
                    uri
                })
            };
            if let Some(ref r) = redirect_final {
                println!("redirect_uri={}", r);
            }
            let (auth_url, pending) = orch
                .begin_oauth_from_config(
                    &tenant,
                    config_path,
                    Some(&flow),
                    redirect_final.as_deref(),
                    scope.as_deref(),
                )
                .await?;
            println!("🔗 授权链接: {}", auth_url);
            if open_browser {
                let _ = if cfg!(target_os = "macos") {
                    std::process::Command::new("open").arg(&auth_url).status()
                } else if cfg!(target_os = "linux") {
                    std::process::Command::new("xdg-open")
                        .arg(&auth_url)
                        .status()
                } else {
                    Ok(Default::default())
                };
            }
            // Save session (flow/next_state/context) to ~/.openact/sessions/<uuid>.json
            let sid = uuid::Uuid::new_v4().to_string();
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let dir = format!("{}/.openact/sessions", home);
            fs::create_dir_all(&dir)?;
            let session_path = format!("{}/{}.json", dir, sid);
            let session_json = serde_json::json!({
                "flow_name": pending.flow_name,
                "next_state": pending.next_state,
                "context": pending.context,
                "redirect_uri": redirect_final,
                "auth_url": auth_url,
            });
            fs::write(&session_path, serde_json::to_string_pretty(&session_json)?)?;
            println!("session_file={}", session_path);

            // Optional: wait for callback and auto-complete
            if wait {
                // Pick host/port/path from redirect
                let redirect = session_json
                    .get("redirect_uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let url = url::Url::parse(&redirect)
                    .map_err(|e| anyhow::anyhow!(format!("bad redirect: {}", e)))?;
                let host = url.host_str().unwrap_or("127.0.0.1");
                let port = url.port().unwrap_or(8080);
                let path = url.path().to_string();
                println!("⏳ 等待授权回调: http://{}:{}{}", host, port, path);
                let listener = std::net::TcpListener::bind((host, port))?;
                let (mut stream, _addr) = listener.accept()?;
                use std::io::{Read, Write};
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf)?;
                let req = String::from_utf8_lossy(&buf[..n]);
                let first = req.lines().next().unwrap_or("");
                let mut callback_url = String::new();
                if let Some(rest) = first.strip_prefix("GET ") {
                    if let Some(p) = rest.strip_suffix(" HTTP/1.1") {
                        callback_url = format!("http://{}:{}{}", host, port, p);
                    }
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nOK",
                );
                // Complete in-process using orchestrator
                let trn = orch.complete_oauth_with_callback(pending, &callback_url)?;
                println!("\u{1F389} 认证成功! TRN: {}", trn);
                // Cleanup session file after success unless keep_session
                if !keep_session {
                    let _ = std::fs::remove_file(&session_path);
                    println!("session_file_deleted={}", session_path);
                }
            }
        }
        Commands::AuthComplete {
            tenant: _,
            config,
            session,
            callback_url,
        } => {
            use serde_json::Value;
            let orch = AuthOrchestrator::new(ctx.db.pool().clone());
            let config_path = std::path::Path::new(&config);
            // Load saved session
            let text = std::fs::read_to_string(&session)?;
            let v: Value = serde_json::from_str(&text)?;
            let flow_name = v
                .get("flow_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("invalid session: flow_name"))?;
            let next_state = v
                .get("next_state")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("invalid session: next_state"))?;
            let mut context = v
                .get("context")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("invalid session: context"))?;
            let expected_state = v
                .get("context")
                .and_then(|c| c.get("state"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            // Parse code/state from callback_url and inject into context
            let parsed = url::Url::parse(&callback_url)?;
            let mut code = String::new();
            let mut state = String::new();
            for (k, val) in parsed.query_pairs() {
                if k == "code" {
                    code = val.to_string();
                }
                if k == "state" {
                    state = val.to_string();
                }
            }
            if code.is_empty() {
                anyhow::bail!("missing code in callback url");
            }
            if !expected_state.is_empty() && !state.is_empty() && expected_state != state {
                anyhow::bail!(format!(
                    "state mismatch: expected={}, got={}",
                    expected_state, state
                ));
            }
            if let Value::Object(ref mut obj) = context {
                obj.insert("code".into(), Value::String(code));
                if !state.is_empty() {
                    obj.insert("state".into(), Value::String(state));
                }
            }
            let trn = orch
                .resume_with_context(config_path, flow_name, next_state, context)
                .await?;
            println!("🎉 认证成功! TRN: {}", trn);
        }
        Commands::AuthRepairKeys {
            tenant,
            provider,
            to_version,
            dry_run,
        } => {
            use sqlx::Row;
            let pool = ctx.db.pool().clone();
            let mut where_clauses: Vec<&str> = Vec::new();
            if tenant.is_some() {
                where_clauses.push("tenant = ?");
            }
            if provider.is_some() {
                where_clauses.push("provider = ?");
            }
            where_clauses.push("(key_version IS NULL OR key_version = 0)");
            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", where_clauses.join(" AND "))
            };
            let sql_list = format!("SELECT trn, tenant, provider, key_version, access_token_nonce FROM auth_connections{}", where_sql);
            let mut q = sqlx::query(&sql_list);
            if let Some(t) = &tenant {
                q = q.bind(t);
            }
            if let Some(p) = &provider {
                q = q.bind(p);
            }
            let rows = q.fetch_all(&pool).await?;
            println!("found {} rows to repair", rows.len());
            if dry_run {
                return Ok(());
            }
            let sql_upd = format!("UPDATE auth_connections SET key_version = ? WHERE trn = ?");
            for r in rows {
                let trn: String = r.get("trn");
                sqlx::query(&sql_upd)
                    .bind(to_version)
                    .bind(&trn)
                    .execute(&pool)
                    .await?;
                println!("repaired: {} -> key_version={} ", trn, to_version);
            }
        }
        Commands::AuthSessionList => {
            use serde_json::Value;
            use std::fs;
            use std::time::UNIX_EPOCH;
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let dir = format!("{}/.openact/sessions", home);
            let rd = fs::read_dir(&dir);
            match rd {
                Ok(entries) => {
                    for e in entries.flatten() {
                        let path = e.path();
                        if path.extension().and_then(|s| s.to_str()) != Some("json") {
                            continue;
                        }
                        let meta = e.metadata().ok();
                        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                        let mtime = meta
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        let id = fname.strip_suffix(".json").unwrap_or(fname);
                        let flow = std::fs::read_to_string(&path)
                            .ok()
                            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                            .and_then(|v| {
                                v.get("flow_name")
                                    .and_then(|x| x.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| "".to_string());
                        println!(
                            "id={} size={}B mtime={} flow={} path={}",
                            id,
                            size,
                            mtime,
                            flow,
                            path.display()
                        );
                    }
                }
                Err(_) => println!("no sessions found"),
            }
        }
        Commands::AuthSessionClean { all, ids } => {
            use std::fs;
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let dir = format!("{}/.openact/sessions", home);
            if all && ids.is_empty() {
                if let Ok(entries) = fs::read_dir(&dir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.extension().and_then(|s| s.to_str()) == Some("json") {
                            let _ = fs::remove_file(&p);
                        }
                    }
                }
                println!("sessions cleaned: all");
            } else if !ids.is_empty() {
                for id in ids {
                    let path = if id.ends_with(".json") {
                        id.clone()
                    } else {
                        format!("{}/{}.json", dir, id)
                    };
                    match fs::remove_file(&path) {
                        Ok(_) => println!("deleted: {}", path),
                        Err(e) => println!("skip: {} ({})", path, e),
                    }
                }
            } else {
                println!("nothing to do: provide --all or session ids");
            }
        }
        Commands::AuthSessionInspect { session } => {
            let text = std::fs::read_to_string(&session)?;
            let v: serde_json::Value = serde_json::from_str(&text)?;
            let flow = v.get("flow_name").and_then(|x| x.as_str()).unwrap_or("");
            let next = v.get("next_state").and_then(|x| x.as_str()).unwrap_or("");
            let redirect = v.get("redirect_uri").and_then(|x| x.as_str()).unwrap_or("");
            let auth_url = v.get("auth_url").and_then(|x| x.as_str()).unwrap_or("");
            println!(
                "flow={} next_state={} redirect_uri={} auth_url={}",
                flow, next, redirect, auth_url
            );
            if let Some(state) = v.pointer("/context/state").and_then(|x| x.as_str()) {
                println!("state={}", state);
            }
        }
        Commands::BindingBind {
            tenant,
            auth_trn,
            action_trn,
        } => {
            // Pre-checks: action exists
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            if let Err(e) = registry.get_by_trn(&action_trn).await {
                let msg = format!("{}", e);
                if msg.contains("no rows returned") || msg.contains("not found") {
                    anyhow::bail!(format!("action not found: {}", action_trn));
                } else {
                    anyhow::bail!(e);
                }
            }
            // Pre-checks: auth exists
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            let exists = am.get(&auth_trn).await?.is_some();
            if !exists {
                anyhow::bail!(format!("auth not found: {}", auth_trn));
            }

            let bm = BindingManager::new(ctx.db.pool().clone());
            let b = bm
                .bind(&tenant, &auth_trn, &action_trn, Some("cli"))
                .await?;
            if !json_only {
                println!("bound: {} -> {}", b.auth_trn, b.action_trn);
            }
        }
        Commands::BindingUnbind {
            tenant,
            auth_trn,
            action_trn,
        } => {
            // Pre-checks with friendly errors
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            if let Err(e) = registry.get_by_trn(&action_trn).await {
                let msg = format!("{}", e);
                if msg.contains("no rows returned") || msg.contains("not found") {
                    anyhow::bail!(format!("action not found: {}", action_trn));
                } else {
                    anyhow::bail!(e);
                }
            }
            let am = AuthManager::from_database_url(cfg.database_url.clone()).await?;
            if am.get(&auth_trn).await?.is_none() {
                anyhow::bail!(format!("auth not found: {}", auth_trn));
            }
            let bm = BindingManager::new(ctx.db.pool().clone());
            let ok = bm.unbind(&tenant, &auth_trn, &action_trn).await?;
            if !json_only {
                println!("{}", if ok { "unbound" } else { "not found" });
            }
        }
        Commands::Run {
            tenant,
            action_trn,
            exec_trn,
            output,
            dry_run,
            trace,
            input_json,
            path,
            query,
            header,
            body_json,
            body,
            all_pages,
            max_pages,
            per_page,
            save,
            form,
            file,
            download_to,
            stream,
            retry_summary,
        } => {
            // Prefer TRN-based execution via core engine (unified persistence)
            let engine = ExecutionEngine::new(ctx.db.clone());

            let force_json = json_only || output == "json";
            if dry_run {
                let preview = serde_json::json!({
                    "tenant": tenant,
                    "action_trn": action_trn,
                    "exec_trn": exec_trn,
                    "trace": trace,
                });
                if force_json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "ok": true,
                            "data": preview
                        }))?
                    );
                } else {
                    println!("[dry-run] 即将执行: {}", preview);
                }
                if let Some(file) = save {
                    std::fs::write(file, serde_json::to_string_pretty(&preview)?)?;
                }
                return Ok(());
            }

            // Build optional input from --input-json
            // Merge input from flags and --input-json (flags take precedence)
            let from_file = if let Some(p) = input_json {
                let text = std::fs::read_to_string(&p).map_err(|e| {
                    anyhow::anyhow!(format!("failed to read input file {}: {}", p, e))
                })?;
                let v: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| anyhow::anyhow!(format!("invalid input json: {}", e)))?;
                let path_params = v
                    .get("path_params")
                    .and_then(|x| x.as_object())
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                let query = v
                    .get("query")
                    .and_then(|x| x.as_object())
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                let headers = v.get("headers").and_then(|x| x.as_object()).map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                });
                let body = v.get("body").cloned();
                Some(openact_core::ActionInput {
                    path_params,
                    query,
                    headers,
                    body,
                    pagination: None,
                })
            } else {
                None
            };

            // Helpers to parse k=v and K=V
            fn parse_kv_pairs(
                pairs: &Vec<String>,
            ) -> anyhow::Result<std::collections::HashMap<String, serde_json::Value>> {
                let mut m = std::collections::HashMap::new();
                for item in pairs {
                    if let Some((k, v)) = item.split_once('=') {
                        // try parse value as JSON, fallback to string
                        let val = serde_json::from_str::<serde_json::Value>(v)
                            .unwrap_or(serde_json::Value::String(v.to_string()));
                        m.insert(k.to_string(), val);
                    } else {
                        return Err(anyhow::anyhow!(format!(
                            "invalid pair '{}', expected k=v",
                            item
                        )));
                    }
                }
                Ok(m)
            }
            fn parse_header_pairs(
                pairs: &Vec<String>,
            ) -> anyhow::Result<std::collections::HashMap<String, String>> {
                let mut m = std::collections::HashMap::new();
                for item in pairs {
                    if let Some((k, v)) = item.split_once('=') {
                        m.insert(k.to_string(), v.to_string());
                    } else {
                        return Err(anyhow::anyhow!(format!(
                            "invalid header '{}', expected K=V",
                            item
                        )));
                    }
                }
                Ok(m)
            }

            // Merge explicit flags
            let path_map = if !path.is_empty() {
                Some(parse_kv_pairs(&path)?)
            } else {
                from_file.as_ref().and_then(|i| i.path_params.clone())
            };
            let query_map = if !query.is_empty() {
                Some(parse_kv_pairs(&query)?)
            } else {
                from_file.as_ref().and_then(|i| i.query.clone())
            };
            let header_map = if !header.is_empty() {
                Some(parse_header_pairs(&header)?)
            } else {
                from_file.as_ref().and_then(|i| i.headers.clone())
            };
            let body_val = if let Some(text) = body_json {
                Some(serde_json::from_str::<serde_json::Value>(&text).map_err(|e| anyhow::anyhow!(format!("invalid --body-json: {}", e)))?)
            } else if let Some(spec) = body {
                if let Some(rest) = spec.strip_prefix('@') {
                    let t = std::fs::read_to_string(rest).map_err(|e| anyhow::anyhow!(format!("failed to read body file {}: {}", rest, e)))?;
                    Some(serde_json::from_str::<serde_json::Value>(&t).map_err(|e| anyhow::anyhow!(format!("invalid body file json: {}", e)))?)
                } else {
                    Some(serde_json::from_str::<serde_json::Value>(&spec).unwrap_or(serde_json::Value::String(spec)))
                }
            } else { from_file.as_ref().and_then(|i| i.body.clone()) };

            // Map --form/--file into multipart structure under body
            let mut body_val = body_val;
            if !form.is_empty() || !file.is_empty() {
                // Build {"_multipart": {"fields": {...}, "files": [{field, path}]}}
                let mut fields_map = serde_json::Map::new();
                for kv in &form {
                    if let Some((k, v)) = kv.split_once('=') { fields_map.insert(k.to_string(), serde_json::Value::String(v.to_string())); }
                }
                let mut files_arr: Vec<serde_json::Value> = Vec::new();
                for fv in &file {
                    if let Some((field, path)) = fv.split_once('=') {
                        let path = path.trim();
                        let p = if let Some(rest) = path.strip_prefix('@') { rest.to_string() } else { path.to_string() };
                        files_arr.push(serde_json::json!({"field": field, "path": p}));
                    }
                }
                let mut mp = serde_json::Map::new();
                if !fields_map.is_empty() { mp.insert("fields".to_string(), serde_json::Value::Object(fields_map)); }
                if !files_arr.is_empty() { mp.insert("files".to_string(), serde_json::Value::Array(files_arr)); }
                let mut obj = serde_json::Map::new();
                obj.insert("_multipart".to_string(), serde_json::Value::Object(mp));
                body_val = Some(serde_json::Value::Object(obj));
            }

            let pagination = if all_pages || max_pages.is_some() || per_page.is_some() {
                Some(openact_core::PaginationOptions { all_pages, max_pages, per_page })
            } else { None };
            let input_opt = if path_map.is_some() || query_map.is_some() || header_map.is_some() || body_val.is_some() || pagination.is_some() { Some(openact_core::ActionInput { path_params: path_map, query: query_map, headers: header_map, body: body_val, pagination }) } else { None };

            match engine
                .run_action_by_trn_with_input(&tenant, &action_trn, &exec_trn, input_opt)
                .await
            {
                Ok(res) => {
                    // Optionally post-process response for download/stream
                    let mut adjusted_response = res.response_data.clone();
                    // Download non-JSON body to file
                    if let Some(ref file_path) = download_to {
                        if let Some(resp) = adjusted_response.as_mut() {
                            if let Some(http) = resp.get_mut("http") {
                                if let Some(obj) = http.as_object_mut() {
                                    if let Some(body) = obj.get("body") {
                                        if let Some(s) = body.as_str() {
                                            // write string body to file
                                            if let Err(e) = std::fs::write(file_path, s.as_bytes()) {
                                                eprintln!("warn: failed to write download_to file {}: {}", file_path, e);
                                            } else {
                                                obj.insert(
                                                    "body".to_string(),
                                                    serde_json::json!({"saved_to": file_path, "bytes": s.len()}),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if force_json {
                        // Extract trace/diagnostics when requested
                        let mut extra = serde_json::Map::new();
                        if trace {
                            if let Some(ref data) = adjusted_response {
                                if let Some(tr) = data.get("trace") { extra.insert("trace".to_string(), tr.clone()); }
                                if let Some(diag) = data.get("diagnostics") { extra.insert("diagnostics".to_string(), diag.clone()); }
                            }
                        }
                        if retry_summary || trace {
                            if let Some(ref data) = adjusted_response {
                                if let Some(retry) = data.get("retry") { extra.insert("retry".to_string(), retry.clone()); }
                            }
                        }
                        let mut data_obj = serde_json::json!({
                            "status": res.status,
                            "response": adjusted_response,
                            "error": res.error_message,
                            "status_code": res.status_code,
                            "duration_ms": res.duration_ms,
                            "exec_trn": exec_trn,
                            "action_trn": action_trn,
                        });
                        if !extra.is_empty() {
                            if let Some(obj) = data_obj.as_object_mut() { obj.extend(extra); }
                        }
                        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "ok": true, "data": data_obj }))?);
                    } else {
                        println!("📊 执行状态: {:?}", res.status);
                        if let Some(ref data) = adjusted_response {
                            // If stream requested and body is NDJSON array, print each element
                            if stream {
                                if let Some(arr) = data.get("http").and_then(|h| h.get("body")).and_then(|b| b.as_array()) {
                                    for item in arr { println!("{}", item); }
                                } else {
                                    println!("📄 响应数据: {}", data);
                                }
                            } else {
                                println!("📄 响应数据: {}", data);
                            }
                            if retry_summary || trace {
                                if let Some(retry) = data.get("retry") { eprintln!("🔁 retry: {}", retry); }
                            }
                            if trace {
                                if let Some(tr) = data.get("trace") { eprintln!("🔎 trace: {}", tr); }
                                if let Some(diag) = data.get("diagnostics") { eprintln!("🩺 diagnostics: {}", diag); }
                            }
                        }
                        if let Some(ref error) = res.error_message { println!("❌ 错误信息: {}", error); }
                        println!("⏱️  执行时长: {}ms", res.duration_ms.unwrap_or(0));
                        if let Some(code) = res.status_code { println!("🔢 状态码: {}", code); }
                    }
                    if let Some(file) = save {
                        let mut doc = serde_json::json!({
                            "status": res.status,
                            "response": adjusted_response,
                            "error": res.error_message,
                            "status_code": res.status_code,
                            "duration_ms": res.duration_ms,
                            "exec_trn": exec_trn,
                            "action_trn": action_trn,
                        });
                        if trace {
                            if let Some(ref data) = adjusted_response {
                                if let Some(tr) = data.get("trace") {
                                    if let Some(obj) = doc.as_object_mut() { obj.insert("trace".to_string(), tr.clone()); }
                                }
                                if let Some(diag) = data.get("diagnostics") {
                                    if let Some(obj) = doc.as_object_mut() { obj.insert("diagnostics".to_string(), diag.clone()); }
                                }
                            }
                        }
                        if retry_summary || trace {
                            if let Some(ref data) = adjusted_response {
                                if let Some(retry) = data.get("retry") {
                                    if let Some(obj) = doc.as_object_mut() { obj.insert("retry".to_string(), retry.clone()); }
                                }
                            }
                        }
                        std::fs::write(file, serde_json::to_string_pretty(&doc)?)?;
                    }
                }
                Err(e) => {
                    if force_json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "ok": false,
                                "error": {"code": "RunFailed", "message": e.to_string()},
                                "meta": {"exec_trn": exec_trn, "action_trn": action_trn, "tenant": tenant}
                            }))?
                        );
                    } else {
                        println!("❌ 执行失败: {}", e);
                    }
                    // propagate non-zero exit
                    return Err(anyhow::anyhow!(e));
                }
            }
        }
        Commands::ActionRegister {
            tenant,
            provider,
            name,
            trn,
            yaml_path,
        } => {
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            let p = std::path::Path::new(&yaml_path);
            let action = registry
                .register_from_yaml(&tenant, &provider, &name, &trn, p)
                .await?;
            if !json_only {
                println!(
                    "registered: {} -> {} {} {}",
                    action.trn, tenant, provider, name
                );
            }
        }
        Commands::ActionDelete { trn } => {
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            let ok = registry.delete_by_trn(&trn).await?;
            if !json_only {
                println!("{}", if ok { "deleted" } else { "not found" });
            }
        }
        Commands::ActionInspect { trn, output } => {
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            let a = registry.get_by_trn(&trn).await?;
            let force_json = json_only || output == "json";
            if force_json {
                let data = serde_json::json!({
                    "trn": a.trn,
                    "tenant": a.tenant,
                    "provider": a.provider,
                    "name": a.name,
                    "active": a.is_active,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"ok": true, "data": data}))?
                );
            } else {
                println!(
                    "trn={} tenant={} provider={} name={} active={}",
                    a.trn, a.tenant, a.provider, a.name, a.is_active
                );
            }
        }
        Commands::ActionUpdate { trn, yaml_path } => {
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            let p = std::path::Path::new(&yaml_path);
            let a = registry.update_from_yaml(&trn, p).await?;
            if !json_only {
                println!("updated: {}", a.trn);
            }
        }
        Commands::ActionExport { trn } => {
            let registry = ActionRegistry::new(ctx.db.pool().clone());
            let spec = registry.export_spec_by_trn(&trn).await?;
            if !json_only {
                println!("{}", spec);
            }
        }
        Commands::Doctor {
            dsl,
            port_start,
            port_end,
        } => {
            if !json_only {
                println!("== OpenAct Doctor ==");
                // Env checks
                let db_url_res = std::env::var("OPENACT_DATABASE_URL")
                    .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"));
                let db_url_str = db_url_res
                    .clone()
                    .unwrap_or_else(|_| "(missing)".to_string());
                println!("DB URL: {}", db_url_str);
                if db_url_res.is_err() {
                    let cwd = std::env::current_dir().ok();
                    if let Some(p) = cwd {
                        println!("Suggestion: export OPENACT_DATABASE_URL=sqlite:{}/manifest/data/openact.db", p.display());
                    }
                }
                let master = std::env::var("OPENACT_MASTER_KEY")
                    .or_else(|_| std::env::var("AUTHFLOW_MASTER_KEY"));
                println!(
                    "Master Key: {}",
                    if master.is_ok() { "(set)" } else { "(not set)" }
                );
                if master.is_err() {
                    println!("Suggestion: export OPENACT_MASTER_KEY=your-32-bytes-key");
                }

                // DB connectivity
                let pool = ctx.db.pool().clone();
                match sqlx::query("SELECT 1").fetch_one(&pool).await {
                    Ok(_) => println!("DB connectivity: OK"),
                    Err(e) => println!("DB connectivity: FAIL ({})", e),
                }

                // Port availability
                let mut avail = Vec::new();
                for p in port_start..=port_end {
                    if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() {
                        avail.push(p);
                    }
                    if avail.len() >= 3 {
                        break;
                    }
                }
                if avail.is_empty() {
                    println!("Ports {}-{}: no free ports", port_start, port_end);
                    println!("Suggestion: choose a different range via --port-start/--port-end, or pass --redirect to auth-begin/auth-login");
                } else {
                    println!("Free ports (sample): {:?}", avail);
                }

                // Secrets mapping (optional DSL)
                if let Some(path) = dsl {
                    use openact_core::AuthOrchestrator;
                    use std::path::Path;
                    let orch = AuthOrchestrator::new(ctx.db.pool().clone());
                    let p = Path::new(&path);
                    // reuse orchestrator's secret extractor via temp functions by calling begin and catching error
                    let res = orch
                        .begin_oauth_from_config(
                            "doctor",
                            p,
                            Some("OAuth"),
                            Some("http://localhost:8080/oauth/callback"),
                            Some("user:email"),
                        )
                        .await;
                    match res {
                        Ok((url, _)) => {
                            println!("DSL parse: OK (authorize_url sample: {})", url);
                        }
                        Err(e) => {
                            let msg = format!("{}", e);
                            if msg.contains("missing required secrets") {
                                println!("Secrets: MISSING -> {}", msg);
                                // derive keys from message: missing required secrets: [k1, k2]
                                if let (Some(s), Some(eidx)) = (msg.find('['), msg.find(']')) {
                                    if eidx > s {
                                        let keys_str = &msg[s + 1..eidx];
                                        let keys: Vec<String> = keys_str
                                            .split(',')
                                            .map(|t| t.trim().trim_matches('"').to_string())
                                            .filter(|k| !k.is_empty())
                                            .collect();
                                        if !keys.is_empty() {
                                            println!("Suggestions (env):");
                                            for k in &keys {
                                                println!(
                                                    "  export {}=<value>",
                                                    k.replace('-', "_").to_uppercase()
                                                );
                                            }
                                            println!("Suggestion (file): set OPENACT_SECRETS_FILE to a json/yaml containing:");
                                            let sample: serde_json::Value = serde_json::json!(keys
                                                .iter()
                                                .map(|k| (k.clone(), "<value>".into()))
                                                .collect::<serde_json::Map<_, _>>());
                                            println!(
                                                "{}",
                                                serde_json::to_string_pretty(&sample)
                                                    .unwrap_or("{}".to_string())
                                            );
                                        }
                                    }
                                }
                            } else {
                                println!("DSL check: FAIL -> {}", msg);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
