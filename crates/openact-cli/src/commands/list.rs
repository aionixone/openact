//! List command for connections, actions, and connectors

use crate::{
    cli::{ListResource, OutputFormat},
    error::CliResult,
    utils::{truncate_text, ColoredOutput},
};
use openact_core::store::{ActionRepository, ConnectionStore};
use openact_core::{ActionRecord, ConnectionRecord, ConnectorKind};
use openact_registry::ConnectorRegistry;
use openact_store::sql_store::SqlStore;
use serde_json::{json, Value as JsonValue};
use tracing::debug;

pub struct ListCommand;

impl ListCommand {
    pub async fn run(db_path: &str, resource: ListResource) -> CliResult<()> {
        // Create database connection
        let store = SqlStore::new(db_path).await?;

        match resource {
            ListResource::Connections { connector, format } => {
                Self::list_connections(&store, connector.as_deref(), format).await
            }
            ListResource::Actions {
                connector,
                connection,
                format,
            } => {
                Self::list_actions(&store, connector.as_deref(), connection.as_deref(), format)
                    .await
            }
            ListResource::Connectors { format } => Self::list_connectors(&store, format).await,
        }
    }

    async fn list_connections(
        store: &SqlStore,
        connector_filter: Option<&str>,
        format: OutputFormat,
    ) -> CliResult<()> {
        debug!("Listing connections with filter: {:?}", connector_filter);

        let connections = if let Some(connector) = connector_filter {
            ConnectionStore::list_by_connector(store, connector).await?
        } else {
            // Get all connections by listing all connectors first
            let connectors = ConnectionStore::list_distinct_connectors(store).await?;
            let mut all_connections = Vec::new();
            for connector_kind in connectors {
                let conn_list =
                    ConnectionStore::list_by_connector(store, connector_kind.as_str()).await?;
                all_connections.extend(conn_list);
            }
            all_connections
        };

        match format {
            OutputFormat::Table => {
                Self::display_connections_table(&connections);
            }
            _ => {
                let json_data = Self::connections_to_json(&connections);
                println!("{}", format.format_json(&json_data)?);
            }
        }

        Ok(())
    }

    async fn list_actions(
        store: &SqlStore,
        connector_filter: Option<&str>,
        connection_filter: Option<&str>,
        format: OutputFormat,
    ) -> CliResult<()> {
        debug!(
            "Listing actions with connector filter: {:?}, connection filter: {:?}",
            connector_filter, connection_filter
        );

        let actions = if let Some(connector) = connector_filter {
            let connector_kind = ConnectorKind::new(connector);
            ActionRepository::list_by_connector(store, &connector_kind).await?
        } else if let Some(connection) = connection_filter {
            let connection_trn = crate::utils::parse_trn(connection)?;
            ActionRepository::list_by_connection(store, &connection_trn).await?
        } else {
            // Get all actions by listing all connectors first
            let connectors = ConnectionStore::list_distinct_connectors(store).await?;
            let mut all_actions = Vec::new();
            for connector_kind in connectors {
                let action_list =
                    ActionRepository::list_by_connector(store, &connector_kind).await?;
                all_actions.extend(action_list);
            }
            all_actions
        };

        match format {
            OutputFormat::Table => {
                Self::display_actions_table(&actions);
            }
            _ => {
                let json_data = Self::actions_to_json(&actions);
                println!("{}", format.format_json(&json_data)?);
            }
        }

        Ok(())
    }

    async fn list_connectors(store: &SqlStore, format: OutputFormat) -> CliResult<()> {
        debug!("Listing registered connectors");

        // Get distinct connectors from database
        let db_connectors = ConnectionStore::list_distinct_connectors(store).await?;

        // Create a simple registry to get registered connectors
        // TODO: This could be enhanced to show which connectors are registered vs. available in DB
        let registry = ConnectorRegistry::new(store.clone(), store.clone());
        let registered_connectors = registry.registered_connectors();

        match format {
            OutputFormat::Table => {
                Self::display_connectors_table(&db_connectors, &registered_connectors);
            }
            _ => {
                let json_data = Self::connectors_to_json(&db_connectors, &registered_connectors);
                println!("{}", format.format_json(&json_data)?);
            }
        }

        Ok(())
    }

    fn display_connections_table(connections: &[ConnectionRecord]) {
        if connections.is_empty() {
            println!("{}", ColoredOutput::info("No connections found"));
            return;
        }

        println!(
            "{}",
            ColoredOutput::success(&format!("Found {} connection(s):", connections.len()))
        );
        println!();

        // Table header
        println!(
            "{:<40} {:<12} {:<20} {:<15} {:<10}",
            ColoredOutput::highlight("TRN"),
            ColoredOutput::highlight("Connector"),
            ColoredOutput::highlight("Name"),
            ColoredOutput::highlight("Created"),
            ColoredOutput::highlight("Version")
        );
        println!("{}", "-".repeat(100));

        for connection in connections {
            let created = connection.created_at.format("%Y-%m-%d %H:%M").to_string();
            println!(
                "{:<40} {:<12} {:<20} {:<15} {:<10}",
                truncate_text(&connection.trn.to_string(), 40),
                connection.connector.as_str(),
                truncate_text(&connection.name, 20),
                created,
                connection.version
            );
        }
    }

