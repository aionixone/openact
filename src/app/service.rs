use anyhow::{Result, anyhow};
use std::sync::Arc;

use crate::executor::{ExecutionResult, Executor};
use crate::models::{ConnectionConfig, TaskConfig};
use crate::store::ConnectionStore; // bring trait for get/put on auth store into scope
use crate::store::service::StorageService;
use crate::templates::{TemplateInputs, TemplateLoader};

use crate::interface::dto::{AdhocExecuteRequestDto, ConnectionStatusDto, ExecuteOverridesDto};

pub struct OpenActService {
    storage: Arc<StorageService>,
    template_loader: TemplateLoader,
}

impl OpenActService {
    pub async fn from_env() -> Result<Self> {
        let templates_dir =
            std::env::var("OPENACT_TEMPLATES_DIR").unwrap_or_else(|_| "templates".to_string());
        Ok(Self {
            storage: StorageService::global().await,
            template_loader: TemplateLoader::new(templates_dir),
        })
    }

    pub fn from_storage(storage: Arc<StorageService>) -> Self {
        let templates_dir =
            std::env::var("OPENACT_TEMPLATES_DIR").unwrap_or_else(|_| "templates".to_string());
        Self {
            storage,
            template_loader: TemplateLoader::new(templates_dir),
        }
    }

