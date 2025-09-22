//! Connection repository for managing Connection configurations
//!
//! This module provides CRUD operations for Connection configurations,
//! with support for encryption of sensitive authentication parameters.

use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::{SqlitePool, Row};
use serde_json;

use crate::{
    models::{ConnectionConfig, AuthParameters, InvocationHttpParameters},
    store::encryption::{FieldEncryption, EncryptedField},
};

/// Repository for managing Connection configurations
pub struct ConnectionRepository {
    pool: SqlitePool,
    encryption: Option<FieldEncryption>,
}

impl ConnectionRepository {
    /// Create a new ConnectionRepository
    pub fn new(pool: SqlitePool, encryption: Option<FieldEncryption>) -> Self {
        Self { pool, encryption }
    }

    /// Create or update a connection
    pub async fn upsert(&self, connection: &ConnectionConfig) -> Result<()> {
        // Encrypt sensitive auth_parameters
        let auth_params_json = serde_json::to_string(&connection.auth_parameters)?;
        let (auth_params_encrypted, auth_params_nonce, key_version) = 
            self.encrypt_field(&auth_params_json)?;

        // Serialize optional JSON fields
        let default_headers_json = connection.invocation_http_parameters
            .as_ref()
            .map(|p| serde_json::to_string(&p.header_parameters))
            .transpose()?;
        
        let default_query_params_json = connection.invocation_http_parameters
            .as_ref()
            .map(|p| serde_json::to_string(&p.query_string_parameters))
            .transpose()?;
        
        let default_body_json = connection.invocation_http_parameters
            .as_ref()
            .map(|p| serde_json::to_string(&p.body_parameters))
            .transpose()?;

        let network_config_json = connection.network_config
            .as_ref()
            .map(|nc| serde_json::to_string(nc))
            .transpose()?;

        let timeout_config_json = connection.timeout_config
            .as_ref()
            .map(|tc| serde_json::to_string(tc))
            .transpose()?;

        let http_policy_json = connection.http_policy
            .as_ref()
            .map(|hp| serde_json::to_string(hp))
            .transpose()?;

        let authorization_type_str = serde_json::to_string(&connection.authorization_type)?
            .trim_matches('"').to_string(); // Remove quotes from enum

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO connections (
                trn, name, authorization_type, auth_params_encrypted, auth_params_nonce,
                auth_ref,
                default_headers_json, default_query_params_json, default_body_json,
                network_config_json, timeout_config_json, http_policy_json,
                key_version, created_at, updated_at, version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&connection.trn)
        .bind(&connection.name)
        .bind(&authorization_type_str)
        .bind(&auth_params_encrypted)
        .bind(&auth_params_nonce)
        .bind(&connection.auth_ref)
        .bind(&default_headers_json)
        .bind(&default_query_params_json)
        .bind(&default_body_json)
        .bind(&network_config_json)
        .bind(&timeout_config_json)
        .bind(&http_policy_json)
        .bind(key_version)
        .bind(&connection.created_at)
        .bind(&Utc::now()) // Always update updated_at
        .bind(&connection.version)
        .execute(&self.pool)
        .await
        .map_err(|e| anyhow!("Failed to upsert connection: {}", e))?;

        Ok(())
    }

    /// Get a connection by TRN
    pub async fn get_by_trn(&self, trn: &str) -> Result<Option<ConnectionConfig>> {
        let row = sqlx::query(
            r#"
            SELECT trn, name, authorization_type, auth_params_encrypted, auth_params_nonce,
                   auth_ref,
                   default_headers_json, default_query_params_json, default_body_json,
                   network_config_json, timeout_config_json, http_policy_json,
                   key_version, created_at, updated_at, version
            FROM connections WHERE trn = ?1
            "#,
        )
        .bind(trn)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch connection: {}", e))?;

        match row {
            Some(row) => {
                let auth_params_encrypted: String = row.get("auth_params_encrypted");
                let auth_params_nonce: String = row.get("auth_params_nonce");
                let key_version: i64 = row.get("key_version");

                // Decrypt auth_parameters
                let auth_params_json = self.decrypt_field(
                    &auth_params_encrypted,
                    &auth_params_nonce,
                    Some(key_version as u32),
                )?;
                let auth_parameters: AuthParameters = serde_json::from_str(&auth_params_json)?;

                // Parse authorization_type
                let authorization_type_str: String = row.get("authorization_type");
                let authorization_type = serde_json::from_str(&format!("\"{}\"", authorization_type_str))?;

                // Parse optional JSON fields
                let invocation_http_parameters = self.parse_invocation_http_parameters(&row)?;
                let network_config = self.parse_optional_json(&row, "network_config_json")?;
                let timeout_config = self.parse_optional_json(&row, "timeout_config_json")?;
                let http_policy = self.parse_optional_json(&row, "http_policy_json")?;

                Ok(Some(ConnectionConfig {
                    trn: row.get("trn"),
                    name: row.get("name"),
                    authorization_type,
                    auth_parameters,
                    invocation_http_parameters,
                    auth_ref: row.get("auth_ref"),
                    network_config,
                    timeout_config,
                    http_policy,
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    version: row.get("version"),
                }))
            }
            None => Ok(None),
        }
    }

