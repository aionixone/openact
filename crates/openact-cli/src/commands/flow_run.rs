use std::{
    io::{self, Write},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use clap::Args;
use openact_authflow::engine::TaskHandler;
use openact_authflow::{
    actions::ActionRouter,
    runner::{FlowRunner, FlowRunnerConfig},
};
use openact_core::store::{AuthConnectionStore, RunStore};
use openact_store::SqlStore;
use serde_json::Value as JsonValue;
use stepflow_dsl::WorkflowDSL;

use crate::{
    error::CliResult,
    utils::{read_input_data, write_output_data, ColoredOutput},
};
use tracing::info;

#[derive(Args, Debug, Clone)]
pub struct FlowRunArgs {
    #[arg(long, help = "Path to StepFlow DSL file (YAML)")]
    pub dsl: String,

    #[arg(short, long, conflicts_with = "input_file", help = "Input data as JSON string")]
    pub input: Option<String>,

    #[arg(long, help = "Input data from file (JSON or YAML)")]
    pub input_file: Option<String>,

    #[arg(
        long,
        default_value = "/vars/auth/authorize_url",
        help = "Pointer to authorize URL in context"
    )]
    pub authorize_url_ptr: String,

    #[arg(long, default_value = "/vars/auth/state", help = "Pointer to state value in context")]
    pub state_ptr: String,

    #[arg(long, default_value = "/redirectUri", help = "Pointer in input to inject callback URL")]
    pub redirect_ptr: String,

    #[arg(long, default_value = "/auth_ref", help = "Pointer in final context for auth_ref")]
    pub auth_ref_ptr: String,

    #[arg(
        long,
        default_value = "/connection_ref",
        help = "Pointer in final context for connection_ref"
    )]
    pub connection_ref_ptr: String,

    #[arg(long, default_value = "127.0.0.1:0", help = "Callback bind address")]
    pub callback_addr: String,

    #[arg(long, default_value = "/oauth/callback", help = "Callback HTTP path")]
    pub callback_path: String,

    #[arg(long, default_value = "300", help = "Timeout waiting for callback (seconds)")]
    pub timeout_secs: u64,

    #[arg(long, help = "Write flow result JSON to file")]
    pub output: Option<String>,
}

pub struct FlowRunCommand;

impl FlowRunCommand {
    pub async fn run(db_path: &str, args: FlowRunArgs) -> CliResult<()> {
        crate::utils::validate_file_exists(&args.dsl)?;
        let dsl_text = tokio::fs::read_to_string(&args.dsl)
            .await
            .with_context(|| format!("Failed to read DSL file {}", args.dsl))?;
        let dsl: WorkflowDSL = serde_yaml::from_str(&dsl_text)
            .with_context(|| format!("Failed to parse StepFlow DSL from {}", args.dsl))?;
        let dsl = Arc::new(dsl);

        let input_data: JsonValue = read_input_data(args.input.clone(), args.input_file.clone())?;

        let sql_store = SqlStore::new(db_path)
            .await
            .with_context(|| format!("Failed to open database at {}", db_path))?;
        sql_store
            .migrate()
            .await
            .with_context(|| "Failed to run database migrations".to_string())?;

        let auth_store: Arc<dyn AuthConnectionStore> = Arc::new(sql_store.clone());
        let run_store: Arc<dyn RunStore> = Arc::new(sql_store.clone());
        let task_handler: Arc<dyn TaskHandler> = Arc::new(ActionRouter::new(auth_store.clone()));

        let config = FlowRunnerConfig {
            authorize_url_ptr: args.authorize_url_ptr.clone(),
            state_ptr: args.state_ptr.clone(),
            redirect_ptr: Some(args.redirect_ptr.clone()),
            auth_ref_ptr: Some(args.auth_ref_ptr.clone()),
            connection_ref_ptr: Some(args.connection_ref_ptr.clone()),
            callback_addr: args
                .callback_addr
                .parse::<SocketAddr>()
                .with_context(|| format!("Invalid callback address: {}", args.callback_addr))?,
            callback_path: args.callback_path.clone(),
            callback_timeout: Duration::from_secs(args.timeout_secs),
        };

        let runner =
            FlowRunner::new(Arc::clone(&dsl), Arc::clone(&task_handler), run_store, config);
        let handle = runner.start(input_data).await?;
        let authorize_url = handle.authorize_url.clone();
        let callback_url = handle.callback_url.clone();

        println!("{}:\n   {}", ColoredOutput::highlight("Authorize URL"), authorize_url);
        println!("{} {}", ColoredOutput::info("Waiting for callback at"), callback_url);
        io::stdout().flush().ok();
        info!(
            authorize_url = %authorize_url,
            callback_url = %callback_url,
            "OAuth flow paused; open the authorize URL and complete the callback"
        );

        let result = handle.wait_for_completion().await?;

        println!("{} {}", ColoredOutput::success("Flow completed. Run ID:"), result.run_id);
        if let Some(auth_ref) = &result.auth_ref {
            println!("{} {}", ColoredOutput::highlight("auth_ref"), auth_ref);
        }
        if let Some(connection_ref) = &result.connection_ref {
            println!("{} {}", ColoredOutput::highlight("connection_ref"), connection_ref);
        }

        let output_obj = serde_json::json!({
            "run_id": result.run_id,
            "auth_ref": result.auth_ref,
            "connection_ref": result.connection_ref,
            "final_context": result.final_context,
        });

        if let Some(file) = args.output {
            let content = serde_json::to_string_pretty(&output_obj)?;
            write_output_data(&file, &content)?;
            println!("{} {}", ColoredOutput::success("Result written to"), file);
        } else {
            println!(
                "{}\n{}",
                ColoredOutput::highlight("Final context"),
                serde_json::to_string_pretty(&output_obj)?
            );
        }

        Ok(())
    }
}
