use crate::encryption::{EncryptedField, FieldEncryption};
use crate::error::{Result, StorageError};
use crate::models::{AuthConnection, OpenActConnection, OpenActTask};
use crate::pool::DbPool;
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use sqlx::Row;
use base64::{engine::general_purpose::STANDARD, Engine as _};

pub struct AuthConnectionRepository {
    pub pool: DbPool,
    pub encryption: Option<FieldEncryption>,
}

impl AuthConnectionRepository {
    pub fn new(pool: DbPool, encryption: Option<FieldEncryption>) -> Self {
        Self { pool, encryption }
    }

    fn encrypt(&self, plaintext: &str) -> Result<EncryptedField> {
        if let Some(enc) = &self.encryption {
            Ok(enc.encrypt_field(plaintext)?)
        } else {
            Ok(EncryptedField {
                data: STANDARD.encode(plaintext),
                nonce: STANDARD.encode("no-encryption"),
                key_version: 0,
            })
        }
    }
    fn decrypt(&self, ef: &EncryptedField) -> Result<String> {
        if let Some(enc) = &self.encryption {
            Ok(enc.decrypt_field(ef)?)
        } else {
            if ef.key_version == 0 {
                let bytes = STANDARD.decode(&ef.data)?;
                Ok(String::from_utf8(bytes)?)
            } else {
                Err(anyhow!("cannot decrypt without encryption feature").into())
            }
        }
    }

