//! OpenAct CLI main entry point

use clap::Parser;
use openact_cli::{
    cli::{Cli, Commands},
    commands::{ExecuteCommand, ExportCommand, ImportCommand, ListCommand, MigrateCommand},
    error::CliResult,
    utils::{init_tracing, ColoredOutput},
};
use tracing::info;

#[tokio::main]
async fn main() {
    let exit_code = match run().await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{} {}", ColoredOutput::error("Error:"), e);
            1
        }
    };

    std::process::exit(exit_code);
}

async fn run() -> CliResult<()> {
    let cli = Cli::parse();

    // Initialize tracing
    init_tracing()?;

    // Disable colored output if requested
    if cli.no_color {
        colored::control::set_override(false);
    }

    info!("OpenAct CLI v{}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Commands::ServeMcp { args } => {
            openact_cli::commands::serve_mcp::execute(args, &cli.db_path)
                .await
                .map_err(|e| e.into())
        }

        Commands::ServeRest { args } => {
            openact_cli::commands::serve_rest::execute(args, &cli.db_path)
                .await
                .map_err(|e| e.into())
        }

        Commands::Serve {
            rest,
            mcp_http,
            mcp_stdio,
            allow_patterns,
            deny_patterns,
            max_concurrency,
            timeout_secs,
        } => {
            let app_state = openact_server::AppState::from_db_path(&cli.db_path).await?;
            let governance = openact_server::GovernanceConfig::new(
                allow_patterns,
                deny_patterns,
                max_concurrency,
                timeout_secs,
            );
            let cfg = openact_server::ServeConfig {
                rest_addr: rest,
                mcp_http_addr: mcp_http,
                mcp_stdio,
            };
            openact_server::serve_unified(app_state, governance, cfg)
                .await
                .map_err(|e| e.into())
        }

        Commands::Migrate { force } => MigrateCommand::run(&cli.db_path, force).await,

        Commands::Import {
            file,
            conflict_resolution,
            dry_run,
        } => ImportCommand::run(&cli.db_path, &file, conflict_resolution, dry_run).await,

        Commands::Export {
            file,
            connectors,
            include_sensitive,
            pretty,
        } => ExportCommand::run(&cli.db_path, &file, connectors, include_sensitive, pretty).await,

        Commands::List { resource } => ListCommand::run(&cli.db_path, resource).await,

        Commands::Execute {
            action_trn,
            input,
            input_file,
            format,
            output,
            show_metadata,
        } => {
            ExecuteCommand::run(
                &cli.db_path,
                &action_trn,
                input,
                input_file,
                format,
                output,
                show_metadata,
            )
            .await
        }

        Commands::ExecuteFile {
            config_file,
            action_name,
            input,
            input_file,
            format,
            output,
            show_metadata,
            dry_run,
            timeout,
        } => {
            openact_cli::commands::execute_file::execute(
                &config_file,
                &action_name,
                input,
                input_file,
                format,
                output,
                show_metadata,
                dry_run,
                timeout,
            )
            .await
        }

        Commands::ExecuteInline {
            config_json,
            action_name,
            input,
            input_file,
            format,
            output,
            show_metadata,
            dry_run,
            timeout,
        } => {
            openact_cli::commands::execute_inline::execute(
                &config_json,
                &action_name,
                input,
                input_file,
                format,
                output,
                show_metadata,
                dry_run,
                timeout,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use openact_cli::cli::ListResource;

    #[test]
    fn test_cli_parsing() {
        // Test basic command parsing
        let cli =
            Cli::try_parse_from(&["openact", "--db-path", "/tmp/test.db", "migrate"]).unwrap();

        assert_eq!(cli.db_path, "/tmp/test.db");
        matches!(cli.command, Commands::Migrate { .. });
    }

    #[test]
    fn test_execute_command_parsing() {
        let cli = Cli::try_parse_from(&[
            "openact",
            "execute",
            "trn:openact:test:action/http/get-user",
            "--input",
            r#"{"id": 123}"#,
            "--format",
            "json",
            "--show-metadata",
        ])
        .unwrap();

        if let Commands::Execute {
            action_trn,
            input,
            show_metadata,
            ..
        } = cli.command
        {
            assert_eq!(action_trn, "trn:openact:test:action/http/get-user");
            assert_eq!(input, Some(r#"{"id": 123}"#.to_string()));
            assert!(show_metadata);
        } else {
            panic!("Expected Execute command");
        }
    }

    #[test]
    fn test_list_command_parsing() {
        let cli = Cli::try_parse_from(&[
            "openact",
            "list",
            "connections",
            "--connector",
            "http",
            "--format",
            "table",
        ])
        .unwrap();

        if let Commands::List { resource } = cli.command {
            if let ListResource::Connections { connector, .. } = resource {
                assert_eq!(connector, Some("http".to_string()));
            } else {
                panic!("Expected Connections resource");
            }
        } else {
            panic!("Expected List command");
        }
    }
}
