use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "openapi-cli", version, about = "OpenAct CLI")] 
struct Cli {
    #[arg(long, global = true)]
    db_url: Option<String>,

    #[arg(long, global = true, default_value_t = false)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute a task by TRN
    Execute {
        /// Task TRN
        task_trn: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();

    if let Some(db) = &cli.db_url {
        std::env::set_var("OPENACT_DB_URL", db);
    }

    match &cli.command {
        Commands::Execute { task_trn } => {
            let service = openact::store::StorageService::global().await;
            let result = service.execute_by_trn(task_trn).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&json!({
                    "status": result.status,
                    "headers": result.headers,
                    "body": result.body,
                }))?);
            } else {
                println!("Status: {}", result.status);
                println!("Headers:");
                for (k, v) in result.headers.iter() { println!("  {}: {}", k, v); }
                println!("Body:\n{}", serde_json::to_string_pretty(&result.body)?);
            }
        }
    }

    Ok(())
}