    pub async fn get_by_trn(&self, trn: &str) -> Result<Option<AuthConnection>> {
        let row = sqlx::query("SELECT * FROM auth_connections WHERE trn = ?")
            .bind(trn)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            let access_token_encrypted: String = row.try_get("access_token_encrypted")?;
            let access_token_nonce: String = row.try_get("access_token_nonce")?;
            let key_version: i64 = row.try_get("key_version").unwrap_or(1);
            let access_token = self.decrypt(&EncryptedField {
                data: access_token_encrypted,
                nonce: access_token_nonce,
                key_version: key_version as u32,
            })?;

            let refresh_token = match (
                row.try_get::<String, _>("refresh_token_encrypted"),
                row.try_get::<String, _>("refresh_token_nonce"),
            ) {
                (Ok(data), Ok(nonce)) if !data.is_empty() && !nonce.is_empty() => {
                    let token = self.decrypt(&EncryptedField {
                        data,
                        nonce,
                        key_version: key_version as u32,
                    })?;
                    Some(token)
                }
                _ => None,
            };

            let extra = match (
                row.try_get::<String, _>("extra_data_encrypted"),
                row.try_get::<String, _>("extra_data_nonce"),
            ) {
                (Ok(data), Ok(nonce)) if !data.is_empty() && !nonce.is_empty() => {
                    let json = self.decrypt(&EncryptedField {
                        data,
                        nonce,
                        key_version: key_version as u32,
                    })?;
                    serde_json::from_str(&json).unwrap_or(serde_json::Value::Null)
                }
                _ => serde_json::Value::Null,
            };

            let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at").ok();

            let conn = AuthConnection {
                tenant: row.try_get("tenant")?,
                provider: row.try_get("provider")?,
                user_id: row.try_get("user_id")?,
                access_token,
                refresh_token,
                expires_at,
                token_type: row
                    .try_get("token_type")
                    .unwrap_or_else(|_| "Bearer".to_string()),
                scope: row.try_get("scope").ok(),
                extra,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            Ok(Some(conn))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert(&self, trn: &str, conn: &AuthConnection) -> Result<()> {
        let at = self.encrypt(&conn.access_token)?;
        let (rt_data, rt_nonce) = if let Some(rt) = &conn.refresh_token {
            let ef = self.encrypt(rt)?;
            (Some(ef.data), Some(ef.nonce))
        } else {
            (None, None)
        };
        let (extra_data, extra_nonce) = if conn.extra != serde_json::Value::Null {
            let json = serde_json::to_string(&conn.extra)?;
            let ef = self.encrypt(&json)?;
            (Some(ef.data), Some(ef.nonce))
        } else {
            (None, None)
        };
        let key_version_val: i64 = if self.encryption.is_some() { 1 } else { 0 };

        let existing: i64 =
            sqlx::query_scalar("SELECT COUNT(1) FROM auth_connections WHERE trn = ?")
                .bind(trn)
                .fetch_one(&self.pool)
                .await?;
        if existing > 0 {
            sqlx::query(r#"UPDATE auth_connections SET access_token_encrypted=?, access_token_nonce=?, refresh_token_encrypted=?, refresh_token_nonce=?, expires_at=?, token_type=?, scope=?, extra_data_encrypted=?, extra_data_nonce=?, key_version=?, updated_at=CURRENT_TIMESTAMP, version=version+1 WHERE trn=?"#)
                .bind(at.data).bind(at.nonce)
                .bind(rt_data).bind(rt_nonce)
                .bind(conn.expires_at)
                .bind(&conn.token_type)
                .bind(&conn.scope)
                .bind(extra_data).bind(extra_nonce)
                .bind(key_version_val)
                .bind(trn)
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query(r#"INSERT INTO auth_connections (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce, refresh_token_encrypted, refresh_token_nonce, expires_at, token_type, scope, extra_data_encrypted, extra_data_nonce, key_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#)
                .bind(trn)
                .bind(&conn.tenant)
                .bind(&conn.provider)
                .bind(&conn.user_id)
                .bind(at.data).bind(at.nonce)
                .bind(rt_data).bind(rt_nonce)
                .bind(conn.expires_at)
                .bind(&conn.token_type)
                .bind(&conn.scope)
                .bind(extra_data).bind(extra_nonce)
                .bind(key_version_val)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn delete(&self, trn: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM auth_connections WHERE trn = ?")
            .bind(trn)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_refs(&self) -> Result<Vec<String>> {
        let refs = sqlx::query_scalar::<_, String>("SELECT trn FROM auth_connections ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(refs)
    }

    pub async fn cleanup_expired(&self) -> Result<usize> {
        let res = sqlx::query("DELETE FROM auth_connections WHERE expires_at IS NOT NULL AND expires_at < CURRENT_TIMESTAMP")
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() as usize)
    }

    pub async fn count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_connections").fetch_one(&self.pool).await?;
        Ok(count as usize)
    }
}

pub struct OpenActConnectionRepository {
    pub pool: DbPool,
    pub encryption: Option<FieldEncryption>,
}

impl OpenActConnectionRepository {
    pub fn new(pool: DbPool, encryption: Option<FieldEncryption>) -> Self {
        Self { pool, encryption }
    }

    fn encrypt(&self, plaintext: &str) -> Result<EncryptedField> {
        if let Some(enc) = &self.encryption {
            Ok(enc.encrypt_field(plaintext)?)
        } else {
            Ok(EncryptedField {
                data: STANDARD.encode(plaintext),
                nonce: STANDARD.encode("no-encryption"),
                key_version: 0,
            })
        }
    }

    fn decrypt(&self, ef: &EncryptedField) -> Result<String> {
        if let Some(enc) = &self.encryption {
            Ok(enc.decrypt_field(ef)?)
        } else {
            if ef.key_version == 0 {
                let bytes = STANDARD.decode(&ef.data)?;
                Ok(String::from_utf8(bytes)?)
            } else {
                Err(anyhow!("cannot decrypt without encryption feature").into())
            }
        }
    }

    pub async fn get(&self, trn: &str) -> Result<Option<OpenActConnection>> {
        let row = sqlx::query("SELECT * FROM openact_connections WHERE trn = ?")
            .bind(trn)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            let key_version: i64 = row.try_get("key_version").unwrap_or(1);
            let (secrets_encrypted, secrets_nonce): (Option<String>, Option<String>) = (
                row.try_get("secrets_encrypted").ok(),
                row.try_get("secrets_nonce").ok(),
            );
            let secrets_encrypted = secrets_encrypted;
            let secrets_nonce = secrets_nonce;

            let secrets_decrypted: Option<String> = match (secrets_encrypted, secrets_nonce) {
                (Some(data), Some(nonce)) if !data.is_empty() && !nonce.is_empty() => {
                    Some(self.decrypt(&EncryptedField {
                        data,
                        nonce,
                        key_version: key_version as u32,
                    })?)
                }
                _ => None,
            };

            let conn = OpenActConnection {
                trn: row.try_get("trn")?,
                tenant: row.try_get("tenant")?,
                provider: row.try_get("provider")?,
                name: row.try_get("name").ok(),
                auth_kind: row.try_get("auth_kind")?,
                auth_ref: row.try_get("auth_ref").ok(),
                network_config_json: row.try_get("network_config_json").ok(),
                tls_config_json: row.try_get("tls_config_json").ok(),
                http_policy_json: row.try_get("http_policy_json").ok(),
                default_headers_json: row.try_get("default_headers_json").ok(),
                default_query_params_json: row.try_get("default_query_params_json").ok(),
                default_body_json: row.try_get("default_body_json").ok(),
                secrets_encrypted: secrets_decrypted,
                secrets_nonce: None,
                key_version: key_version as i32,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                version: row.try_get("version").unwrap_or(1),
            };
            Ok(Some(conn))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert(&self, conn: &OpenActConnection) -> Result<()> {
        // Encrypt secrets if present
        let (secrets_data, secrets_nonce, key_version_val) = if let Some(plain) = &conn.secrets_encrypted {
            let ef = self.encrypt(plain)?;
            (Some(ef.data), Some(ef.nonce), if self.encryption.is_some() { 1_i64 } else { 0_i64 })
        } else {
            (None, None, if self.encryption.is_some() { 1_i64 } else { 0_i64 })
        };

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM openact_connections WHERE trn = ?")
            .bind(&conn.trn)
            .fetch_one(&self.pool)
            .await?;
        if exists > 0 {
            // Optimistic lock when version > 0
            let mut q = String::from(
                "UPDATE openact_connections SET tenant=?, provider=?, name=?, auth_kind=?, auth_ref=?, network_config_json=?, tls_config_json=?, http_policy_json=?, default_headers_json=?, default_query_params_json=?, default_body_json=?, secrets_encrypted=?, secrets_nonce=?, key_version=?, updated_at=CURRENT_TIMESTAMP, version=version+1 WHERE trn=?",
            );
            let mut query = sqlx::query(&q)
                .bind(&conn.tenant)
                .bind(&conn.provider)
                .bind(&conn.name)
                .bind(&conn.auth_kind)
                .bind(&conn.auth_ref)
                .bind(&conn.network_config_json)
                .bind(&conn.tls_config_json)
                .bind(&conn.http_policy_json)
                .bind(&conn.default_headers_json)
                .bind(&conn.default_query_params_json)
                .bind(&conn.default_body_json)
                .bind(secrets_data.clone())
                .bind(secrets_nonce.clone())
                .bind(key_version_val)
                .bind(&conn.trn);

            let res = if conn.version > 0 {
                q.push_str(" AND version = ?");
                query = sqlx::query(&q)
                    .bind(&conn.tenant)
                    .bind(&conn.provider)
                    .bind(&conn.name)
                    .bind(&conn.auth_kind)
                    .bind(&conn.auth_ref)
                    .bind(&conn.network_config_json)
                    .bind(&conn.tls_config_json)
                    .bind(&conn.http_policy_json)
                    .bind(&conn.default_headers_json)
                    .bind(&conn.default_query_params_json)
                    .bind(&conn.default_body_json)
                    .bind(secrets_data)
                    .bind(secrets_nonce)
                    .bind(key_version_val)
                    .bind(&conn.trn)
                    .bind(conn.version);
                query.execute(&self.pool).await?
            } else {
                query.execute(&self.pool).await?
            };

            if conn.version > 0 && res.rows_affected() == 0 {
                return Err(StorageError::Other(anyhow!("version conflict for trn {}", conn.trn)));
            }
        } else {
            sqlx::query(r#"INSERT INTO openact_connections (trn, tenant, provider, name, auth_kind, auth_ref, network_config_json, tls_config_json, http_policy_json, default_headers_json, default_query_params_json, default_body_json, secrets_encrypted, secrets_nonce, key_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#)
                .bind(&conn.trn)
                .bind(&conn.tenant)
                .bind(&conn.provider)
                .bind(&conn.name)
                .bind(&conn.auth_kind)
                .bind(&conn.auth_ref)
                .bind(&conn.network_config_json)
                .bind(&conn.tls_config_json)
                .bind(&conn.http_policy_json)
                .bind(&conn.default_headers_json)
                .bind(&conn.default_query_params_json)
                .bind(&conn.default_body_json)
                .bind(secrets_data)
                .bind(secrets_nonce)
                .bind(key_version_val)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn delete(&self, trn: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM openact_connections WHERE trn = ?")
            .bind(trn)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_by_tenant(&self, tenant: &str) -> Result<Vec<OpenActConnection>> {
        let rows = sqlx::query("SELECT * FROM openact_connections WHERE tenant = ? ORDER BY updated_at DESC")
            .bind(tenant)
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let key_version: i64 = row.try_get("key_version").unwrap_or(1);
            let (se_data, se_nonce): (Option<String>, Option<String>) = (
                row.try_get("secrets_encrypted").ok(),
                row.try_get("secrets_nonce").ok(),
            );
            let secrets_decrypted = match (se_data, se_nonce) {
                (Some(data), Some(nonce)) if !data.is_empty() && !nonce.is_empty() => {
                    Some(self.decrypt(&EncryptedField { data, nonce, key_version: key_version as u32 })?)
                }
                _ => None,
            };

            out.push(OpenActConnection {
                trn: row.try_get("trn")?,
                tenant: row.try_get("tenant")?,
                provider: row.try_get("provider")?,
                name: row.try_get("name").ok(),
                auth_kind: row.try_get("auth_kind")?,
                auth_ref: row.try_get("auth_ref").ok(),
                network_config_json: row.try_get("network_config_json").ok(),
                tls_config_json: row.try_get("tls_config_json").ok(),
                http_policy_json: row.try_get("http_policy_json").ok(),
                default_headers_json: row.try_get("default_headers_json").ok(),
                default_query_params_json: row.try_get("default_query_params_json").ok(),
                default_body_json: row.try_get("default_body_json").ok(),
                secrets_encrypted: secrets_decrypted,
                secrets_nonce: None,
                key_version: key_version as i32,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                version: row.try_get("version").unwrap_or(1),
            });
        }
        Ok(out)
    }

    pub async fn count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM openact_connections")
            .fetch_one(&self.pool)
            .await?;
        Ok(count as usize)
    }
}

pub struct OpenActTaskRepository {
    pub pool: DbPool,
}

impl OpenActTaskRepository {
    pub fn new(pool: DbPool) -> Self { Self { pool } }

    pub async fn get(&self, trn: &str) -> Result<Option<OpenActTask>> {
        let row = sqlx::query("SELECT * FROM openact_tasks WHERE trn = ?")
            .bind(trn)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            Ok(Some(OpenActTask {
                trn: row.try_get("trn")?,
                tenant: row.try_get("tenant")?,
                connection_trn: row.try_get("connection_trn")?,
                api_endpoint: row.try_get("api_endpoint")?,
                method: row.try_get("method")?,
                headers_json: row.try_get("headers_json").ok(),
                query_params_json: row.try_get("query_params_json").ok(),
                request_body_json: row.try_get("request_body_json").ok(),
                pagination_json: row.try_get("pagination_json").ok(),
                http_policy_json: row.try_get("http_policy_json").ok(),
                response_policy_json: row.try_get("response_policy_json").ok(),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                version: row.try_get("version").unwrap_or(1),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert(&self, task: &OpenActTask) -> Result<()> {
        // FK validation: ensure connection exists
        let exists_conn: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM openact_connections WHERE trn = ?")
            .bind(&task.connection_trn)
            .fetch_one(&self.pool)
            .await?;
        if exists_conn == 0 {
            return Err(StorageError::Other(anyhow!("connection_trn not found: {}", task.connection_trn)));
        }

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM openact_tasks WHERE trn = ?")
            .bind(&task.trn)
            .fetch_one(&self.pool)
            .await?;
        if exists > 0 {
            let mut q = String::from("UPDATE openact_tasks SET tenant=?, connection_trn=?, api_endpoint=?, method=?, headers_json=?, query_params_json=?, request_body_json=?, pagination_json=?, http_policy_json=?, response_policy_json=?, updated_at=CURRENT_TIMESTAMP, version=version+1 WHERE trn=?");
            let mut query = sqlx::query(&q)
                .bind(&task.tenant)
                .bind(&task.connection_trn)
                .bind(&task.api_endpoint)
                .bind(&task.method)
                .bind(&task.headers_json)
                .bind(&task.query_params_json)
                .bind(&task.request_body_json)
                .bind(&task.pagination_json)
                .bind(&task.http_policy_json)
                .bind(&task.response_policy_json)
                .bind(&task.trn);
            let res = if task.version > 0 {
                q.push_str(" AND version = ?");
                query = sqlx::query(&q)
                    .bind(&task.tenant)
                    .bind(&task.connection_trn)
                    .bind(&task.api_endpoint)
                    .bind(&task.method)
                    .bind(&task.headers_json)
                    .bind(&task.query_params_json)
                    .bind(&task.request_body_json)
                    .bind(&task.pagination_json)
                    .bind(&task.http_policy_json)
                    .bind(&task.response_policy_json)
                    .bind(&task.trn)
                    .bind(task.version);
                query.execute(&self.pool).await?
            } else {
                query.execute(&self.pool).await?
            };
            if task.version > 0 && res.rows_affected() == 0 {
                return Err(StorageError::Other(anyhow!("version conflict for task {}", task.trn)));
            }
        } else {
            sqlx::query(r#"INSERT INTO openact_tasks (trn, tenant, connection_trn, api_endpoint, method, headers_json, query_params_json, request_body_json, pagination_json, http_policy_json, response_policy_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#)
                .bind(&task.trn)
                .bind(&task.tenant)
                .bind(&task.connection_trn)
                .bind(&task.api_endpoint)
                .bind(&task.method)
                .bind(&task.headers_json)
                .bind(&task.query_params_json)
                .bind(&task.request_body_json)
                .bind(&task.pagination_json)
                .bind(&task.http_policy_json)
                .bind(&task.response_policy_json)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn delete(&self, trn: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM openact_tasks WHERE trn = ?")
            .bind(trn)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_by_connection(&self, connection_trn: &str) -> Result<Vec<OpenActTask>> {
        let rows = sqlx::query("SELECT * FROM openact_tasks WHERE connection_trn = ? ORDER BY updated_at DESC")
            .bind(connection_trn)
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(OpenActTask {
                trn: row.try_get("trn")?,
                tenant: row.try_get("tenant")?,
                connection_trn: row.try_get("connection_trn")?,
                api_endpoint: row.try_get("api_endpoint")?,
                method: row.try_get("method")?,
                headers_json: row.try_get("headers_json").ok(),
                query_params_json: row.try_get("query_params_json").ok(),
                request_body_json: row.try_get("request_body_json").ok(),
                pagination_json: row.try_get("pagination_json").ok(),
                http_policy_json: row.try_get("http_policy_json").ok(),
                response_policy_json: row.try_get("response_policy_json").ok(),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                version: row.try_get("version").unwrap_or(1),
            });
        }
        Ok(out)
    }

    pub async fn count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM openact_tasks")
            .fetch_one(&self.pool)
            .await?;
        Ok(count as usize)
    }
}