    fn display_actions_table(actions: &[ActionRecord]) {
        if actions.is_empty() {
            println!("{}", ColoredOutput::info("No actions found"));
            return;
        }

        println!(
            "{}",
            ColoredOutput::success(&format!("Found {} action(s):", actions.len()))
        );
        println!();

        // Table header
        println!(
            "{:<40} {:<12} {:<20} {:<40} {:<10}",
            ColoredOutput::highlight("TRN"),
            ColoredOutput::highlight("Connector"),
            ColoredOutput::highlight("Name"),
            ColoredOutput::highlight("Connection"),
            ColoredOutput::highlight("MCP")
        );
        println!("{}", "-".repeat(125));

        for action in actions {
            let mcp_status = if action.mcp_enabled { "✓" } else { "-" };
            println!(
                "{:<40} {:<12} {:<20} {:<40} {:<10}",
                truncate_text(&action.trn.to_string(), 40),
                action.connector.as_str(),
                truncate_text(&action.name, 20),
                truncate_text(&action.connection_trn.to_string(), 40),
                mcp_status
            );
        }
    }

    fn display_connectors_table(
        db_connectors: &[ConnectorKind],
        registered_connectors: &[ConnectorKind],
    ) {
        println!("{}", ColoredOutput::success("Connector Status:"));
        println!();

        // Combine and deduplicate connectors
        let mut all_connectors: std::collections::HashSet<&ConnectorKind> =
            std::collections::HashSet::new();
        all_connectors.extend(db_connectors);
        all_connectors.extend(registered_connectors);

        if all_connectors.is_empty() {
            println!("{}", ColoredOutput::info("No connectors found"));
            return;
        }

        // Table header
        println!(
            "{:<15} {:<12} {:<12}",
            ColoredOutput::highlight("Connector"),
            ColoredOutput::highlight("In DB"),
            ColoredOutput::highlight("Registered")
        );
        println!("{}", "-".repeat(40));

        for connector in all_connectors {
            let in_db = if db_connectors.contains(connector) {
                "✓"
            } else {
                "-"
            };
            let registered = if registered_connectors.contains(connector) {
                "✓"
            } else {
                "-"
            };

            println!(
                "{:<15} {:<12} {:<12}",
                connector.as_str(),
                in_db,
                registered
            );
        }
    }

    fn connections_to_json(connections: &[ConnectionRecord]) -> JsonValue {
        json!({
            "connections": connections.iter().map(|conn| {
                json!({
                    "trn": conn.trn.to_string(),
                    "connector": conn.connector.as_str(),
                    "name": conn.name,
                    "config": conn.config_json,
                    "created_at": conn.created_at.to_rfc3339(),
                    "updated_at": conn.updated_at.to_rfc3339(),
                    "version": conn.version
                })
            }).collect::<Vec<_>>(),
            "count": connections.len()
        })
    }

    fn actions_to_json(actions: &[ActionRecord]) -> JsonValue {
        json!({
            "actions": actions.iter().map(|action| {
                json!({
                    "trn": action.trn.to_string(),
                    "connector": action.connector.as_str(),
                    "name": action.name,
                    "connection_trn": action.connection_trn.to_string(),
                    "config": action.config_json,
                    "mcp_enabled": action.mcp_enabled,
                    "mcp_overrides": action.mcp_overrides,
                    "created_at": action.created_at.to_rfc3339(),
                    "updated_at": action.updated_at.to_rfc3339(),
                    "version": action.version
                })
            }).collect::<Vec<_>>(),
            "count": actions.len()
        })
    }

    fn connectors_to_json(
        db_connectors: &[ConnectorKind],
        registered_connectors: &[ConnectorKind],
    ) -> JsonValue {
        let mut all_connectors: std::collections::HashSet<&ConnectorKind> =
            std::collections::HashSet::new();
        all_connectors.extend(db_connectors);
        all_connectors.extend(registered_connectors);

        json!({
            "connectors": all_connectors.iter().map(|connector| {
                json!({
                    "name": connector.as_str(),
                    "in_database": db_connectors.contains(connector),
                    "registered": registered_connectors.contains(connector)
                })
            }).collect::<Vec<_>>(),
            "count": all_connectors.len()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_list_empty_connections() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let store = SqlStore::new(db_path.to_str().unwrap()).await.unwrap();
        store.migrate().await.unwrap();

        let result = ListCommand::list_connections(&store, None, OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_connections_with_data() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let store = SqlStore::new(db_path.to_str().unwrap()).await.unwrap();
        store.migrate().await.unwrap();

        // Add test connection
        let connection = ConnectionRecord {
            trn: openact_core::Trn::new("trn:openact:test:connection/http/test"),
            connector: ConnectorKind::new("http"),
            name: "test".to_string(),
            config_json: json!({"base_url": "https://api.example.com"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };
        ConnectionStore::upsert(&store, &connection).await.unwrap();

        let result = ListCommand::list_connections(&store, Some("http"), OutputFormat::Json).await;
        assert!(result.is_ok());
    }
}