    // Connections
    pub async fn upsert_connection(&self, c: &ConnectionConfig) -> Result<()> {
        self.storage.upsert_connection(c).await
    }
    pub async fn get_connection(&self, trn: &str) -> Result<Option<ConnectionConfig>> {
        self.storage.get_connection(trn).await
    }
    pub async fn list_connections(
        &self,
        auth_type: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ConnectionConfig>> {
        self.storage
            .list_connections(auth_type, limit, offset)
            .await
    }
    pub async fn delete_connection(&self, trn: &str) -> Result<bool> {
        self.storage.delete_connection(trn).await
    }

    // Tasks
    pub async fn upsert_task(&self, t: &TaskConfig) -> Result<()> {
        self.storage.upsert_task(t).await
    }
    pub async fn get_task(&self, trn: &str) -> Result<Option<TaskConfig>> {
        self.storage.get_task(trn).await
    }
    pub async fn list_tasks(
        &self,
        connection_trn: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskConfig>> {
        self.storage.list_tasks(connection_trn, limit, offset).await
    }
    pub async fn delete_task(&self, trn: &str) -> Result<bool> {
        self.storage.delete_task(trn).await
    }

    // Execute
    pub async fn execute_task(
        &self,
        task_trn: &str,
        overrides: Option<ExecuteOverridesDto>,
    ) -> Result<ExecutionResult> {
        let (conn, mut task) = self
            .storage
            .get_execution_context(task_trn)
            .await?
            .ok_or_else(|| anyhow!("Task not found: {}", task_trn))?;

        if let Some(ov) = overrides {
            if let Some(m) = ov.method {
                task.method = m;
            }
            if let Some(ep) = ov.endpoint {
                task.api_endpoint = ep;
            }
            if let Some(h) = ov.headers {
                let mut headers = task.headers.unwrap_or_default();
                for (k, vs) in h {
                    headers.insert(k, vs);
                }
                task.headers = Some(headers);
            }
            if let Some(q) = ov.query {
                let mut qs = task.query_params.unwrap_or_default();
                for (k, vs) in q {
                    qs.insert(k, vs);
                }
                task.query_params = Some(qs);
            }
            if let Some(b) = ov.body {
                task.request_body = Some(b);
            }
            if let Some(rp) = ov.retry_policy {
                task.retry_policy = Some(rp);
            }
        }

        let executor = Executor::new();
        executor.execute(&conn, &task).await
    }

    // Ad-hoc Execute
    /// Execute an ad-hoc action using an existing connection without persisting the task
    pub async fn execute_adhoc(&self, req: AdhocExecuteRequestDto) -> Result<ExecutionResult> {
        // Get the connection by TRN
        let connection = self
            .get_connection(&req.connection_trn)
            .await?
            .ok_or_else(|| anyhow!("Connection not found: {}", req.connection_trn))?;

        // Convert Vec<String> headers/query to MultiValue (HashMap<String, Vec<String>>)
        let headers = req.headers.map(|h| {
            h.into_iter().collect::<std::collections::HashMap<String, crate::models::common::MultiValue>>()
        });
        let query_params = req.query.map(|q| {
            q.into_iter().collect::<std::collections::HashMap<String, crate::models::common::MultiValue>>()
        });

        // Create a temporary TaskConfig for execution (not persisted)
        let temp_task = TaskConfig {
            trn: format!("ephemeral::{}", uuid::Uuid::new_v4()), // Temporary TRN
            name: "Ad-hoc execution".to_string(),
            connection_trn: req.connection_trn,
            api_endpoint: req.endpoint,
            method: req.method,
            headers,
            query_params,
            request_body: req.body,
            timeout_config: req.timeout_config,
            network_config: req.network_config,
            http_policy: req.http_policy,
            response_policy: req.response_policy,
            retry_policy: req.retry_policy,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
        };

        let executor = Executor::new();
        executor.execute(&connection, &temp_task).await
    }

    // System
    pub async fn stats(&self) -> Result<crate::store::service::StorageStats> {
        self.storage.get_stats().await
    }
    pub async fn get_stats(&self) -> Result<crate::store::service::StorageStats> {
        self.storage.get_stats().await
    }
    pub async fn cleanup(&self) -> Result<crate::store::service::CleanupResult> {
        self.storage.cleanup().await
    }
    pub async fn cache_stats(&self) -> Result<crate::store::service::CacheStats> {
        Ok(self.storage.get_cache_stats().await)
    }
    pub async fn get_cache_stats(&self) -> crate::store::service::CacheStats {
        self.storage.get_cache_stats().await
    }

    // Connection status (no network calls)
    pub async fn connection_status(&self, trn: &str) -> Result<Option<ConnectionStatusDto>> {
        use crate::models::connection::AuthorizationType;
        let conn = match self.storage.get_connection(trn).await? {
            Some(c) => c,
            None => return Ok(None),
        };
        let mut status = ConnectionStatusDto {
            trn: conn.trn.clone(),
            authorization_type: conn.authorization_type.clone(),
            has_auth_ref: None,
            status: "ready".to_string(),
            expires_at: None,
            seconds_to_expiry: None,
            message: None,
        };

        match conn.authorization_type {
            AuthorizationType::ApiKey | AuthorizationType::Basic => {
                // Consider ready if required parameters exist
                let ok = match conn.authorization_type {
                    AuthorizationType::ApiKey => conn
                        .auth_parameters
                        .api_key_auth_parameters
                        .as_ref()
                        .map(|p| !p.api_key_name.is_empty() && !p.api_key_value.is_empty())
                        .unwrap_or(false),
                    AuthorizationType::Basic => conn
                        .auth_parameters
                        .basic_auth_parameters
                        .as_ref()
                        .map(|p| !p.username.is_empty() && !p.password.is_empty())
                        .unwrap_or(false),
                    _ => false,
                };
                if !ok {
                    status.status = "misconfigured".to_string();
                    status.message = Some("missing required auth parameters".to_string());
                }
            }
            AuthorizationType::OAuth2ClientCredentials => {
                // Token stored under derived TRN; fetch if present to compute expiry
                use crate::utils::trn::{make_auth_cc_token_trn, parse_connection_trn};
                let (tenant, id) = parse_connection_trn(&conn.trn)?;
                let token_ref = make_auth_cc_token_trn(&tenant, &id);
                let store = self.storage.clone();
                let maybe = store.get(&token_ref).await?;
                if let Some(ac) = maybe {
                    status.expires_at = ac.expires_at;
                    if let Some(exp) = ac.expires_at {
                        let now = chrono::Utc::now();
                        status.seconds_to_expiry = Some((exp - now).num_seconds());
                        if exp <= now {
                            status.status = "expired".to_string();
                        } else if exp <= now + chrono::Duration::seconds(600) {
                            status.status = "expiring_soon".to_string();
                        } else {
                            status.status = "ready".to_string();
                        }
                    } else {
                        status.status = "ready".to_string();
                    }
                } else {
                    status.status = "not_issued".to_string();
                    status.message = Some("no client-credentials token found yet".to_string());
                }
            }
            AuthorizationType::OAuth2AuthorizationCode => {
                status.has_auth_ref = Some(conn.auth_ref.is_some());
                if let Some(ref auth_ref) = conn.auth_ref {
                    let maybe = self.storage.get(auth_ref).await?;
                    if let Some(ac) = maybe {
                        status.expires_at = ac.expires_at;
                        if let Some(exp) = ac.expires_at {
                            let now = chrono::Utc::now();
                            status.seconds_to_expiry = Some((exp - now).num_seconds());
                            if exp <= now {
                                status.status = "expired".to_string();
                            } else if exp <= now + chrono::Duration::seconds(600) {
                                status.status = "expiring_soon".to_string();
                            } else {
                                status.status = "ready".to_string();
                            }
                        } else {
                            // No expiry info - assume ready but cannot predict
                            status.status = "ready".to_string();
                        }
                    } else {
                        status.status = "not_authorized".to_string();
                        status.message =
                            Some("auth_ref bound but no token record in store".to_string());
                    }
                } else {
                    status.status = "unbound".to_string();
                    status.message = Some("no auth_ref bound; run OAuth flow or bind".to_string());
                }
            }
        }

        Ok(Some(status))
    }

    // Execution context
    pub async fn get_execution_context(
        &self,
        task_trn: &str,
    ) -> Result<Option<(crate::models::ConnectionConfig, crate::models::TaskConfig)>> {
        self.storage.get_execution_context(task_trn).await
    }

    // Direct storage access for advanced operations
    pub fn database(&self) -> &crate::store::DatabaseManager {
        self.storage.database()
    }

    /// Expose storage service for advanced operations (e.g., storing auth tokens)
    pub fn storage(&self) -> std::sync::Arc<StorageService> {
        self.storage.clone()
    }

    // Config
    pub async fn import(
        &self,
        connections: Vec<ConnectionConfig>,
        tasks: Vec<TaskConfig>,
    ) -> Result<(usize, usize)> {
        self.storage.import_configurations(connections, tasks).await
    }
    pub async fn import_configurations(
        &self,
        connections: Vec<ConnectionConfig>,
        tasks: Vec<TaskConfig>,
    ) -> Result<(usize, usize)> {
        self.storage.import_configurations(connections, tasks).await
    }
    pub async fn export(&self) -> Result<(Vec<ConnectionConfig>, Vec<TaskConfig>)> {
        self.storage.export_configurations().await
    }
    pub async fn export_configurations(&self) -> Result<(Vec<ConnectionConfig>, Vec<TaskConfig>)> {
        self.storage.export_configurations().await
    }

    // Templates
    /// Instantiate a connection template and register it in the database
    pub async fn instantiate_and_upsert_connection(
        &self,
        provider: &str,
        template_name: &str,
        tenant: &str,
        connection_name: &str,
        inputs: TemplateInputs,
    ) -> Result<ConnectionConfig> {
        // Load template
        let template = self
            .template_loader
            .load_connection_template(provider, template_name)?;

        // Instantiate to DTO
        let connection_dto = self.template_loader.instantiate_connection(
            &template,
            tenant,
            connection_name,
            &inputs,
        )?;

        // Check if connection already exists to get version info
        let existing = self.get_connection(&connection_dto.trn).await?;
        let (existing_version, existing_created_at) = match existing {
            Some(conn) => (Some(conn.version), Some(conn.created_at)),
            None => (None, None),
        };

        // Convert DTO to Config with proper metadata
        let connection_config = connection_dto.to_config(existing_version, existing_created_at);

        // Upsert using existing service
        self.upsert_connection(&connection_config).await?;

        tracing::info!(
            "Template connection registered: provider={}, template={}, trn={}",
            provider,
            template_name,
            connection_config.trn
        );

        Ok(connection_config)
    }

    /// Instantiate a task template and register it in the database
    pub async fn instantiate_and_upsert_task(
        &self,
        provider: &str,
        action: &str,
        tenant: &str,
        task_name: &str,
        connection_trn: &str,
        inputs: TemplateInputs,
    ) -> Result<TaskConfig> {
        // Load template
        let template = self.template_loader.load_task_template(provider, action)?;

        // Instantiate to DTO
        let task_dto = self.template_loader.instantiate_task(
            &template,
            tenant,
            task_name,
            connection_trn,
            &inputs,
        )?;

        // Check if task already exists to get version info
        let existing = self.get_task(&task_dto.trn).await?;
        let (existing_version, existing_created_at) = match existing {
            Some(task) => (Some(task.version), Some(task.created_at)),
            None => (None, None),
        };

        // Convert DTO to Config with proper metadata
        let task_config = task_dto.to_config(existing_version, existing_created_at);

        // Upsert using existing service
        self.upsert_task(&task_config).await?;

        tracing::info!(
            "Template task registered: provider={}, action={}, trn={}",
            provider,
            action,
            task_config.trn
        );

        Ok(task_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::connection::AuthorizationType;
    use crate::models::{ApiKeyAuthParameters, ConnectionConfig, OAuth2Parameters};
    use crate::store::ConnectionStore;
    use crate::store::DatabaseManager;
    use chrono::Utc;

    async fn make_service() -> OpenActService {
        let db = DatabaseManager::new("sqlite::memory:").await.unwrap();
        let storage = Arc::new(StorageService::new(db));
        OpenActService::from_storage(storage)
    }

    fn make_conn(trn: &str, name: &str, kind: AuthorizationType) -> ConnectionConfig {
        ConnectionConfig::new(trn.to_string(), name.to_string(), kind)
    }

    #[tokio::test]
    async fn status_api_key_ready_and_misconfigured() {
        let svc = make_service().await;
        // Ready
        let mut c1 = make_conn(
            "trn:openact:tenant:connection/ak1",
            "ak1",
            AuthorizationType::ApiKey,
        );
        c1.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "secret".to_string(),
        });
        svc.upsert_connection(&c1).await.unwrap();
        let s1 = svc.connection_status(&c1.trn).await.unwrap().unwrap();
        assert_eq!(s1.status, "ready");

        // Misconfigured
        let mut c2 = make_conn(
            "trn:openact:tenant:connection/ak2",
            "ak2",
            AuthorizationType::ApiKey,
        );
        c2.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "".to_string(),
            api_key_value: "".to_string(),
        });
        svc.upsert_connection(&c2).await.unwrap();
        let s2 = svc.connection_status(&c2.trn).await.unwrap().unwrap();
        assert_eq!(s2.status, "misconfigured");
    }

