//! OAuth2 token management for HTTP connector

use crate::auth::{AuthConnectionStore, RefreshOutcome, TokenInfo};
use crate::error::{ConnectorError, ConnectorResult};
use crate::http::connection::{AuthorizationType, OAuth2Parameters};
use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};

#[cfg(feature = "http")]
use oauth2::{
    basic::BasicClient, reqwest::http_client, AuthUrl, ClientId, ClientSecret, RefreshToken,
    TokenResponse, TokenUrl,
};

/// OAuth2 token manager
pub struct OAuth2TokenManager<'a> {
    auth_store: &'a dyn AuthConnectionStore,
}

impl<'a> OAuth2TokenManager<'a> {
    pub fn new(auth_store: &'a dyn AuthConnectionStore) -> Self {
        Self { auth_store }
    }

    /// Get or refresh OAuth2 access token
    pub async fn get_access_token(
        &self,
        auth_ref: &str,
        oauth_params: &OAuth2Parameters,
        auth_type: &AuthorizationType,
    ) -> ConnectorResult<TokenInfo> {
        // First, try to get existing token from store
        let auth_connection = self
            .auth_store
            .get(auth_ref)
            .await
            .map_err(|e| ConnectorError::Authentication(format!("Failed to fetch auth connection: {}", e)))?;

        if let Some(auth_conn) = auth_connection {
            // Check if token is still fresh (60 second buffer)
            if let Some(expires_at) = auth_conn.expires_at {
                if expires_at > Utc::now() + Duration::seconds(60) {
                    // Token is still fresh, decrypt and return
                    let token_info = self.decrypt_token_info(&auth_conn)?;
                    return Ok(token_info);
                }
            }

            // Token is expired or near expiry, try to refresh
            if let Some(_refresh_token_encrypted) = &auth_conn.refresh_token_encrypted {
                match self.refresh_token(&auth_conn, oauth_params).await {
                    Ok(RefreshOutcome::Refreshed(token_info)) => return Ok(token_info),
                    Ok(RefreshOutcome::Reused(token_info)) => return Ok(token_info),
                    Ok(RefreshOutcome::Failed(msg)) => {
                        return Err(ConnectorError::Authentication(format!(
                            "Token refresh failed: {}",
                            msg
                        )))
                    }
                    Err(e) => {
                        return Err(ConnectorError::Authentication(format!(
                            "Token refresh error: {}",
                            e
                        )))
                    }
                }
            }
        }

        // No existing token or refresh failed, try client credentials flow if applicable
        if matches!(auth_type, AuthorizationType::OAuth2ClientCredentials) {
            return self.client_credentials_flow(oauth_params).await;
        }

        // For authorization code flow, we need a valid auth_connection with refresh token
        Err(ConnectorError::Authentication(format!(
            "No valid OAuth2 token found for auth_ref: {}. Please complete OAuth flow first.",
            auth_ref
        )))
    }

    /// Perform OAuth2 client credentials flow
    pub async fn client_credentials_flow(&self, oauth_params: &OAuth2Parameters) -> ConnectorResult<TokenInfo> {
        #[cfg(feature = "http")]
        {
            let client = BasicClient::new(
                ClientId::new(oauth_params.client_id.clone()),
                Some(ClientSecret::new(oauth_params.client_secret.clone())),
                AuthUrl::new("https://invalid.example/auth".to_string()).expect("static"),
                oauth_params.token_url.as_ref().map(|url| 
                    TokenUrl::new(url.clone())
                        .map_err(|e| ConnectorError::InvalidConfig(format!("Invalid token URL: {}", e)))
                ).transpose()?,
            );

            let mut req = client.exchange_client_credentials();

            // Add scopes if specified
            if let Some(scope) = &oauth_params.scope {
                for scope_str in scope.split_whitespace() {
                    req = req.add_scope(oauth2::Scope::new(scope_str.to_string()));
                }
            }

            let token = req
                .request(http_client)
                .map_err(|e| ConnectorError::Authentication(format!("OAuth2 client credentials failed: {}", e)))?;

            let access_token = token.access_token().secret().to_string();
            let refresh_token = token.refresh_token().map(|t| t.secret().to_string());
            let expires_at = token
                .expires_in()
                .map(|d| Utc::now() + Duration::seconds(d.as_secs() as i64));

            Ok(TokenInfo {
                access_token,
                refresh_token,
                expires_at,
                token_type: token.token_type().as_ref().to_string(),
                scope: oauth_params.scope.clone(),
                extra_data: None,
            })
        }

        #[cfg(not(feature = "http"))]
        {
            Err(ConnectorError::InvalidConfig(
                "OAuth2 support requires http feature".to_string(),
            ))
        }
    }

