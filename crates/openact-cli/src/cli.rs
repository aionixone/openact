//! CLI argument definitions using clap

use clap::{Parser, Subcommand};
use serde_json::Value as JsonValue;

#[derive(Parser)]
#[command(
    name = "openact",
    about = "OpenAct - Universal API Action Executor",
    version,
    author = "OpenAct Team"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Database file path
    #[arg(
        long,
        env = "OPENACT_DB_PATH",
        default_value = "./data/openact.db",
        help = "Path to SQLite database file"
    )]
    pub db_path: String,

    /// Enable verbose logging
    #[arg(short, long, help = "Enable verbose output")]
    pub verbose: bool,

    /// Disable colored output
    #[arg(long, help = "Disable colored output")]
    pub no_color: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start MCP (Model Context Protocol) server
    ServeMcp {
        #[command(flatten)]
        args: crate::commands::ServeMcpArgs,
    },
    /// Start REST API server
    ServeRest {
        #[command(flatten)]
        args: crate::commands::ServeRestArgs,
    },
    /// Start unified server (REST + MCP)
    Serve {
        /// Host and port for REST API (optional)
        #[arg(long)]
        rest: Option<String>,
        /// Host and port for MCP over HTTP (optional)
        #[arg(long = "mcp-http")]
        mcp_http: Option<String>,
        /// Enable MCP over stdio
        #[arg(long = "mcp-stdio", default_value = "false")]
        mcp_stdio: bool,
        /// Allow patterns for action filtering
        #[arg(long = "allow")]
        allow_patterns: Vec<String>,
        /// Deny patterns for action filtering
        #[arg(long = "deny")]
        deny_patterns: Vec<String>,
        /// Maximum concurrency
        #[arg(long, default_value = "10")]
        max_concurrency: usize,
        /// Timeout seconds
        #[arg(long, default_value = "30")]
        timeout_secs: u64,
    },
    /// Initialize database and run migrations
    Migrate {
        /// Force re-run all migrations
        #[arg(long, help = "Force re-run all migrations")]
        force: bool,
    },

    /// Import configuration from file
    Import {
        /// Configuration file path (YAML or JSON)
        #[arg(help = "Configuration file to import")]
        file: String,

        /// Conflict resolution strategy
        #[arg(
            long,
            value_enum,
            default_value = "abort",
            help = "How to handle conflicts"
        )]
        conflict_resolution: ConflictResolution,

        /// Dry run - don't actually import
        #[arg(long, help = "Show what would be imported without making changes")]
        dry_run: bool,
    },

    /// Export configuration to file
    Export {
        /// Output file path (YAML or JSON, determined by extension)
        #[arg(help = "Output file path")]
        file: String,

        /// Connector types to export (default: all)
        #[arg(
            short,
            long,
            help = "Connector types to export (e.g., http,postgresql)"
        )]
        connectors: Vec<String>,

        /// Include sensitive data in export
        #[arg(long, help = "Include sensitive data (passwords, tokens) in export")]
        include_sensitive: bool,

        /// Pretty print JSON output
        #[arg(long, help = "Pretty print JSON output")]
        pretty: bool,
    },

    /// List connections and actions
    List {
        #[command(subcommand)]
        resource: ListResource,
    },

    /// Execute an action
    Execute {
        /// Action TRN to execute
        #[arg(help = "Action TRN (e.g., trn:openact:tenant:action/http/get-user)")]
        action_trn: String,

        /// Input data as JSON string
        #[arg(short, long, help = "Input data as JSON string")]
        input: Option<String>,

        /// Input data from file
        #[arg(
            long,
            conflicts_with = "input",
            help = "Read input data from file (JSON or YAML)"
        )]
        input_file: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "pretty", help = "Output format")]
        format: OutputFormat,

        /// Save output to file
        #[arg(long, help = "Save output to file")]
        output: Option<String>,

        /// Show execution metadata
        #[arg(long, help = "Include execution metadata in output")]
        show_metadata: bool,
    },

    /// Execute action from configuration file
    ExecuteFile {
        /// Configuration file path (YAML or JSON)
        #[arg(help = "Configuration file containing connections and actions")]
        config_file: String,

        /// Action name to execute (from config file)
        #[arg(help = "Action name as defined in the config file")]
        action_name: String,

        /// Input data as JSON string
        #[arg(short, long, help = "Input data as JSON string")]
        input: Option<String>,

        /// Input data from file
        #[arg(
            long,
            conflicts_with = "input",
            help = "Read input data from file (JSON or YAML)"
        )]
        input_file: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "pretty", help = "Output format")]
        format: OutputFormat,

        /// Save output to file
        #[arg(long, help = "Save output to file")]
        output: Option<String>,

        /// Show execution metadata
        #[arg(long, help = "Include execution metadata in output")]
        show_metadata: bool,

        /// Dry run - validate configuration and action but don't execute
        #[arg(long, help = "Validate configuration and action but don't execute")]
        dry_run: bool,

        /// Timeout in seconds
        #[arg(long, default_value = "30", help = "Execution timeout in seconds")]
        timeout: u64,
    },

    /// Execute action from inline configuration
    ExecuteInline {
        /// JSON configuration containing connections and actions
        #[arg(help = "Inline JSON configuration")]
        config_json: String,

        /// Action name to execute
        #[arg(help = "Action name as defined in the inline configuration")]
        action_name: String,

        /// Input data as JSON string
        #[arg(short, long, help = "Input data as JSON string")]
        input: Option<String>,

        /// Input data from file
        #[arg(
            long,
            conflicts_with = "input",
            help = "Read input data from file (JSON or YAML)"
        )]
        input_file: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "pretty", help = "Output format")]
        format: OutputFormat,

        /// Save output to file
        #[arg(long, help = "Save output to file")]
        output: Option<String>,

        /// Show execution metadata
        #[arg(long, help = "Include execution metadata in output")]
        show_metadata: bool,

        /// Dry run - validate configuration and action but don't execute
        #[arg(long, help = "Validate configuration and action but don't execute")]
        dry_run: bool,

        /// Timeout in seconds
        #[arg(long, default_value = "30", help = "Execution timeout in seconds")]
        timeout: u64,
    },
}