    #[tokio::test]
    async fn status_oauth2_ac_unbound_not_authorized_and_ready() {
        let svc = make_service().await;
        // Base AC connection (no auth_ref)
        let mut c = make_conn(
            "trn:openact:tenant:connection/ac1",
            "ac1",
            AuthorizationType::OAuth2AuthorizationCode,
        );
        c.auth_parameters.oauth_parameters = Some(OAuth2Parameters {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            token_url: "https://example.com/token".to_string(),
            scope: Some("read".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        svc.upsert_connection(&c).await.unwrap();

        // Unbound
        let s_unbound = svc.connection_status(&c.trn).await.unwrap().unwrap();
        assert_eq!(s_unbound.status, "unbound");

        // Bind auth_ref but no record -> not_authorized
        let mut c2 = c.clone();
        c2.trn = "trn:openact:tenant:connection/ac2".to_string();
        c2.auth_ref = Some("trn:openact:tenant:auth/oauth2_ac-user".to_string());
        svc.upsert_connection(&c2).await.unwrap();
        let s_na = svc.connection_status(&c2.trn).await.unwrap().unwrap();
        assert_eq!(s_na.status, "not_authorized");

        // Insert auth record with future expiry -> ready
        use crate::models::AuthConnection;
        let ac = AuthConnection::new_with_params(
            "tenant",
            "oauth2_ac",
            "user",
            "AT".to_string(),
            Some("RT".to_string()),
            Some(Utc::now() + chrono::Duration::seconds(3600)),
            Some("Bearer".to_string()),
            Some("read".to_string()),
            None,
        )
        .unwrap();
        // store under the auth_ref key using the same storage instance
        let _ = svc
            .storage
            .put("trn:openact:tenant:auth/oauth2_ac-user", &ac)
            .await;

        let s_ready = svc.connection_status(&c2.trn).await.unwrap().unwrap();
        assert_eq!(s_ready.status, "ready");
    }
}
