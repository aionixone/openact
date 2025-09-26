use clap::{Args, Parser, Subcommand};
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