    /// Refresh an existing OAuth2 token
    async fn refresh_token(
        &self,
        auth_conn: &crate::auth::AuthConnection,
        oauth_params: &OAuth2Parameters,
    ) -> anyhow::Result<RefreshOutcome> {
        // Decrypt refresh token (simplified - in real implementation, use proper decryption)
        let refresh_token = auth_conn
            .refresh_token_encrypted
            .as_ref()
            .ok_or_else(|| anyhow!("No refresh token available"))?;

        #[cfg(feature = "http")]
        {
            let client = BasicClient::new(
                ClientId::new(oauth_params.client_id.clone()),
                Some(ClientSecret::new(oauth_params.client_secret.clone())),
                AuthUrl::new("https://invalid.example/auth".to_string()).expect("static"),
                oauth_params.token_url.as_ref().map(|url| 
                    TokenUrl::new(url.clone())
                        .context("Invalid token URL")
                ).transpose()?,
            );

            let rt = RefreshToken::new(refresh_token.clone());
            let req = client.exchange_refresh_token(&rt);
            
            match req.request(http_client) {
                Ok(token) => {
                    let access_token = token.access_token().secret().to_string();
                    let new_refresh = token
                        .refresh_token()
                        .map(|t| t.secret().to_string())
                        .or_else(|| Some(refresh_token.clone())); // Keep old refresh token if none returned
                    let expires_at = token
                        .expires_in()
                        .map(|d| Utc::now() + Duration::seconds(d.as_secs() as i64));

                    let token_info = TokenInfo {
                        access_token: access_token.clone(),
                        refresh_token: new_refresh.clone(),
                        expires_at,
                        token_type: token.token_type().as_ref().to_string(),
                        scope: auth_conn.scope.clone(),
                        extra_data: None,
                    };

                    // TODO: Update auth_connection in store with new token
                    // This requires implementing encryption/decryption logic

                    Ok(RefreshOutcome::Refreshed(token_info))
                }
                Err(e) => Ok(RefreshOutcome::Failed(format!("Token refresh failed: {}", e))),
            }
        }

        #[cfg(not(feature = "http"))]
        {
            Ok(RefreshOutcome::Failed(
                "OAuth2 support requires http feature".to_string(),
            ))
        }
    }

    /// Decrypt token info from auth connection (simplified implementation)
    fn decrypt_token_info(&self, auth_conn: &crate::auth::AuthConnection) -> ConnectorResult<TokenInfo> {
        // TODO: Implement proper decryption
        // For now, assume tokens are base64 encoded (simplified)
        let access_token = auth_conn.access_token_encrypted.clone();
        let refresh_token = auth_conn.refresh_token_encrypted.clone();

        Ok(TokenInfo {
            access_token,
            refresh_token,
            expires_at: auth_conn.expires_at,
            token_type: auth_conn.token_type.clone(),
            scope: auth_conn.scope.clone(),
            extra_data: None,
        })
    }
}

/// Mock auth connection store for testing
#[cfg(test)]
pub struct MockAuthStore {
    connections: std::collections::HashMap<String, crate::auth::AuthConnection>,
}

#[cfg(test)]
impl MockAuthStore {
    pub fn new() -> Self {
        Self {
            connections: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, auth_ref: String, conn: crate::auth::AuthConnection) {
        self.connections.insert(auth_ref, conn);
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl AuthConnectionStore for MockAuthStore {
    async fn get(&self, auth_ref: &str) -> anyhow::Result<Option<crate::auth::AuthConnection>> {
        Ok(self.connections.get(auth_ref).cloned())
    }

    async fn put(&self, _auth_ref: &str, _connection: &crate::auth::AuthConnection) -> anyhow::Result<()> {
        // MockAuthStore is immutable for simplicity
        Ok(())
    }

    async fn delete(&self, _auth_ref: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_oauth2_manager_fresh_token() {
        let mut store = MockAuthStore::new();
        let future_time = Utc::now() + Duration::hours(1);
        
        let auth_conn = crate::auth::AuthConnection {
            trn: "test-trn".to_string(),
            tenant: "test".to_string(),
            provider: "github".to_string(),
            user_id: "user1".to_string(),
            access_token_encrypted: "fresh-token".to_string(),
            access_token_nonce: "nonce".to_string(),
            refresh_token_encrypted: Some("refresh-token".to_string()),
            refresh_token_nonce: Some("nonce".to_string()),
            expires_at: Some(future_time),
            token_type: "Bearer".to_string(),
            scope: Some("read:user".to_string()),
            extra_data_encrypted: None,
            extra_data_nonce: None,
            key_version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        store.insert("test-auth-ref".to_string(), auth_conn);

        let manager = OAuth2TokenManager::new(&store);
        let oauth_params = OAuth2Parameters {
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            auth_url: Some("https://github.com/login/oauth/authorize".to_string()),
            token_url: Some("https://github.com/login/oauth/access_token".to_string()),
            scope: Some("read:user".to_string()),
            redirect_uri: None,
            use_pkce: Some(false),
        };

        let result = manager
            .get_access_token("test-auth-ref", &oauth_params, &AuthorizationType::OAuth2AuthorizationCode)
            .await;

        assert!(result.is_ok());
        let token_info = result.unwrap();
        assert_eq!(token_info.access_token, "fresh-token");
        assert_eq!(token_info.refresh_token, Some("refresh-token".to_string()));
    }
}