    /// List all connections with optional filtering and pagination
    pub async fn list(&self, authorization_type_filter: Option<&str>, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<ConnectionConfig>> {
        let query = if let Some(auth_type) = authorization_type_filter {
            sqlx::query(
                r#"
                SELECT trn, name, authorization_type, auth_params_encrypted, auth_params_nonce,
                       auth_ref,
                       default_headers_json, default_query_params_json, default_body_json,
                       network_config_json, timeout_config_json, http_policy_json,
                       key_version, created_at, updated_at, version
                FROM connections WHERE authorization_type = ?1
                ORDER BY created_at DESC
                LIMIT ?2 OFFSET ?3
                "#,
            )
            .bind(auth_type)
            .bind(limit.unwrap_or(100))
            .bind(offset.unwrap_or(0))
        } else {
            sqlx::query(
                r#"
                SELECT trn, name, authorization_type, auth_params_encrypted, auth_params_nonce,
                       auth_ref,
                       default_headers_json, default_query_params_json, default_body_json,
                       network_config_json, timeout_config_json, http_policy_json,
                       key_version, created_at, updated_at, version
                FROM connections
                ORDER BY created_at DESC
                LIMIT ?1 OFFSET ?2
                "#,
            )
            .bind(limit.unwrap_or(100))
            .bind(offset.unwrap_or(0))
        };

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to list connections: {}", e))?;

        let mut connections = Vec::new();
        for row in rows {
            let auth_params_encrypted: String = row.get("auth_params_encrypted");
            let auth_params_nonce: String = row.get("auth_params_nonce");
            let key_version: i64 = row.get("key_version");

            // Decrypt auth_parameters
            let auth_params_json = self.decrypt_field(
                &auth_params_encrypted,
                &auth_params_nonce,
                Some(key_version as u32),
            )?;
            let auth_parameters: AuthParameters = serde_json::from_str(&auth_params_json)?;

            // Parse authorization_type
            let authorization_type_str: String = row.get("authorization_type");
            let authorization_type = serde_json::from_str(&format!("\"{}\"", authorization_type_str))?;

            // Parse optional JSON fields
            let invocation_http_parameters = self.parse_invocation_http_parameters(&row)?;
            let network_config = self.parse_optional_json(&row, "network_config_json")?;
            let timeout_config = self.parse_optional_json(&row, "timeout_config_json")?;
            let http_policy = self.parse_optional_json(&row, "http_policy_json")?;

            connections.push(ConnectionConfig {
                trn: row.get("trn"),
                name: row.get("name"),
                authorization_type,
                auth_parameters,
                invocation_http_parameters,
                auth_ref: row.get("auth_ref"),
                network_config,
                timeout_config,
                http_policy,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            });
        }

        Ok(connections)
    }

    /// Delete a connection by TRN
    pub async fn delete(&self, trn: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM connections WHERE trn = ?1")
            .bind(trn)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to delete connection: {}", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// Count connections by authorization type
    pub async fn count_by_type(&self, authorization_type: Option<&str>) -> Result<i64> {
        let count = if let Some(auth_type) = authorization_type {
            sqlx::query_scalar("SELECT COUNT(*) FROM connections WHERE authorization_type = ?1")
                .bind(auth_type)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM connections")
                .fetch_one(&self.pool)
                .await?
        };

        Ok(count)
    }

    // Helper methods for encryption/decryption
    fn encrypt_field(&self, data: &str) -> Result<(String, String, i64)> {
        if let Some(ref encryption) = self.encryption {
            let encrypted = encryption.encrypt_field(data)?;
            Ok((encrypted.data, encrypted.nonce, encrypted.key_version as i64))
        } else {
            // Store directly without encryption (for development only)
            use base64::{engine::general_purpose::STANDARD, Engine};
            Ok((STANDARD.encode(data), "no-encryption".to_string(), 0))
        }
    }

    fn decrypt_field(&self, data: &str, nonce: &str, key_version: Option<u32>) -> Result<String> {
        if let Some(ref encryption) = self.encryption {
            let encrypted = EncryptedField {
                data: data.to_string(),
                nonce: nonce.to_string(),
                key_version: key_version.unwrap_or(1),
            };
            encryption.decrypt_field(&encrypted)
        } else {
            // Decode directly without encryption
            use base64::{engine::general_purpose::STANDARD, Engine};
            let decoded = STANDARD.decode(data)
                .map_err(|e| anyhow!("Failed to decode data: {}", e))?;
            String::from_utf8(decoded)
                .map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e))
        }
    }

