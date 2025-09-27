use crate::app::service::OpenActService;
use crate::interface::dto::AdhocExecuteRequestDto;
use crate::models::common::RetryPolicy;
use crate::models::{ConnectionConfig, TaskConfig};
use crate::store::ConnectionStore;
use crate::store::{DatabaseManager, StorageService};
use crate::templates::TemplateInputs;
use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "openact", version, about = "OpenAct CLI")]
pub struct Cli {
    #[arg(long, global = true)]
    pub db_url: Option<String>,

    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,

    /// Use HTTP server mode instead of local execution (e.g. http://127.0.0.1:8080)
    #[arg(long, global = true)]
    pub server: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

// Removed unused execute_via_server helper (was for proxying via server mode)

#[derive(Args, Debug, Default)]
pub struct ExecuteOverrides {
    /// Override HTTP method
    #[arg(long)]
    pub method: Option<String>,
    /// Override endpoint URL
    #[arg(long)]
    pub endpoint: Option<String>,
    /// Add or override headers: key:value (repeatable)
    #[arg(long = "header")]
    pub headers: Vec<String>,
    /// Add or override query params: key=value (repeatable)
    #[arg(long = "query")]
    pub queries: Vec<String>,
    /// Provide request body (JSON string or @file)
    #[arg(long)]
    pub body: Option<String>,
    /// Output control: status-only, headers-only, body-only
    #[arg(long, default_value = "")]
    pub output: String,
    /// Override maximum retry attempts (0-10)
    #[arg(long)]
    pub max_retries: Option<u32>,
    /// Override base delay in milliseconds (10-10000)
    #[arg(long)]
    pub retry_delay_ms: Option<u64>,
    /// Override retry policy: aggressive|conservative|custom
    #[arg(long)]
    pub retry_policy: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute a task by TRN
    Execute {
        /// Task TRN
        task_trn: String,
        #[command(flatten)]
        overrides: ExecuteOverrides,
    },
    /// Execute ad-hoc action using existing connection
    ExecuteAdhoc {
        /// Connection TRN to use for authentication
        #[arg(long)]
        connection_trn: String,
        /// HTTP method (GET, POST, PUT, DELETE, etc.)
        #[arg(long)]
        method: String,
        /// API endpoint URL
        #[arg(long)]
        endpoint: String,
        /// Optional headers JSON (compat): {"key": ["value1", "value2"]}
        #[arg(long = "headers-json")]
        headers: Option<String>,
        /// Optional header entries: key:value (repeatable)
        #[arg(long = "header")]
        headers_kv: Vec<String>,
        /// Optional query parameters JSON (compat): {"key": ["value1", "value2"]}
        #[arg(long = "query-json")]
        query: Option<String>,
        /// Optional query entries: key=value (repeatable)
        #[arg(long = "query")]
        queries: Vec<String>,
        /// Optional request body (JSON format)
        #[arg(long)]
        body: Option<String>,
        /// Optional retry policy JSON
        #[arg(long)]
        retry_policy: Option<String>,
        /// Optional access token to override Authorization header for this call
        #[arg(long)]
        access_token: Option<String>,
    },
    /// Manage connections
    Connection {
        #[command(subcommand)]
        cmd: ConnectionCmd,
    },
    /// Manage tasks
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// Import/export configurations
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// System operations
    System {
        #[command(subcommand)]
        cmd: SystemCmd,
    },
    /// OAuth operations
    Oauth {
        #[command(subcommand)]
        cmd: OauthCmd,
    },
    /// Provider template operations
    Templates {
        #[command(subcommand)]
        cmd: TemplatesCmd,
    },
    /// One-click connect: create -> authorize/bind -> test
    Connect {
        /// Provider name (e.g., github, slack)
        #[arg(long)]
        provider: String,
        /// Template name (e.g., oauth2, api_key)
        #[arg(long)]
        template: String,
        /// Tenant identifier
        #[arg(long)]
        tenant: String,
        /// Connection name
        #[arg(long)]
        name: String,
        /// Secrets file (JSON) containing provider credentials
        #[arg(long)]
        secrets_file: Option<PathBuf>,
        /// Input parameters file (JSON) for customization
        #[arg(long)]
        inputs_file: Option<PathBuf>,
        /// Override parameters file (JSON) for explicit field overrides
        #[arg(long)]
        overrides_file: Option<PathBuf>,
        /// For OAuth2 AC: existing auth connection TRN to bind
        #[arg(long)]
        auth_trn: Option<String>,
        /// Optional test endpoint (defaults to https://httpbin.org/get)
        #[arg(long)]
        endpoint: Option<String>,
        /// Optional DSL file for AC start (YAML). When set, server mode uses AC flow
        #[arg(long)]
        dsl_file: Option<PathBuf>,
        /// Poll interval seconds for AC status when --server
        #[arg(long, default_value_t = 1)]
        poll_interval_secs: u64,
        /// Poll timeout seconds for AC status when --server
        #[arg(long, default_value_t = 30)]
        poll_timeout_secs: u64,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConnectionCmd {
    /// Create or update a connection from file or STDIN
    Upsert {
        /// Input file (JSON/YAML). If omitted, read from STDIN
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Get a connection by TRN
    Get { trn: String },
    /// List connections
    List {
        #[arg(long)]
        auth_kind: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        offset: Option<i64>,
    },
    /// Test a connection by performing a simple GET to a provided endpoint
    Test {
        /// Connection TRN
        trn: String,
        /// Endpoint to test (defaults to provider-specific default if omitted)
        #[arg(long)]
        endpoint: Option<String>,
    },
    /// Show connection auth status
    Status {
        /// Connection TRN
        trn: String,
    },
    /// Delete a connection by TRN
    Delete {
        trn: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCmd {
    /// Create or update a task from file or STDIN
    Upsert {
        /// Input file (JSON/YAML). If omitted, read from STDIN
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Get a task by TRN
    Get { trn: String },
    /// List tasks
    List {
        #[arg(long)]
        connection_trn: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        offset: Option<i64>,
    },
    /// Delete a task by TRN
    Delete {
        trn: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Import connections and/or tasks from file(s)
    Import {
        #[arg(long)]
        connections: Option<PathBuf>,
        #[arg(long)]
        tasks: Option<PathBuf>,
    },
    /// Export all connections and tasks
    Export {
        /// Output format: json or yaml (default: json when --json, otherwise yaml)
        #[arg(long)]
        format: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SystemCmd {
    /// Show stats of storage
    Stats,
    /// Cleanup expired data (e.g., expired auth connections)
    Cleanup,
    /// Reset database schema (WARNING: This will delete all data)
    ResetDb {
        /// Confirm the operation by providing --yes flag
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TemplatesCmd {
    /// List available templates
    List {
        /// Filter by provider (optional)
        #[arg(long)]
        provider: Option<String>,
        /// Filter by template type: connection, task
        #[arg(long)]
        template_type: Option<String>,
    },
    /// Show template details
    Show {
        /// Provider name
        #[arg(long)]
        provider: String,
        /// Template type: connection or task
        #[arg(long)]
        template_type: String,
        /// Template name
        #[arg(long)]
        name: String,
    },
    /// Manage connection templates
    Connections {
        #[command(subcommand)]
        cmd: TemplateConnectionCmd,
    },
    /// Manage task templates
    Tasks {
        #[command(subcommand)]
        cmd: TemplateTaskCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum TemplateConnectionCmd {
    /// Create a connection from a template
    Create {
        /// Provider name (e.g., github, slack, google)
        #[arg(long)]
        provider: String,
        /// Template name (e.g., oauth2, api_key)
        #[arg(long)]
        template: String,
        /// Tenant identifier
        #[arg(long)]
        tenant: String,
        /// Connection name
        #[arg(long)]
        name: String,
        /// Secrets file (JSON) containing provider credentials
        #[arg(long)]
        secrets_file: Option<PathBuf>,
        /// Input parameters file (JSON) for customization
        #[arg(long)]
        inputs_file: Option<PathBuf>,
        /// Override parameters file (JSON) for explicit field overrides
        #[arg(long)]
        overrides_file: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TemplateTaskCmd {
    /// Create a task from a template
    Create {
        /// Provider name (e.g., github, slack, google)
        #[arg(long)]
        provider: String,
        /// Action name (e.g., get_user, list_repos)
        #[arg(long)]
        action: String,
        /// Tenant identifier
        #[arg(long)]
        tenant: String,
        /// Task name
        #[arg(long)]
        name: String,
        /// Connection TRN to use for this task
        #[arg(long)]
        connection_trn: String,
        /// Input parameters file (JSON) for customization
        #[arg(long)]
        inputs_file: Option<PathBuf>,
        /// Override parameters file (JSON) for explicit field overrides
        #[arg(long)]
        overrides_file: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum OauthCmd {
    /// Start Authorization Code flow (prints authorize_url/state/code_verifier)
    Start {
        /// DSL YAML file path
        #[arg(short, long)]
        dsl: std::path::PathBuf,
        /// Open authorize_url in system default browser
        #[arg(long, default_value_t = false)]
        open_browser: bool,
    },
    /// Resume with code/state
    Resume {
        /// DSL YAML file path
        #[arg(short, long)]
        dsl: std::path::PathBuf,
        /// run_id from start
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        code: String,
        #[arg(long)]
        state: String,
        /// Optionally bind the obtained auth to a connection TRN
        #[arg(long)]
        bind_connection: Option<String>,
    },
    /// Bind auth connection TRN to a connection TRN
    Bind {
        /// connection TRN
        connection_trn: String,
        /// auth connection TRN
        auth_trn: String,
    },
    /// OAuth 2.0 Device Code (RFC 8628) flow
    DeviceCode {
        /// Token endpoint URL
        #[arg(long)]
        token_url: String,
        /// Device authorization endpoint URL
        #[arg(long)]
        device_code_url: String,
        /// OAuth2 client_id
        #[arg(long)]
        client_id: String,
        /// OAuth2 client_secret (optional)
        #[arg(long)]
        client_secret: Option<String>,
        /// Scope (optional, space-separated)
        #[arg(long)]
        scope: Option<String>,
        /// Tenant for storing credentials
        #[arg(long)]
        tenant: String,
        /// Provider name for auth record (e.g., github)
        #[arg(long)]
        provider: String,
        /// User identifier used to build auth record TRN
        #[arg(long)]
        user_id: String,
        /// Optionally bind to a connection TRN after success
        #[arg(long)]
        bind_connection: Option<String>,
    },
}

pub async fn run(cli: Cli) -> Result<()> {
    // Initialize OpenAct service (prefer explicit db_url)
    let service = if let Some(db) = &cli.db_url {
        let manager = DatabaseManager::new(db).await?;
        let storage = std::sync::Arc::new(StorageService::new(manager));
        OpenActService::from_storage(storage)
    } else {
        OpenActService::from_env().await?
    };

    match &cli.command {
        Commands::Connect {
            provider,
            template,
            tenant,
            name,
            secrets_file,
            inputs_file,
            overrides_file,
            auth_trn,
            endpoint,
            dsl_file,
            poll_interval_secs,
            poll_timeout_secs,
        } => {
            // If --server provided, use server-side /connect flow for parity
            if let Some(base) = &cli.server {
                let mut body = serde_json::json!({
                    "provider": provider,
                    "template": template,
                    "tenant": tenant,
                    "name": name,
                    "mode": if dsl_file.is_some() { "ac" } else { "cc" },
                });
                // optional: endpoint hint for cc test
                if let Some(ep) = endpoint {
                    body["endpoint"] = serde_json::json!(ep);
                }
                // optional: dsl_yaml for AC
                if let Some(path) = dsl_file {
                    let s = std::fs::read_to_string(path)?;
                    body["dsl_yaml"] = serde_json::json!(s);
                }

                let url = format!("{}/api/v1/connect", base.trim_end_matches('/'));
                let resp = reqwest::Client::new().post(&url).json(&body).send().await?;
                let status = resp.status();
                let bytes = resp.bytes().await?;
                if !status.is_success() {
                    return Err(anyhow!(
                        "server error {}: {}",
                        status,
                        String::from_utf8_lossy(&bytes)
                    ));
                }
                let mut val: serde_json::Value = serde_json::from_slice(&bytes)?;
                // If AC, poll status until done (simple loop)
                if dsl_file.is_some() {
                    let run_id = val
                        .get("run_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if run_id.is_empty() {
                        return Err(anyhow!("missing run_id in server response"));
                    }
                    if !cli.json {
                        println!(
                            "authorize_url: {}",
                            val.get("authorize_url")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                        );
                        println!("run_id: {}", run_id);
                        println!("Polling status...");
                    }
                    let status_url = format!(
                        "{}/api/v1/connect/ac/status?run_id={}",
                        base.trim_end_matches('/'),
                        urlencoding::encode(&run_id)
                    );
                    // basic polling with configurable interval/timeout
                    let max_iters = (*poll_timeout_secs / *poll_interval_secs).max(1);
                    for _ in 0..max_iters {
                        let r = reqwest::Client::new().get(&status_url).send().await?;
                        if r.status().is_success() {
                            let s: serde_json::Value = r.json().await?;
                            if s.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                                val["ac_status"] = s.clone();
                                break;
                            } else if let Some(h) = s.get("next_hints").and_then(|v| v.as_array()) {
                                if !cli.json {
                                    println!("hints: {}", serde_json::to_string(h)?);
                                }
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(*poll_interval_secs))
                            .await;
                    }
                }
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&val)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&val)?);
                }
                return Ok(());
            }
            // Build template inputs
            let mut inputs = TemplateInputs::default();
            if let Some(path) = secrets_file {
                let content = std::fs::read_to_string(path)?;
                let secrets: std::collections::HashMap<String, String> =
                    parse_json_or_yaml(&content)?;
                inputs.secrets = secrets;
            }
            if let Some(path) = inputs_file {
                let content = std::fs::read_to_string(path)?;
                let input_params: std::collections::HashMap<String, serde_json::Value> =
                    parse_json_or_yaml(&content)?;
                inputs.inputs = input_params;
            }
            if let Some(path) = overrides_file {
                let content = std::fs::read_to_string(path)?;
                let override_params: std::collections::HashMap<String, serde_json::Value> =
                    parse_json_or_yaml(&content)?;
                inputs.overrides = override_params;
            }

            // Create connection from template
            let connection = service
                .instantiate_and_upsert_connection(provider, template, tenant, name, inputs)
                .await?;

            // Optional: bind AC auth_trn
            if let Some(auth) = auth_trn {
                let manager = service.database();
                let repo = manager.connection_repository();
                let mut conn = repo.get_by_trn(&connection.trn).await?.ok_or_else(|| {
                    anyhow!("connection not found after create: {}", connection.trn)
                })?;
                conn.auth_ref = Some(auth.clone());
                repo.upsert(&conn).await?;
                if !cli.json {
                    println!("bound: {} -> {}", conn.trn, auth);
                }
            }

            // If OAuth2 Client Credentials, proactively fetch token to validate setup
            match connection.authorization_type {
                crate::models::connection::AuthorizationType::OAuth2ClientCredentials => {
                    // Ignore errors but report outcome
                    let token_outcome = crate::oauth::runtime::get_cc_token(&connection.trn).await;
                    if !cli.json {
                        match token_outcome {
                            Ok(_) => println!("🔐 cc token acquired"),
                            Err(e) => println!("[warn] cc token acquisition failed: {}", e),
                        }
                    }
                }
                _ => {}
            }

            // Status check
            let status = service.connection_status(&connection.trn).await?;

            // Test
            let ep = endpoint
                .clone()
                .unwrap_or_else(|| "https://httpbin.org/get".to_string());
            let req = AdhocExecuteRequestDto {
                connection_trn: connection.trn.clone(),
                method: "GET".to_string(),
                endpoint: ep,
                headers: None,
                query: None,
                body: None,
                timeout_config: None,
                network_config: None,
                http_policy: None,
                response_policy: None,
                retry_policy: None,
            };
            let test_res = service.execute_adhoc(req).await;

            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "connection": connection,
                        "status": status,
                        "test": match test_res { Ok(ref r) => serde_json::json!({"status": r.status, "ok": r.status < 400 }), Err(ref e) => serde_json::json!({"error": e.to_string()}) }
                    }))?
                );
            } else {
                println!("✅ created: {}", connection.trn);
                if let Some(s) = &status {
                    println!("🔎 status: {}", s.status);
                }
                match &test_res {
                    Ok(r) => println!("🧪 test: {}", r.status),
                    Err(e) => println!("🧪 test failed: {}", e),
                }
                // Next-step hints based on status
                if let Some(s) = status {
                    match s.status.as_str() {
                        "unbound" => println!(
                            "Next: run `openact-cli oauth bind --connection-trn {} --auth-trn <auth_trn>`",
                            connection.trn
                        ),
                        "not_authorized" => {
                            println!("Next: re-authorize via OAuth flow, then bind the auth_trn")
                        }
                        "expired" => println!("Next: refresh token or re-authorize"),
                        "misconfigured" => {
                            println!("Next: fix auth parameters and re-run connect/test")
                        }
                        _ => {}
                    }
                }
            }
        }
        Commands::ExecuteAdhoc {
            connection_trn,
            method,
            endpoint,
            headers,
            query,
            body,
            retry_policy,
            headers_kv,
            queries,
            access_token,
        } => {
            // Parse optional JSON fields (compat)
            let mut headers_map: std::collections::HashMap<String, Vec<String>> =
                if let Some(h) = headers {
                    parse_json_or_yaml(h)?
                } else {
                    std::collections::HashMap::new()
                };
            // Merge key:value style headers
            for kv in headers_kv {
                if let Some((k, v)) = kv.split_once(':') {
                    headers_map.insert(k.trim().to_string(), vec![v.trim().to_string()]);
                }
            }

            let mut query_map: std::collections::HashMap<String, Vec<String>> =
                if let Some(q) = query {
                    parse_json_or_yaml(q)?
                } else {
                    std::collections::HashMap::new()
                };
            // Merge key=value style queries
            for kv in queries {
                if let Some((k, v)) = kv.split_once('=') {
                    query_map.insert(k.trim().to_string(), vec![v.trim().to_string()]);
                }
            }

            let parsed_body: Option<serde_json::Value> = if let Some(b) = body {
                Some(parse_json_or_yaml(b)?)
            } else {
                None
            };

            let parsed_retry_policy: Option<RetryPolicy> = if let Some(rp) = retry_policy {
                Some(parse_json_or_yaml(rp)?)
            } else {
                None
            };

            // Create ad-hoc request
            let mut req = AdhocExecuteRequestDto {
                connection_trn: connection_trn.clone(),
                method: method.clone(),
                endpoint: endpoint.clone(),
                headers: if headers_map.is_empty() {
                    None
                } else {
                    Some(headers_map)
                },
                query: if query_map.is_empty() {
                    None
                } else {
                    Some(query_map)
                },
                body: parsed_body,
                timeout_config: None,
                network_config: None,
                http_policy: None,
                response_policy: None,
                retry_policy: parsed_retry_policy,
            };

            // Access-token override: inject Authorization header directly
            if let Some(token) = access_token {
                let token_header = format!("Bearer {}", token.trim());
                let mut hdrs = req.headers.unwrap_or_default();
                hdrs.insert("Authorization".to_string(), vec![token_header]);
                req.headers = Some(hdrs);
            }

            // Execute ad-hoc request
            let result = service.execute_adhoc(req).await?;

            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "status": result.status, "headers": result.headers, "body": result.body
                    }))?
                );
            } else {
                println!("Status: {}", result.status);
                println!("Headers:");
                for (k, v) in result.headers.iter() {
                    println!("  {}: {}", k, v);
                }
                println!("Body:\n{}", serde_json::to_string_pretty(&result.body)?);
            }
        }

        Commands::Execute {
            task_trn,
            overrides,
        } => {
            let (conn, mut task) = service
                .get_execution_context(task_trn)
                .await?
                .ok_or_else(|| anyhow!("Task or connection not found: {}", task_trn))?;

            // Apply overrides (ConnectionWins 仍然最终在执行器内生效；这里只是临时覆盖 task)
            if let Some(m) = &overrides.method {
                task.method = m.clone();
            }
            if let Some(ep) = &overrides.endpoint {
                task.api_endpoint = ep.clone();
            }
            if !overrides.headers.is_empty() {
                let mut headers = task.headers.unwrap_or_default();
                for kv in &overrides.headers {
                    if let Some((k, v)) = kv.split_once(':') {
                        headers.insert(k.trim().to_string(), vec![v.trim().to_string()]);
                    }
                }
                task.headers = Some(headers);
            }
            if !overrides.queries.is_empty() {
                let mut qs = task.query_params.unwrap_or_default();
                for kv in &overrides.queries {
                    if let Some((k, v)) = kv.split_once('=') {
                        qs.insert(k.trim().to_string(), vec![v.trim().to_string()]);
                    }
                }
                task.query_params = Some(qs);
            }
            if let Some(body) = &overrides.body {
                let content = if let Some(path) = body.strip_prefix('@') {
                    std::fs::read_to_string(path)?
                } else {
                    body.clone()
                };
                let val = parse_json_or_yaml::<serde_json::Value>(&content)?;
                task.request_body = Some(val);
            }

            // 应用重试策略覆盖
            if let Some(retry_policy) = build_retry_policy_from_overrides(overrides)? {
                task.retry_policy = Some(retry_policy);
            }

            let executor = crate::executor::Executor::new();
            let result = executor.execute(&conn, &task).await?;

            match overrides.output.as_str() {
                "status-only" => println!("{}", result.status),
                "headers-only" => println!("{}", serde_json::to_string_pretty(&result.headers)?),
                "body-only" => println!("{}", serde_json::to_string_pretty(&result.body)?),
                _ => {
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "status": result.status,
                                "headers": result.headers,
                                "body": result.body,
                            }))?
                        );
                    } else {
                        println!("Status: {}", result.status);
                        println!("Headers:");
                        for (k, v) in result.headers.iter() {
                            println!("  {}: {}", k, v);
                        }
                        println!("Body:\n{}", serde_json::to_string_pretty(&result.body)?);
                    }
                }
            }
        }
        Commands::Connection { cmd } => match cmd {
            ConnectionCmd::Upsert { file } => {
                if let Some(base) = &cli.server {
                    let s = read_input(file.as_ref())?;
                    let cfg: ConnectionConfig = parse_json_or_yaml(&s)?;
                    let url = format!("{}/api/v1/connections", base.trim_end_matches('/'));
                    let resp = reqwest::Client::new().post(&url).json(&cfg).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    println!("upserted: {}", cfg.trn);
                    return Ok(());
                }
                let s = read_input(file.as_ref())?;
                let cfg: ConnectionConfig = parse_json_or_yaml(&s)?;
                service.upsert_connection(&cfg).await?;
                println!("upserted: {}", cfg.trn);
            }
            ConnectionCmd::Test { trn, endpoint } => {
                if let Some(base) = &cli.server {
                    // Use server endpoint
                    let url = format!(
                        "{}/api/v1/connections/{}/test",
                        base.trim_end_matches('/'),
                        urlencoding::encode(trn)
                    );
                    let body = serde_json::json!({
                        "endpoint": endpoint.clone().unwrap_or_else(|| "https://httpbin.org/get".to_string())
                    });
                    let resp = reqwest::Client::new().post(&url).json(&body).send().await?;
                    let status = resp.status();
                    let bytes = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&bytes)
                        ));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::from_slice::<
                                serde_json::Value,
                            >(&bytes)?)?
                        );
                    } else {
                        println!("{}", String::from_utf8_lossy(&bytes));
                    }
                } else {
                    // Local test via adhoc execution
                    let ep = endpoint
                        .clone()
                        .unwrap_or_else(|| "https://httpbin.org/get".to_string());
                    let req = AdhocExecuteRequestDto {
                        connection_trn: trn.clone(),
                        method: "GET".to_string(),
                        endpoint: ep,
                        headers: None,
                        query: None,
                        body: None,
                        timeout_config: None,
                        network_config: None,
                        http_policy: None,
                        response_policy: None,
                        retry_policy: None,
                    };
                    let result = service.execute_adhoc(req).await?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "status": result.status, "headers": result.headers, "body": result.body
                            }))?
                        );
                    } else {
                        println!("Status: {}", result.status);
                        println!("Headers:");
                        for (k, v) in result.headers.iter() {
                            println!("  {}: {}", k, v);
                        }
                        println!("Body:\n{}", serde_json::to_string_pretty(&result.body)?);
                    }
                }
            }
            ConnectionCmd::Status { trn } => {
                if let Some(base) = &cli.server {
                    let url = format!(
                        "{}/api/v1/connections/{}/status",
                        base.trim_end_matches('/'),
                        urlencoding::encode(trn)
                    );
                    let resp = reqwest::Client::new().get(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::from_slice::<
                                serde_json::Value,
                            >(&body)?)?
                        );
                    } else {
                        println!("{}", String::from_utf8_lossy(&body));
                    }
                } else {
                    let svc = &service;
                    match svc.connection_status(trn).await? {
                        Some(s) => {
                            if cli.json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&serde_json::to_value(&s)?)?
                                );
                            } else {
                                println!(
                                    "TRN: {}\nType: {:?}\nStatus: {}{}",
                                    s.trn,
                                    s.authorization_type,
                                    s.status,
                                    match s.seconds_to_expiry {
                                        Some(secs) if secs > 0 =>
                                            format!(" (expires in {}s)", secs),
                                        Some(_) => " (expired)".to_string(),
                                        None => "".to_string(),
                                    }
                                );
                                if let Some(msg) = &s.message {
                                    println!("Note: {}", msg);
                                }
                                // Next-step hints
                                match s.status.as_str() {
                                    "unbound" => println!(
                                        "Hint: run `openact-cli oauth bind --connection-trn <trn> --auth-trn <auth_trn>`"
                                    ),
                                    "not_authorized" => println!(
                                        "Hint: re-authorize via your OAuth flow and bind the new auth_trn"
                                    ),
                                    "expired" => println!(
                                        "Hint: retry auth flow or refresh token if supported"
                                    ),
                                    "misconfigured" => println!(
                                        "Hint: fix connection auth parameters, then `openact-cli connection test <trn>`"
                                    ),
                                    "not_issued" => println!(
                                        "Hint: execute once or run `openact-cli connect ...` to prefetch token"
                                    ),
                                    _ => {}
                                }
                            }
                        }
                        None => return Err(anyhow!("connection not found: {}", trn)),
                    }
                }
            }
            ConnectionCmd::Get { trn } => {
                if let Some(base) = &cli.server {
                    let url = format!(
                        "{}/api/v1/connections/{}",
                        base.trim_end_matches('/'),
                        urlencoding::encode(trn)
                    );
                    let resp = reqwest::Client::new().get(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::from_slice::<
                                serde_json::Value,
                            >(&body)?)?
                        );
                    } else {
                        println!("{}", String::from_utf8_lossy(&body));
                    }
                    return Ok(());
                }
                let found = service.get_connection(trn).await?;
                match found {
                    Some(c) => {
                        if cli.json {
                            println!("{}", serde_json::to_string_pretty(&c)?);
                        } else {
                            println!(
                                "TRN: {}\nname: {}\nauth_kind: {:?}",
                                c.trn, c.name, c.authorization_type
                            );
                        }
                    }
                    None => return Err(anyhow!("connection not found: {}", trn)),
                }
            }
            ConnectionCmd::List {
                auth_kind,
                limit,
                offset,
            } => {
                if let Some(base) = &cli.server {
                    let mut url = format!("{}/api/v1/connections", base.trim_end_matches('/'));
                    let mut first = true;
                    if let Some(k) = auth_kind {
                        url.push_str(if first { "?" } else { "&" });
                        first = false;
                        url.push_str("auth_type=");
                        url.push_str(&urlencoding::encode(k));
                    }
                    if let Some(v) = limit {
                        url.push_str(if first { "?" } else { "&" });
                        first = false;
                        url.push_str("limit=");
                        url.push_str(&v.to_string());
                    }
                    if let Some(v) = offset {
                        url.push_str(if first { "?" } else { "&" });
                        url.push_str("offset=");
                        url.push_str(&v.to_string());
                    }
                    let resp = reqwest::Client::new().get(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::from_slice::<
                                serde_json::Value,
                            >(&body)?)?
                        );
                    } else {
                        println!("{}", String::from_utf8_lossy(&body));
                    }
                    return Ok(());
                }
                let list = service
                    .list_connections(auth_kind.as_deref(), *limit, *offset)
                    .await?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for c in list {
                        println!("{}\t{:?}\t{}", c.trn, c.authorization_type, c.name);
                    }
                }
            }
            ConnectionCmd::Delete { trn, yes } => {
                if !*yes {
                    println!("use --yes to confirm delete");
                    return Ok(());
                }
                if let Some(base) = &cli.server {
                    let url = format!(
                        "{}/api/v1/connections/{}",
                        base.trim_end_matches('/'),
                        urlencoding::encode(trn)
                    );
                    let resp = reqwest::Client::new().delete(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if status.as_u16() == 204 {
                        println!("deleted: {}", trn);
                        return Ok(());
                    }
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    println!("deleted: {}", trn);
                    return Ok(());
                }
                let ok = service.delete_connection(trn).await?;
                if ok {
                    println!("deleted: {}", trn);
                } else {
                    println!("not found: {}", trn);
                }
            }
        },
        Commands::Task { cmd } => match cmd {
            TaskCmd::Upsert { file } => {
                if let Some(base) = &cli.server {
                    let s = read_input(file.as_ref())?;
                    let cfg: TaskConfig = parse_json_or_yaml(&s)?;
                    let url = format!("{}/api/v1/tasks", base.trim_end_matches('/'));
                    let resp = reqwest::Client::new().post(&url).json(&cfg).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    println!("upserted: {}", cfg.trn);
                    return Ok(());
                }
                let s = read_input(file.as_ref())?;
                let cfg: TaskConfig = parse_json_or_yaml(&s)?;
                service.upsert_task(&cfg).await?;
                println!("upserted: {}", cfg.trn);
            }
            TaskCmd::Get { trn } => {
                if let Some(base) = &cli.server {
                    let url = format!(
                        "{}/api/v1/tasks/{}",
                        base.trim_end_matches('/'),
                        urlencoding::encode(trn)
                    );
                    let resp = reqwest::Client::new().get(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::from_slice::<
                                serde_json::Value,
                            >(&body)?)?
                        );
                    } else {
                        println!("{}", String::from_utf8_lossy(&body));
                    }
                    return Ok(());
                }
                let found = service.get_task(trn).await?;
                match found {
                    Some(t) => {
                        if cli.json {
                            println!("{}", serde_json::to_string_pretty(&t)?);
                        } else {
                            println!(
                                "TRN: {}\nname: {}\nconnection: {}\nmethod: {}\nendpoint: {}",
                                t.trn, t.name, t.connection_trn, t.method, t.api_endpoint
                            );
                        }
                    }
                    None => return Err(anyhow!("task not found: {}", trn)),
                }
            }
            TaskCmd::List {
                connection_trn,
                limit,
                offset,
            } => {
                if let Some(base) = &cli.server {
                    let mut url = format!("{}/api/v1/tasks", base.trim_end_matches('/'));
                    let mut params = Vec::new();
                    if let Some(k) = connection_trn {
                        params.push(format!("connection_trn={}", urlencoding::encode(k)));
                    }
                    if let Some(v) = limit {
                        params.push(format!("limit={}", v));
                    }
                    if let Some(v) = offset {
                        params.push(format!("offset={}", v));
                    }
                    if !params.is_empty() {
                        url.push('?');
                        url.push_str(&params.join("&"));
                    }
                    let resp = reqwest::Client::new().get(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::from_slice::<
                                serde_json::Value,
                            >(&body)?)?
                        );
                    } else {
                        println!("{}", String::from_utf8_lossy(&body));
                    }
                    return Ok(());
                }
                let list = service
                    .list_tasks(connection_trn.as_deref(), *limit, *offset)
                    .await?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for t in list {
                        println!(
                            "{}\t{}\t{} {}",
                            t.trn, t.connection_trn, t.method, t.api_endpoint
                        );
                    }
                }
            }
            TaskCmd::Delete { trn, yes } => {
                if !*yes {
                    println!("use --yes to confirm delete");
                    return Ok(());
                }
                if let Some(base) = &cli.server {
                    let url = format!(
                        "{}/api/v1/tasks/{}",
                        base.trim_end_matches('/'),
                        urlencoding::encode(trn)
                    );
                    let resp = reqwest::Client::new().delete(&url).send().await?;
                    let status = resp.status();
                    let body = resp.bytes().await?;
                    if status.as_u16() == 204 {
                        println!("deleted: {}", trn);
                        return Ok(());
                    }
                    if !status.is_success() {
                        return Err(anyhow!(
                            "server error {}: {}",
                            status,
                            String::from_utf8_lossy(&body)
                        ));
                    }
                    println!("deleted: {}", trn);
                    return Ok(());
                }
                let ok = service.delete_task(trn).await?;
                if ok {
                    println!("deleted: {}", trn);
                } else {
                    println!("not found: {}", trn);
                }
            }
        },
        Commands::Config { cmd } => match cmd {
            ConfigCmd::Import { connections, tasks } => {
                if connections.is_none() && tasks.is_none() {
                    return Err(anyhow!("provide --connections and/or --tasks"));
                }
                let mut conns: Vec<ConnectionConfig> = Vec::new();
                let mut tsk: Vec<TaskConfig> = Vec::new();
                if let Some(p) = connections {
                    let s = std::fs::read_to_string(p)?;
                    conns = parse_json_or_yaml(&s)?;
                }
                if let Some(p) = tasks {
                    let s = std::fs::read_to_string(p)?;
                    tsk = parse_json_or_yaml(&s)?;
                }
                let (ic, it) = service.import_configurations(conns, tsk).await?;
                println!("imported: connections={} tasks={}", ic, it);
            }
            ConfigCmd::Export { format } => {
                let (conns, tasks) = service.export_configurations().await?;
                let fmt = format
                    .as_deref()
                    .unwrap_or(if cli.json { "json" } else { "yaml" });
                match fmt {
                    "json" => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "connections": conns,
                                "tasks": tasks
                            }))?
                        );
                    }
                    "yaml" => {
                        let obj = serde_json::json!({
                            "connections": conns,
                            "tasks": tasks
                        });
                        let yaml = serde_yaml::to_string(&obj)?;
                        print!("{}", yaml);
                    }
                    other => return Err(anyhow!("unsupported format: {}", other)),
                }
            }
        },
        Commands::System { cmd } => match cmd {
            SystemCmd::Stats => {
                let stats = service.get_stats().await?;
                let cache = service.get_cache_stats().await;
                let cp = crate::executor::client_pool::get_stats();
                if cli.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "storage": stats,
                            "caches": cache,
                            "client_pool": {
                                "hits": cp.hits,
                                "builds": cp.builds,
                                "evictions": cp.evictions,
                                "size": cp.size,
                                "capacity": cp.capacity
                            }
                        }))?
                    );
                } else {
                    println!("connections: {}", stats.total_connections);
                    println!("tasks: {}", stats.total_tasks);
                    println!("auth_connections: {}", stats.total_auth_connections);
                    println!("api_key: {}", stats.api_key_connections);
                    println!("basic: {}", stats.basic_connections);
                    println!("oauth2_cc: {}", stats.oauth2_cc_connections);
                    println!("oauth2_ac: {}", stats.oauth2_ac_connections);
                    println!(
                        "cache: exec_hit_rate={:.2}% conn_hit_rate={:.2}% task_hit_rate={:.2}%",
                        cache.exec_hit_rate * 100.0,
                        cache.conn_hit_rate * 100.0,
                        cache.task_hit_rate * 100.0
                    );
                    println!(
                        "client_pool: hits={} builds={} evictions={} size={} capacity={}",
                        cp.hits, cp.builds, cp.evictions, cp.size, cp.capacity
                    );
                }
            }
            SystemCmd::Cleanup => {
                let r = service.cleanup().await?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&r)?);
                } else {
                    println!("expired_auth_connections: {}", r.expired_auth_connections);
                }
            }
            SystemCmd::ResetDb { yes } => {
                if !yes {
                    eprintln!("⚠️  WARNING: This will delete ALL data in the database!");
                    eprintln!(
                        "   This includes all connections, tasks, auth tokens, and execution history."
                    );
                    eprintln!("   This operation cannot be undone.");
                    eprintln!();
                    eprintln!("   To confirm, run:");
                    eprintln!("   openact-cli system reset-db --yes");
                    std::process::exit(1);
                }

                // Get database path from environment or default
                let db_url = cli
                    .db_url
                    .clone()
                    .or_else(|| std::env::var("OPENACT_DB_URL").ok())
                    .unwrap_or_else(|| "sqlite:data/openact.db".to_string());

                if db_url.starts_with("sqlite:") {
                    let db_path = db_url.strip_prefix("sqlite:").unwrap();
                    let path = std::path::Path::new(db_path);

                    if path.exists() {
                        std::fs::remove_file(path).map_err(|e| {
                            anyhow::anyhow!("Failed to delete database file '{}': {}", db_path, e)
                        })?;
                        println!("✅ Database file deleted: {}", db_path);
                    } else {
                        println!("ℹ️  Database file does not exist: {}", db_path);
                    }

                    // Create parent directory if needed
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            anyhow::anyhow!("Failed to create database directory: {}", e)
                        })?;
                    }

                    println!("🔄 Recreating database with fresh schema...");

                    // Initialize a new database manager to trigger migration
                    let new_manager = crate::store::DatabaseManager::new(&db_url).await?;
                    drop(new_manager); // Close the connection

                    println!("✅ Database reset complete. Fresh schema applied.");
                } else {
                    return Err(anyhow::anyhow!(
                        "Database reset is only supported for SQLite databases. Current: {}",
                        db_url
                    ));
                }
            }
        },
        Commands::Oauth { cmd } => {
            match cmd {
                OauthCmd::Start { dsl, open_browser } => {
                    let dsl_path = dsl.clone();
                    let yaml = std::fs::read_to_string(dsl)?;
                    let wf: stepflow_dsl::WorkflowDSL = serde_yaml::from_str(&yaml)?;
                    let run_store = crate::store::MemoryRunStore::default();
                    let router = crate::authflow::actions::DefaultRouter; // not Default
                    let res = crate::authflow::workflow::start_obtain(
                        &wf,
                        &router,
                        &run_store,
                        serde_json::json!({}),
                    )?;
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
        }
        Commands::Templates { cmd } => match cmd {
            TemplatesCmd::List {
                provider,
                template_type,
            } => {
                let templates_dir = std::env::var("OPENACT_TEMPLATES_DIR")
                    .unwrap_or_else(|_| "templates".to_string());
                let loader = crate::templates::TemplateLoader::new(templates_dir);

                let templates =
                    loader.list_templates(provider.as_deref(), template_type.as_deref())?;

                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&templates)?);
                } else {
                    if templates.is_empty() {
                        println!("No templates found.");
                    } else {
                        println!("📋 Available Templates:");
                        println!();

                        let mut current_provider = "";
                        for template in &templates {
                            if template.provider != current_provider {
                                if !current_provider.is_empty() {
                                    println!();
                                }
                                println!("🔧 {}/", template.provider);
                                current_provider = &template.provider;
                            }

                            let type_icon = match template.template_type.as_str() {
                                "connection" => "🔗",
                                "task" => "⚡",
                                _ => "📄",
                            };

                            print!(
                                "  {} {} ({})",
                                type_icon, template.name, template.template_type
                            );
                            if let Some(action) = &template.action {
                                print!(" - {}", action);
                            }
                            println!();
                            println!("     📝 {}", template.metadata.description);
                        }
                    }
                }
            }
            TemplatesCmd::Show {
                provider,
                template_type,
                name,
            } => {
                let templates_dir = std::env::var("OPENACT_TEMPLATES_DIR")
                    .unwrap_or_else(|_| "templates".to_string());
                let loader = crate::templates::TemplateLoader::new(templates_dir);

                let template_details = loader.show_template(provider, template_type, name)?;

                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&template_details)?);
                } else {
                    // Extract metadata for nice display
                    if let Some(metadata) = template_details.get("metadata") {
                        println!("📄 Template Details:");
                        println!("  Provider: {}", provider);
                        println!("  Type: {}", template_type);
                        println!("  Name: {}", name);
                        if let Some(desc) = metadata.get("description").and_then(|v| v.as_str()) {
                            println!("  Description: {}", desc);
                        }
                        if let Some(version) = template_details
                            .get("template_version")
                            .and_then(|v| v.as_str())
                        {
                            println!("  Version: {}", version);
                        }
                        if let Some(docs) = metadata.get("documentation").and_then(|v| v.as_str()) {
                            println!("  Documentation: {}", docs);
                        }
                        if let Some(secrets) =
                            metadata.get("required_secrets").and_then(|v| v.as_array())
                        {
                            if !secrets.is_empty() {
                                println!("  Required Secrets:");
                                for secret in secrets {
                                    if let Some(s) = secret.as_str() {
                                        println!("    - {}", s);
                                    }
                                }
                            }
                        }
                        println!();
                        println!("📋 Full Template:");
                        println!("{}", serde_json::to_string_pretty(&template_details)?);
                    } else {
                        println!("{}", serde_json::to_string_pretty(&template_details)?);
                    }
                }
            }
            TemplatesCmd::Connections { cmd } => match cmd {
                TemplateConnectionCmd::Create {
                    provider,
                    template,
                    tenant,
                    name,
                    secrets_file,
                    inputs_file,
                    overrides_file,
                } => {
                    // Build template inputs
                    let mut inputs = TemplateInputs::default();

                    // Load secrets
                    if let Some(path) = secrets_file {
                        let content = std::fs::read_to_string(path)?;
                        let secrets: std::collections::HashMap<String, String> =
                            parse_json_or_yaml(&content)?;
                        inputs.secrets = secrets;
                    }

                    // Load inputs
                    if let Some(path) = inputs_file {
                        let content = std::fs::read_to_string(path)?;
                        let input_params: std::collections::HashMap<String, serde_json::Value> =
                            parse_json_or_yaml(&content)?;
                        inputs.inputs = input_params;
                    }

                    // Load overrides
                    if let Some(path) = overrides_file {
                        let content = std::fs::read_to_string(path)?;
                        let override_params: std::collections::HashMap<String, serde_json::Value> =
                            parse_json_or_yaml(&content)?;
                        inputs.overrides = override_params;
                    }

                    // Create connection from template
                    let connection = service
                        .instantiate_and_upsert_connection(provider, template, tenant, name, inputs)
                        .await?;

                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&connection)?);
                    } else {
                        println!("✅ Connection created from template:");
                        println!("  TRN: {}", connection.trn);
                        println!("  Provider: {}", provider);
                        println!("  Template: {}", template);
                        println!("  Auth Type: {:?}", connection.authorization_type);
                    }
                }
            },
            TemplatesCmd::Tasks { cmd } => match cmd {
                TemplateTaskCmd::Create {
                    provider,
                    action,
                    tenant,
                    name,
                    connection_trn,
                    inputs_file,
                    overrides_file,
                } => {
                    // Build template inputs
                    let mut inputs = TemplateInputs::default();

                    // Load inputs
                    if let Some(path) = inputs_file {
                        let content = std::fs::read_to_string(path)?;
                        let input_params: std::collections::HashMap<String, serde_json::Value> =
                            parse_json_or_yaml(&content)?;
                        inputs.inputs = input_params;
                    }

                    // Load overrides
                    if let Some(path) = overrides_file {
                        let content = std::fs::read_to_string(path)?;
                        let override_params: std::collections::HashMap<String, serde_json::Value> =
                            parse_json_or_yaml(&content)?;
                        inputs.overrides = override_params;
                    }

                    // Create task from template
                    let task = service
                        .instantiate_and_upsert_task(
                            provider,
                            action,
                            tenant,
                            name,
                            connection_trn,
                            inputs,
                        )
                        .await?;

                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&task)?);
                    } else {
                        println!("✅ Task created from template:");
                        println!("  TRN: {}", task.trn);
                        println!("  Provider: {}", provider);
                        println!("  Action: {}", action);
                        println!("  Connection: {}", task.connection_trn);
                        println!("  Endpoint: {} {}", task.method, task.api_endpoint);
                    }
                }
            },
        },
    }

    Ok(())
}

