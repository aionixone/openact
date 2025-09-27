//! Authentication token management for connectors
//!
//! This module provides interfaces for fetching OAuth tokens from the auth_connections table
//! and handling token refresh logic.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// OAuth token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Access token
    pub access_token: String,
    /// Refresh token (optional)
    pub refresh_token: Option<String>,
    /// Token expiration time
    pub expires_at: Option<DateTime<Utc>>,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// Authorization scope
    pub scope: Option<String>,
    /// Additional metadata
    pub extra_data: Option<serde_json::Value>,
}

/// Auth connection record from auth_connections table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConnection {
    /// TRN identifier
    pub trn: String,
    /// Tenant
    pub tenant: String,
    /// Provider (e.g., "github", "oauth2")
    pub provider: String,
    /// User ID
    pub user_id: String,
    /// Encrypted access token
    pub access_token_encrypted: String,
    /// Nonce for access token
    pub access_token_nonce: String,
    /// Encrypted refresh token (optional)
    pub refresh_token_encrypted: Option<String>,
    /// Nonce for refresh token (optional)
    pub refresh_token_nonce: Option<String>,
    /// Token expiration time
    pub expires_at: Option<DateTime<Utc>>,
    /// Token type
    pub token_type: String,
    /// Authorization scope
    pub scope: Option<String>,
    /// Encrypted extra data
    pub extra_data_encrypted: Option<String>,
    /// Nonce for extra data
    pub extra_data_nonce: Option<String>,
    /// Key version for encryption
    pub key_version: i32,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// Version
    pub version: i64,
}

/// Abstract interface for auth connection storage
#[async_trait]
pub trait AuthConnectionStore: Send + Sync {
    /// Get an auth connection by TRN
    async fn get(&self, auth_ref: &str) -> anyhow::Result<Option<AuthConnection>>;
    
    /// Store or update an auth connection
    async fn put(&self, auth_ref: &str, connection: &AuthConnection) -> anyhow::Result<()>;
    
    /// Delete an auth connection
    async fn delete(&self, auth_ref: &str) -> anyhow::Result<bool>;
}

/// Token refresh outcome
#[derive(Debug)]
pub enum RefreshOutcome {
    /// Token was reused (still fresh)
    Reused(TokenInfo),
    /// Token was refreshed successfully
    Refreshed(TokenInfo),
    /// Token refresh failed
    Failed(String),
}

impl RefreshOutcome {
    /// Extract token info if available
    pub fn token_info(self) -> Option<TokenInfo> {
        match self {
            RefreshOutcome::Reused(info) | RefreshOutcome::Refreshed(info) => Some(info),
            RefreshOutcome::Failed(_) => None,
        }
    }
}