    fn parse_invocation_http_parameters(&self, row: &sqlx::sqlite::SqliteRow) -> Result<Option<InvocationHttpParameters>> {
        use sqlx::Row;
        
        let headers_json: Option<String> = row.get("default_headers_json");
        let query_params_json: Option<String> = row.get("default_query_params_json");
        let body_json: Option<String> = row.get("default_body_json");

        if headers_json.is_none() && query_params_json.is_none() && body_json.is_none() {
            return Ok(None);
        }

        let header_parameters = headers_json
            .map(|j| serde_json::from_str(&j))
            .transpose()?
            .unwrap_or_default();

        let query_string_parameters = query_params_json
            .map(|j| serde_json::from_str(&j))
            .transpose()?
            .unwrap_or_default();

        let body_parameters = body_json
            .map(|j| serde_json::from_str(&j))
            .transpose()?
            .unwrap_or_default();

        Ok(Some(InvocationHttpParameters {
            header_parameters,
            query_string_parameters,
            body_parameters,
        }))
    }

    fn parse_optional_json<T>(&self, row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        use sqlx::Row;
        
        let json: Option<String> = row.get(column);
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthorizationType, ApiKeyAuthParameters};
    use tempfile::tempdir;

    async fn create_test_repo() -> (ConnectionRepository, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&database_url).await.unwrap();
        
        // Initialize the database schema
        sqlx::query(
            r#"
            CREATE TABLE connections (
                trn TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                authorization_type TEXT NOT NULL,
                auth_params_encrypted TEXT NOT NULL,
                auth_params_nonce TEXT NOT NULL,
                auth_ref TEXT,
                default_headers_json TEXT,
                default_query_params_json TEXT,
                default_body_json TEXT,
                network_config_json TEXT,
                timeout_config_json TEXT,
                http_policy_json TEXT,
                key_version INTEGER DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                version INTEGER DEFAULT 1
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo = ConnectionRepository::new(pool, None);
        (repo, temp_dir)
    }

    #[tokio::test]
    async fn test_connection_upsert_and_get() {
        let (repo, _temp_dir) = create_test_repo().await;

        let mut connection = ConnectionConfig::new(
            "trn:openact:test:connection/api-test@v1".to_string(),
            "Test API Connection".to_string(),
            AuthorizationType::ApiKey,
        );

        connection.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "secret123".to_string(),
        });

        // Test upsert
        repo.upsert(&connection).await.unwrap();

        // Test get
        let retrieved = repo.get_by_trn(&connection.trn).await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.trn, connection.trn);
        assert_eq!(retrieved.name, connection.name);
        assert_eq!(retrieved.authorization_type, connection.authorization_type);
        
        // Check that sensitive data was encrypted/decrypted correctly
        assert!(retrieved.auth_parameters.api_key_auth_parameters.is_some());
        let api_key_params = retrieved.auth_parameters.api_key_auth_parameters.unwrap();
        assert_eq!(api_key_params.api_key_value, "secret123");
    }

    #[tokio::test]
    async fn test_connection_list_and_delete() {
        let (repo, _temp_dir) = create_test_repo().await;

        // Create test connections
        let connection1 = ConnectionConfig::new(
            "trn:openact:test:connection/api1@v1".to_string(),
            "API Connection 1".to_string(),
            AuthorizationType::ApiKey,
        );

        let connection2 = ConnectionConfig::new(
            "trn:openact:test:connection/api2@v1".to_string(),
            "API Connection 2".to_string(),
            AuthorizationType::Basic,
        );

        repo.upsert(&connection1).await.unwrap();
        repo.upsert(&connection2).await.unwrap();

        // Test list all
        let all_connections = repo.list(None, None, None).await.unwrap();
        assert_eq!(all_connections.len(), 2);

        // Test list by type
        let api_key_connections = repo.list(Some("api_key"), None, None).await.unwrap();
        assert_eq!(api_key_connections.len(), 1);
        assert_eq!(api_key_connections[0].authorization_type, AuthorizationType::ApiKey);

        // Test count
        let total_count = repo.count_by_type(None).await.unwrap();
        assert_eq!(total_count, 2);

        let api_key_count = repo.count_by_type(Some("api_key")).await.unwrap();
        assert_eq!(api_key_count, 1);

        // Test delete
        let deleted = repo.delete(&connection1.trn).await.unwrap();
        assert!(deleted);

        let remaining = repo.list(None, None, None).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].trn, connection2.trn);
    }
}
