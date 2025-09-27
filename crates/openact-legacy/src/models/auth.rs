//! Authentication models
//!
//! This module contains data structures for managing authentication state,
//! particularly OAuth token information and runtime authentication data.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use crate::store::auth_trn::AuthConnectionTrn;

/// Authentication connection state, including tokens and metadata
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AuthConnection {
    /// TRN identifier
    pub trn: AuthConnectionTrn,
    /// Access token
    #[cfg_attr(feature = "openapi", schema(example = "***redacted***"))]
    pub access_token: String,
    /// Refresh token (optional)
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(example = "***redacted***"))]
    pub refresh_token: Option<String>,
    /// Token expiration time (ISO8601 format)
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Token type (usually "Bearer")
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Authorization scope
    #[serde(default)]
    pub scope: Option<String>,
    /// Additional metadata
    #[serde(default)]
    pub extra: Value,
    /// Creation time
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// Last update time
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

impl std::fmt::Debug for AuthConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConnection")
            .field("trn", &self.trn)
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("token_type", &self.token_type)
            .field("scope", &self.scope)
            .field("extra", &self.extra)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl AuthConnection {
    /// Create a new connection
    pub fn new(
        tenant: impl Into<String>,
        provider: impl Into<String>,
        user_id: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Result<Self> {
        let now = Utc::now();
        let trn = AuthConnectionTrn::new(tenant, provider, user_id)?;

        Ok(Self {
            trn,
            access_token: access_token.into(),
            refresh_token: None,
            expires_at: None,
            token_type: default_token_type(),
            scope: None,
            extra: Value::Null,
            created_at: now,
            updated_at: now,
        })
    }

    /// Create a new connection with full parameters
    pub fn new_with_params(
        tenant: impl Into<String>,
        provider: impl Into<String>,
        user_id: impl Into<String>,
        access_token: impl Into<String>,
        refresh_token: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        token_type: Option<String>,
        scope: Option<String>,
        extra: Option<Value>,
    ) -> Result<Self> {
        let now = Utc::now();
        let trn = AuthConnectionTrn::new(tenant, provider, user_id)?;

        Ok(Self {
            trn,
            access_token: access_token.into(),
            refresh_token,
            expires_at,
            token_type: token_type.unwrap_or_else(default_token_type),
            scope,
            extra: extra.unwrap_or(Value::Null),
            created_at: now,
            updated_at: now,
        })
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() >= expires_at
        } else {
            false // No expiration time means it doesn't expire
        }
    }

    /// Check if the token is about to expire (within given seconds)
    pub fn is_expiring_soon(&self, within_seconds: i64) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = Utc::now();
            let expiry_threshold = now + chrono::Duration::seconds(within_seconds);
            expires_at <= expiry_threshold
        } else {
            false
        }
    }

    /// Update the access token and expiration
    pub fn update_token(&mut self, access_token: String, expires_at: Option<DateTime<Utc>>) {
        self.access_token = access_token;
        self.expires_at = expires_at;
        self.updated_at = Utc::now();
    }

    /// Update only the access token (alias for compatibility)
    pub fn update_access_token(&mut self, access_token: String) {
        self.access_token = access_token;
        self.updated_at = Utc::now();
    }

    /// Update the refresh token
    pub fn update_refresh_token(&mut self, refresh_token: Option<String>) {
        self.refresh_token = refresh_token;
        self.updated_at = Utc::now();
    }

    /// Get the authorization header value
    pub fn authorization_header_value(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }

    /// Get a reference to the TRN as a string
    pub fn trn_string(&self) -> String {
        self.trn.to_string()
    }

    /// Get connection ID (alias for TRN string for compatibility)
    pub fn connection_id(&self) -> String {
        self.trn.to_string()
    }

    /// Update metadata
    pub fn update_metadata(&mut self, extra: Value) {
        self.extra = extra;
        self.updated_at = Utc::now();
    }

    /// Set expiration time (builder-style method for compatibility)
    pub fn with_expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self.updated_at = Utc::now();
        self
    }

    /// Set expiration time based on seconds from now (builder-style method for compatibility)
    pub fn with_expires_in(mut self, expires_in_seconds: i64) -> Self {
        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in_seconds);
        self.expires_at = Some(expires_at);
        self.updated_at = Utc::now();
        self
    }
}

impl Default for AuthConnection {
    fn default() -> Self {
        Self::new("default", "unknown", "unknown", "").unwrap_or_else(|_| {
            // If creation fails, create a minimal valid connection
            let trn = AuthConnectionTrn::new("default", "unknown", "unknown").unwrap();
            let now = Utc::now();
            Self {
                trn,
                access_token: String::new(),
                refresh_token: None,
                expires_at: None,
                token_type: default_token_type(),
                scope: None,
                extra: Value::Null,
                created_at: now,
                updated_at: now,
            }
        })
    }
}