fn read_input(path: Option<&PathBuf>) -> Result<String> {
    if let Some(p) = path {
        return Ok(std::fs::read_to_string(p)?);
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn parse_json_or_yaml<T: serde::de::DeserializeOwned>(s: &str) -> Result<T> {
    let trimmed = s.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        Ok(serde_json::from_str(trimmed)?)
    } else {
        Ok(serde_yaml::from_str(trimmed)?)
    }
}

/// 从CLI参数构建重试策略
fn build_retry_policy_from_overrides(overrides: &ExecuteOverrides) -> Result<Option<RetryPolicy>> {
    // 如果没有任何重试相关参数，返回None
    if overrides.max_retries.is_none()
        && overrides.retry_delay_ms.is_none()
        && overrides.retry_policy.is_none()
    {
        return Ok(None);
    }

    // 基础策略
    let mut policy = match overrides.retry_policy.as_deref() {
        Some("aggressive") => RetryPolicy::aggressive(),
        Some("conservative") => RetryPolicy::conservative(),
        _ => RetryPolicy::default(),
    };

    // 应用覆盖参数
    if let Some(max_retries) = overrides.max_retries {
        if max_retries > 10 {
            return Err(anyhow!("max_retries cannot exceed 10"));
        }
        policy.max_retries = max_retries;
    }

    if let Some(delay_ms) = overrides.retry_delay_ms {
        if delay_ms < 10 || delay_ms > 10000 {
            return Err(anyhow!("retry_delay_ms must be between 10 and 10000"));
        }
        policy.base_delay_ms = delay_ms;
    }

    Ok(Some(policy))
}

#[cfg(test)]
mod cli_integration_tests {
    use super::*;
    use httpmock::prelude::*;

    async fn make_service() -> OpenActService {
        // Use in-memory for isolation and avoid FS permissions in CI
        let db = DatabaseManager::new("sqlite::memory:").await.unwrap();
        // Set a test encryption key for StorageService
        unsafe {
            std::env::set_var(
                "OPENACT_MASTER_KEY",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            );
        }
        let storage = std::sync::Arc::new(StorageService::new(db));
        OpenActService::from_storage(storage)
    }

    #[tokio::test]
    async fn cli_execute_adhoc_authorization_override() {
        let svc = make_service().await;
        // OAuth2 CC connection
        let mut conn = ConnectionConfig::new(
            "trn:openact:default:connection/cli-cc".to_string(),
            "cli-cc".to_string(),
            crate::models::AuthorizationType::OAuth2ClientCredentials,
        );
        // Token endpoint mock (should not be hit)
        let token_server = MockServer::start();
        let token_mock = token_server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token":"T","expires_in":3600}));
        });
        conn.auth_parameters.oauth_parameters = Some(crate::models::OAuth2Parameters {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            token_url: token_server.url("/token"),
            scope: Some("r:all".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        svc.upsert_connection(&conn).await.unwrap();

        // API endpoint requiring our override header
        let api = MockServer::start();
        let protected = api.mock(|when, then| {
            when.method(GET)
                .path("/p")
                .header("authorization", "Bearer OVRT");
            then.status(200).json_body(json!({"ok": true}));
        });

        // Build DTO as CLI would
        let req = AdhocExecuteRequestDto {
            connection_trn: conn.trn,
            method: "GET".to_string(),
            endpoint: format!("{}{}", api.base_url(), "/p"),
            headers: Some(std::collections::HashMap::from([(
                "Authorization".to_string(),
                vec!["Bearer OVRT".to_string()],
            )])),
            query: None,
            body: None,
            timeout_config: None,
            network_config: None,
            http_policy: None,
            response_policy: None,
            retry_policy: None,
        };
        let out = svc.execute_adhoc(req).await.unwrap();
        assert_eq!(out.status, 200);
        protected.assert();
        assert_eq!(token_mock.hits(), 0);
    }

    #[tokio::test]
    async fn cli_execute_adhoc_connection_wins() {
        let svc = make_service().await;
        // API Key connection with defaults
        let mut conn = ConnectionConfig::new(
            "trn:openact:default:connection/cli-merge".to_string(),
            "cli-merge".to_string(),
            crate::models::AuthorizationType::ApiKey,
        );
        conn.auth_parameters.api_key_auth_parameters = Some(crate::models::ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "k".to_string(),
        });
        conn.invocation_http_parameters = Some(crate::models::InvocationHttpParameters {
            header_parameters: vec![
                crate::models::HttpParameter {
                    key: "X-API-Version".to_string(),
                    value: "v2".to_string(),
                },
                crate::models::HttpParameter {
                    key: "Content-Type".to_string(),
                    value: "application/json; charset=utf-8".to_string(),
                },
            ],
            query_string_parameters: vec![crate::models::HttpParameter {
                key: "limit".to_string(),
                value: "100".to_string(),
            }],
            body_parameters: vec![crate::models::HttpParameter {
                key: "source".to_string(),
                value: "connection".to_string(),
            }],
        });
        svc.upsert_connection(&conn).await.unwrap();

        let api = MockServer::start();
        let m = api.mock(|when, then| {
            when.method(POST)
                .path("/m")
                .query_param("limit", "100")
                .header("X-API-Version", "v2")
                .header("Content-Type", "application/json; charset=utf-8");
            then.status(200).json_body(json!({"ok": true}));
        });

        let req = AdhocExecuteRequestDto {
            connection_trn: conn.trn,
            method: "POST".to_string(),
            endpoint: format!("{}{}", api.base_url(), "/m"),
            headers: Some(std::collections::HashMap::from([(
                "Content-Type".to_string(),
                vec!["application/json".to_string()],
            )])),
            query: Some(std::collections::HashMap::from([(
                "limit".to_string(),
                vec!["50".to_string()],
            )])),
            body: Some(json!({"existing":"value"})),
            timeout_config: None,
            network_config: None,
            http_policy: None,
            response_policy: None,
            retry_policy: None,
        };
        let out = svc.execute_adhoc(req).await.unwrap();
        assert_eq!(out.status, 200);
        m.assert();
    }

    #[tokio::test]
    #[ignore]
    async fn cli_connect_cc_local_success() {
        let svc = make_service().await;
        // Inject storage for oauth runtime/global paths
        let storage = svc.storage();
        crate::store::service::set_global_storage_service_for_tests(storage.clone()).await;

        // Mock token endpoint returns token
        let token_server = MockServer::start();
        let _token = token_server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token":"TOK","expires_in":3600}));
        });

        // Create CC connection
        let mut conn = ConnectionConfig::new(
            "trn:openact:default:connection/cli-cc-local".to_string(),
            "cli-cc-local".to_string(),
            crate::models::AuthorizationType::OAuth2ClientCredentials,
        );
        conn.auth_parameters.oauth_parameters = Some(crate::models::OAuth2Parameters {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            token_url: token_server.url("/token"),
            scope: Some("scope".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        svc.upsert_connection(&conn).await.unwrap();

        // Acquire token via runtime
        let out = crate::oauth::runtime::get_cc_token(&conn.trn)
            .await
            .unwrap();
        match out {
            crate::oauth::runtime::RefreshOutcome::Fresh(info)
            | crate::oauth::runtime::RefreshOutcome::Reused(info)
            | crate::oauth::runtime::RefreshOutcome::Refreshed(info) => {
                assert_eq!(info.access_token, "TOK");
            }
        }

        // Status should be ready
        let status = svc.connection_status(&conn.trn).await.unwrap().unwrap();
        assert!(status.status == "ready" || status.status == "expiring_soon");

        // Test call to protected endpoint using injected auth
        let api = MockServer::start();
        let protected = api.mock(|when, then| {
            when.method(GET)
                .path("/g")
                .header("authorization", "Bearer TOK");
            then.status(200).json_body(json!({"ok": true}));
        });
        let req = AdhocExecuteRequestDto {
            connection_trn: conn.trn.clone(),
            method: "GET".to_string(),
            endpoint: format!("{}{}", api.base_url(), "/g"),
            headers: None,
            query: None,
            body: None,
            timeout_config: None,
            network_config: None,
            http_policy: None,
            response_policy: None,
            retry_policy: None,
        };
        let r = svc.execute_adhoc(req).await.unwrap();
        assert_eq!(r.status, 200);
        protected.assert();
    }

    #[tokio::test]
    #[ignore]
    async fn cli_connect_cc_local_token_failure() {
        let svc = make_service().await;
        let storage = svc.storage();
        crate::store::service::set_global_storage_service_for_tests(storage.clone()).await;

        // Mock token endpoint returns 400
        let token_server = MockServer::start();
        let _token = token_server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(400)
                .json_body(json!({"error":"invalid_client"}));
        });

        let mut conn = ConnectionConfig::new(
            "trn:openact:default:connection/cli-cc-fail".to_string(),
            "cli-cc-fail".to_string(),
            crate::models::AuthorizationType::OAuth2ClientCredentials,
        );
        conn.auth_parameters.oauth_parameters = Some(crate::models::OAuth2Parameters {
            client_id: "bad".to_string(),
            client_secret: "bad".to_string(),
            token_url: token_server.url("/token"),
            scope: None,
            redirect_uri: None,
            use_pkce: None,
        });
        svc.upsert_connection(&conn).await.unwrap();

        // Token acquisition should fail; status becomes not_issued
        let out = crate::oauth::runtime::get_cc_token(&conn.trn).await;
        assert!(out.is_err());
        let st = svc.connection_status(&conn.trn).await.unwrap().unwrap();
        assert_eq!(st.status, "not_issued");
    }

    #[tokio::test]
    async fn cli_connect_server_cc_success_mock() {
        // Mock server endpoints
        let server = MockServer::start();
        let connect = server.mock(|when, then|{
            when.method(POST).path("/api/v1/connect");
            then.status(200)
                .header("Content-Type","application/json")
                .json_body(json!({
                    "connection": {"trn":"trn:openact:default:connection/cc","name":"cc","authorizationType":"oauth2_client_credentials"},
                    "status": {"trn":"trn:openact:default:connection/cc","authorization_type":"oauth2_client_credentials","status":"ready"},
                    "test": {"status":200,"headers":{},"body":{}},
                    "next_hints": ["Connection test passed. Ready to use."]
                }));
        });

        // Build CLI
        let cli = Cli {
            db_url: None,
            json: true,
            server: Some(server.base_url()),
            command: Commands::Connect {
                provider: "p".to_string(),
                template: "t".to_string(),
                tenant: "default".to_string(),
                name: "cc".to_string(),
                secrets_file: None,
                inputs_file: None,
                overrides_file: None,
                auth_trn: None,
                endpoint: Some("https://httpbin.org/get".to_string()),
                dsl_file: None,
                poll_interval_secs: 1,
                poll_timeout_secs: 3,
            },
        };
        let _ = run(cli).await.unwrap();
        connect.assert();
    }

    #[tokio::test]
    async fn cli_connect_server_ac_success_polling_mock() {
        let server = MockServer::start();
        // First call returns run_id and authorize_url
        let connect = server.mock(|when, then| {
            when.method(POST).path("/api/v1/connect");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({
                    "connection_trn":"trn:openact:default:connection/ac",
                    "run_id":"RID1",
                    "authorize_url":"https://auth/authorize",
                    "state":"S",
                    "next_hints":["Open the authorize_url in a browser"]
                }));
        });
        // Poll returns done with hints
        let poll = server.mock(|when, then|{
            when.method(GET).path("/api/v1/connect/ac/status").query_param("run_id","RID1");
            then.status(200)
                .header("Content-Type","application/json")
                .json_body(json!({"done":true, "auth_trn":"trn:openact:default:connection/gh-alice", "bound_connection":"trn:openact:default:connection/ac", "next_hints":["Check connection status","Run connection test"]}));
        });

        // Create a temp DSL file to satisfy CLI file reading
        let tmp = tempfile::tempdir().unwrap();
        let dsl_path = tmp.path().join("ac.yml");
        std::fs::write(&dsl_path, "startAt: X\nstates: {}\n").unwrap();

        let cli = Cli {
            db_url: None,
            json: true,
            server: Some(server.base_url()),
            command: Commands::Connect {
                provider: "p".to_string(),
                template: "t".to_string(),
                tenant: "default".to_string(),
                name: "ac".to_string(),
                secrets_file: None,
                inputs_file: None,
                overrides_file: None,
                auth_trn: None,
                endpoint: None,
                dsl_file: Some(dsl_path),
                poll_interval_secs: 1,
                poll_timeout_secs: 3,
            },
        };
        let _ = run(cli).await.unwrap();
        connect.assert();
        poll.assert();
    }

    #[tokio::test]
    async fn cli_connect_server_cc_failure_mock() {
        let server = MockServer::start();
        let connect = server.mock(|when, then|{
            when.method(POST).path("/api/v1/connect");
            then.status(200)
                .header("Content-Type","application/json")
                .json_body(json!({
                    "connection": {"trn":"trn:openact:default:connection/cc","name":"cc","authorizationType":"oauth2_client_credentials"},
                    "status": {"trn":"trn:openact:default:connection/cc","authorization_type":"oauth2_client_credentials","status":"not_issued","message":"token failed"},
                    "test": {"status":500,"headers":{},"body":{}},
                    "next_hints": ["CC token acquisition failed: invalid_client"]
                }));
        });
        let cli = Cli {
            db_url: None,
            json: true,
            server: Some(server.base_url()),
            command: Commands::Connect {
                provider: "p".to_string(),
                template: "t".to_string(),
                tenant: "default".to_string(),
                name: "cc".to_string(),
                secrets_file: None,
                inputs_file: None,
                overrides_file: None,
                auth_trn: None,
                endpoint: Some("https://httpbin.org/get".to_string()),
                dsl_file: None,
                poll_interval_secs: 1,
                poll_timeout_secs: 3,
            },
        };
        let _ = run(cli).await.unwrap();
        connect.assert();
    }

    #[tokio::test]
    async fn cli_connect_server_ac_dsl_error_mock() {
        let server = MockServer::start();
        let connect = server.mock(|when, then|{
            when.method(POST).path("/api/v1/connect");
            then.status(400)
                .header("Content-Type","application/json")
                .json_body(json!({"error_code":"validation.dsl_error","message":"bad yaml","hints":["Ensure YAML is valid"]}));
        });
        // Create dummy dsl file
        let tmp = tempfile::tempdir().unwrap();
        let dsl_path = tmp.path().join("bad.yml");
        std::fs::write(&dsl_path, ":::").unwrap();

        let cli = Cli {
            db_url: None,
            json: true,
            server: Some(server.base_url()),
            command: Commands::Connect {
                provider: "p".to_string(),
                template: "t".to_string(),
                tenant: "default".to_string(),
                name: "ac".to_string(),
                secrets_file: None,
                inputs_file: None,
                overrides_file: None,
                auth_trn: None,
                endpoint: None,
                dsl_file: Some(dsl_path),
                poll_interval_secs: 1,
                poll_timeout_secs: 3,
            },
        };
        let err = run(cli).await.unwrap_err();
        assert!(format!("{}", err).contains("server error"));
        connect.assert();
    }

    #[tokio::test]
    async fn cli_oauth_device_code_success_and_bind_mock() {
        // Mock device and token endpoints
        let mock = MockServer::start();
        let _dev = mock.mock(|when, then|{
            when.method(POST).path("/device");
            then.status(200).header("Content-Type","application/json")
                .json_body(json!({"device_code":"D","user_code":"U","verification_uri":"https://verify","interval":1}));
        });
        let _tok = mock.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(json!({"access_token":"AT","refresh_token":"RT","expires_in":1800}));
        });

        // Prepare a connection to bind later; ensure global storage matches CLI run() path
        let svc = make_service().await;
        let storage = svc.storage();
        crate::store::service::set_global_storage_service_for_tests(storage.clone()).await;
        let conn = ConnectionConfig::new(
            "trn:openact:default:connection/dc-bind".to_string(),
            "dc-bind".to_string(),
            crate::models::AuthorizationType::OAuth2AuthorizationCode,
        );
        svc.upsert_connection(&conn).await.unwrap();

        // Build CLI for oauth device-code
        let cli = Cli {
            db_url: None,
            json: true,
            server: None,
            command: Commands::Oauth {
                cmd: OauthCmd::DeviceCode {
                    token_url: mock.url("/token"),
                    device_code_url: mock.url("/device"),
                    client_id: "id".to_string(),
                    client_secret: None,
                    scope: Some("repo".to_string()),
                    tenant: "default".to_string(),
                    provider: "github".to_string(),
                    user_id: "alice".to_string(),
                    bind_connection: Some(conn.trn.clone()),
                },
            },
        };
        let _ = run(cli).await.unwrap();
    }
}
