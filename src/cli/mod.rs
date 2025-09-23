use crate::interface::dto::{ExecuteOverridesDto, ExecuteRequestDto, ExecuteResponseDto};
use crate::models::common::RetryPolicy;
use crate::models::{ConnectionConfig, TaskConfig};
use crate::store::{DatabaseManager, StorageService};
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

async fn execute_via_server(
    base: &str,
    task_trn: &str,
    overrides: &ExecuteOverrides,
    json_out: bool,
) -> Result<()> {
    // Build request DTO
    let mut hdr: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for kv in &overrides.headers {
        if let Some((k, v)) = kv.split_once(':') {
            hdr.insert(k.trim().to_string(), vec![v.trim().to_string()]);
        }
    }
    let mut qs: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for kv in &overrides.queries {
        if let Some((k, v)) = kv.split_once('=') {
            qs.insert(k.trim().to_string(), vec![v.trim().to_string()]);
        }
    }
    let body_json = if let Some(b) = &overrides.body {
        let content = if let Some(path) = b.strip_prefix('@') {
            std::fs::read_to_string(path)?
        } else {
            b.clone()
        };
        Some(parse_json_or_yaml::<serde_json::Value>(&content)?)
    } else {
        None
    };

    // 构建重试策略
    let retry_policy = build_retry_policy_from_overrides(overrides)?;
    
    let req = ExecuteRequestDto {
        overrides: Some(ExecuteOverridesDto {
            method: overrides.method.clone(),
            endpoint: overrides.endpoint.clone(),
            headers: if hdr.is_empty() { None } else { Some(hdr) },
            query: if qs.is_empty() { None } else { Some(qs) },
            body: body_json,
            retry_policy,
        }),
        output: Some(overrides.output.clone()),
    };

    let url = format!(
        "{}/api/v1/tasks/{}/execute",
        base.trim_end_matches('/'),
        urlencoding::encode(task_trn)
    );
    let resp = reqwest::Client::new().post(&url).json(&req).send().await?;
    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        let text = String::from_utf8_lossy(&bytes);
        return Err(anyhow!("server error {}: {}", status, text));
    }
    let dto: ExecuteResponseDto = serde_json::from_slice(&bytes)?;
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": dto.status, "headers": dto.headers, "body": dto.body
            }))?
        );
    } else {
        println!("Status: {}", dto.status);
        println!("Headers:");
        for (k, v) in dto.headers.iter() {
            println!("  {}: {}", k, v);
        }
        println!("Body:\n{}", serde_json::to_string_pretty(&dto.body)?);
    }
    Ok(())
}

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
}

#[derive(Subcommand, Debug)]
pub enum OauthCmd {
    /// Start Authorization Code flow (prints authorize_url/state/code_verifier)
    Start {
        /// DSL YAML file path
        #[arg(short, long)]
        dsl: std::path::PathBuf,
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
}

pub async fn run(cli: Cli) -> Result<()> {
    // Initialize storage service (prefer explicit db_url)
    let service: std::sync::Arc<StorageService> = if let Some(db) = &cli.db_url {
        let manager = DatabaseManager::new(db).await?;
        std::sync::Arc::new(StorageService::new(manager))
    } else {
        StorageService::global().await
    };

    match &cli.command {
        Commands::Execute {
            task_trn,
            overrides,
        } => {
            // If --server is provided, proxy via HTTP API
            if let Some(base) = &cli.server {
                return execute_via_server(base, task_trn, overrides, cli.json).await;
            }
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
        },
        Commands::Oauth { cmd } => {
            match cmd {
                OauthCmd::Start { dsl } => {
                    let yaml = std::fs::read_to_string(dsl)?;
                    let dsl: stepflow_dsl::WorkflowDSL = serde_yaml::from_str(&yaml)?;
                    let run_store = crate::store::MemoryRunStore::default();
                    let router = crate::authflow::actions::DefaultRouter; // not Default
                    let res =
                        crate::api::start_obtain(&dsl, &router, &run_store, serde_json::json!({}))?;
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
                    let out = crate::api::resume_obtain(
                        &dsl,
                        &router,
                        &run_store,
                        crate::api::ResumeObtainArgs {
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
                        } else {
                            println!("[warn] cannot detect auth_trn from flow output; skip bind");
                        }
                    }
                    println!("{}", serde_json::to_string_pretty(&out)?);
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
            }
        }
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
    if overrides.max_retries.is_none() && overrides.retry_delay_ms.is_none() && overrides.retry_policy.is_none() {
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