#[derive(Subcommand)]
pub enum ListResource {
    /// List connections
    Connections {
        /// Filter by connector type
        #[arg(short, long, help = "Filter by connector type")]
        connector: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "table", help = "Output format")]
        format: OutputFormat,
    },

    /// List actions
    Actions {
        /// Filter by connector type
        #[arg(short, long, help = "Filter by connector type")]
        connector: Option<String>,

        /// Filter by connection TRN
        #[arg(long, help = "Filter by connection TRN")]
        connection: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "table", help = "Output format")]
        format: OutputFormat,
    },

    /// List registered connectors
    Connectors {
        /// Output format
        #[arg(long, value_enum, default_value = "table", help = "Output format")]
        format: OutputFormat,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ConflictResolution {
    /// Abort on any conflict
    Abort,
    /// Skip conflicting items
    Skip,
    /// Overwrite existing items
    Overwrite,
}

impl From<ConflictResolution> for openact_config::ConflictResolution {
    fn from(value: ConflictResolution) -> Self {
        match value {
            ConflictResolution::Abort => Self::Fail,
            ConflictResolution::Skip => Self::PreferDb,
            ConflictResolution::Overwrite => Self::PreferFile,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum OutputFormat {
    /// Human-readable table format
    Table,
    /// Pretty-printed JSON
    Pretty,
    /// Compact JSON
    Json,
    /// YAML format
    Yaml,
}

impl OutputFormat {
    /// Format a JSON value according to the output format
    pub fn format_json(&self, value: &JsonValue) -> Result<String, serde_json::Error> {
        match self {
            Self::Table => {
                // For table format, we'll handle this in the specific command implementations
                Ok(serde_json::to_string_pretty(value)?)
            }
            Self::Pretty => serde_json::to_string_pretty(value),
            Self::Json => serde_json::to_string(value),
            Self::Yaml => serde_yaml::to_string(value).map_err(|e| {
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("YAML serialization error: {}", e),
                ))
            }),
        }
    }
}
