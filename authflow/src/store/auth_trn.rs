//! AuthFlow TRN Identifier Definitions
//!
//! Defines standardized TRN format for the AuthFlow authentication system

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use trn_rust::{Trn, TrnBuilder};

/// AuthFlow Connection TRN
/// Format: trn:authflow:tenant:connection/provider-user_id
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthConnectionTrn {
    /// Tenant identifier
    pub tenant: String,
    /// Provider name (github, slack, google, etc.)
    pub provider: String,
    /// User identifier (may contain special characters, needs encoding)
    pub user_id: String,
    /// Metadata (version, environment, region, etc.)
    pub metadata: HashMap<String, String>,
}

impl AuthConnectionTrn {
    /// Create a new authentication connection TRN
    pub fn new(
        tenant: impl Into<String>,
        provider: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Result<Self> {
        let tenant = tenant.into();
        let provider = provider.into();
        let user_id = user_id.into();

        // Validate input
        Self::validate_component(&tenant, "tenant")?;
        Self::validate_component(&provider, "provider")?;
        Self::validate_user_id(&user_id)?;

        Ok(Self {
            tenant,
            provider,
            user_id,
            metadata: HashMap::new(),
        })
    }

    /// Create an authentication connection TRN with metadata
    pub fn with_metadata(
        tenant: impl Into<String>,
        provider: impl Into<String>,
        user_id: impl Into<String>,
        metadata: HashMap<String, String>,
    ) -> Result<Self> {
        let mut trn = Self::new(tenant, provider, user_id)?;
        trn.metadata = metadata;
        Ok(trn)
    }

    /// Parse from TRN string
    pub fn parse(trn_str: &str) -> Result<Self> {
        let trn = Trn::parse(trn_str).map_err(|e| anyhow!("Failed to parse TRN: {}", e))?;

        // Validate tool type
        if trn.tool() != "authflow" {
            return Err(anyhow!("Expected tool 'authflow', got '{}'", trn.tool()));
        }

        // Validate resource type
        if trn.resource_type() != "connection" {
            return Err(anyhow!(
                "Expected resource type 'connection', got '{}'",
                trn.resource_type()
            ));
        }

        // Parse resource ID (provider-user_id)
        let resource_id = trn.resource_id();
        let (provider, user_id) = Self::parse_resource_id(resource_id)?;

        Ok(Self {
            tenant: trn.tenant().to_string(),
            provider,
            user_id,
            metadata: trn.metadata().clone(),
        })
    }

    /// Convert to TRN string
    pub fn to_trn_string(&self) -> Result<String> {
        let resource_id = self.encode_resource_id();

        let trn = if self.metadata.is_empty() {
            TrnBuilder::new()
                .tool("authflow")
                .tenant(&self.tenant)
                .resource_type("connection")
                .resource_id(&resource_id)
                .build()
                .map_err(|e| anyhow!("Failed to build TRN: {}", e))?
        } else {
            Trn::with_metadata(
                "authflow",
                &self.tenant,
                "connection",
                &resource_id,
                self.metadata.clone(),
            )
            .map_err(|e| anyhow!("Failed to build TRN with metadata: {}", e))?
        };

        Ok(trn.to_string())
    }

    /// Get the unique key for the connection (used as a database primary key)
    pub fn connection_key(&self) -> String {
        format!("{}:{}", self.provider, self.encode_user_id())
    }

    /// Set version metadata
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.metadata.insert("version".to_string(), version.into());
        self
    }

    /// Set environment metadata
    pub fn with_environment(mut self, env: impl Into<String>) -> Self {
        self.metadata.insert("env".to_string(), env.into());
        self
    }

    /// Set region metadata
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.metadata.insert("region".to_string(), region.into());
        self
    }

    /// Encode resource ID (provider-user_id)
    fn encode_resource_id(&self) -> String {
        format!("{}-{}", self.provider, self.encode_user_id())
    }

    /// Encode user ID (only use [A-Za-z0-9.-], other bytes encoded as _hh_)
    fn encode_user_id(&self) -> String {
        let mut out = String::new();
        for b in self.user_id.as_bytes() {
            let c = *b as char;
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                out.push(c);
            } else {
                out.push('_');
                out.push_str(&format!("{:02x}", b));
                out.push('_');
            }
        }
        out
    }

    /// Decode user ID (restore _hh_ to corresponding byte)
    fn decode_user_id(encoded: &str) -> Result<String> {
        let bytes = encoded.as_bytes();
        let mut i = 0usize;
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        while i < bytes.len() {
            if bytes[i] == b'_' {
                if i + 3 < bytes.len() && bytes[i + 3] == b'_' {
                    let h1 = bytes[i + 1] as char;
                    let h2 = bytes[i + 2] as char;
                    let v = u8::from_str_radix(&format!("{}{}", h1, h2), 16)
                        .map_err(|_| anyhow!("Failed to decode user ID: invalid hex"))?;
                    out.push(v);
                    i += 4;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8(out).map_err(|e| anyhow!("Failed to decode user ID: {}", e))
    }

    /// Parse resource ID
    fn parse_resource_id(resource_id: &str) -> Result<(String, String)> {
        let parts: Vec<&str> = resource_id.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid resource ID format, expected 'provider-user_id'"
            ));
        }

        let provider = parts[0].to_string();
        let encoded_user_id = parts[1];
        let user_id = Self::decode_user_id(encoded_user_id)?;

        Ok((provider, user_id))
    }

    /// Validate component
    fn validate_component(component: &str, name: &str) -> Result<()> {
        if component.is_empty() {
            return Err(anyhow!("{} cannot be empty", name));
        }

        if component.contains(':') || component.contains('/') {
            return Err(anyhow!("{} cannot contain ':' or '/' characters", name));
        }

        Ok(())
    }

    /// Validate user ID
    fn validate_user_id(user_id: &str) -> Result<()> {
        if user_id.is_empty() {
            return Err(anyhow!("user_id cannot be empty"));
        }

        // User ID can contain special characters but will be encoded
        Ok(())
    }
}

impl fmt::Display for AuthConnectionTrn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_trn_string() {
            Ok(trn_str) => write!(f, "{}", trn_str),
            Err(_) => write!(f, "invalid-trn"),
        }
    }
}

/// AuthFlow Session TRN
/// Format: trn:authflow:tenant:session:connection_id:session_id
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionTrn {
    pub tenant: String,
    pub connection_id: String,
    pub session_id: String,
}

impl AuthSessionTrn {
    /// Create a new session TRN
    pub fn new(
        tenant: impl Into<String>,
        connection_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Result<Self> {
        let tenant = tenant.into();
        let connection_id = connection_id.into();
        let session_id = session_id.into();

        // Validate input
        if tenant.is_empty() || connection_id.is_empty() || session_id.is_empty() {
            return Err(anyhow!(
                "tenant, connection_id, and session_id cannot be empty"
            ));
        }

        Ok(Self {
            tenant,
            connection_id,
            session_id,
        })
    }

    /// Convert to TRN string
    pub fn to_trn_string(&self) -> String {
        format!(
            "trn:authflow:{}:session:{}:{}",
            self.tenant, self.connection_id, self.session_id
        )
    }

    /// Parse from TRN string
    pub fn parse(trn_str: &str) -> Result<Self> {
        let parts: Vec<&str> = trn_str.split(':').collect();
        if parts.len() != 6 {
            return Err(anyhow!("Invalid session TRN format"));
        }

        if parts[0] != "trn" || parts[1] != "authflow" || parts[3] != "session" {
            return Err(anyhow!("Invalid session TRN format"));
        }

        Ok(Self {
            tenant: parts[2].to_string(),
            connection_id: parts[4].to_string(),
            session_id: parts[5].to_string(),
        })
    }
}

impl fmt::Display for AuthSessionTrn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_trn_string())
    }
}

/// AuthFlow Execution TRN
/// Format: trn:authflow:tenant:execution:flow_name:execution_id
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthExecutionTrn {
    pub tenant: String,
    pub flow_name: String,
    pub execution_id: String,
}

impl AuthExecutionTrn {
    /// Create a new execution TRN
    pub fn new(
        tenant: impl Into<String>,
        flow_name: impl Into<String>,
        execution_id: impl Into<String>,
    ) -> Result<Self> {
        let tenant = tenant.into();
        let flow_name = flow_name.into();
        let execution_id = execution_id.into();

        // Validate input
        if tenant.is_empty() || flow_name.is_empty() || execution_id.is_empty() {
            return Err(anyhow!(
                "tenant, flow_name, and execution_id cannot be empty"
            ));
        }

        Ok(Self {
            tenant,
            flow_name,
            execution_id,
        })
    }

    /// Convert to TRN string
    pub fn to_trn_string(&self) -> String {
        format!(
            "trn:authflow:{}:execution:{}:{}",
            self.tenant, self.flow_name, self.execution_id
        )
    }

    /// Parse from TRN string
    pub fn parse(trn_str: &str) -> Result<Self> {
        let parts: Vec<&str> = trn_str.split(':').collect();
        if parts.len() != 6 {
            return Err(anyhow!("Invalid execution TRN format"));
        }

        if parts[0] != "trn" || parts[1] != "authflow" || parts[3] != "execution" {
            return Err(anyhow!("Invalid execution TRN format"));
        }

        Ok(Self {
            tenant: parts[2].to_string(),
            flow_name: parts[4].to_string(),
            execution_id: parts[5].to_string(),
        })
    }
}

impl fmt::Display for AuthExecutionTrn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_trn_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_connection_trn() {
        // Basic test
        let trn = AuthConnectionTrn::new("company123", "github", "user456").unwrap();
        let trn_str = trn.to_trn_string().unwrap();
        assert_eq!(trn_str, "trn:authflow:company123:connection/github-user456");

        // Parse test
        let parsed = AuthConnectionTrn::parse(&trn_str).unwrap();
        assert_eq!(parsed.tenant, "company123");
        assert_eq!(parsed.provider, "github");
        assert_eq!(parsed.user_id, "user456");

        // Special character test
        let trn_special =
            AuthConnectionTrn::new("company123", "google", "user@example.com").unwrap();
        let trn_str_special = trn_special.to_trn_string().unwrap();
        let parsed_special = AuthConnectionTrn::parse(&trn_str_special).unwrap();
        assert_eq!(parsed_special.user_id, "user@example.com");
    }

    #[test]
    fn test_auth_connection_trn_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("version".to_string(), "v2".to_string());
        metadata.insert("env".to_string(), "prod".to_string());

        let trn =
            AuthConnectionTrn::with_metadata("company123", "slack", "team789-user123", metadata)
                .unwrap();
        let trn_str = trn.to_trn_string().unwrap();

        let parsed = AuthConnectionTrn::parse(&trn_str).unwrap();
        assert_eq!(parsed.metadata.get("version"), Some(&"v2".to_string()));
        assert_eq!(parsed.metadata.get("env"), Some(&"prod".to_string()));
    }

    #[test]
    fn test_auth_session_trn() {
        let trn = AuthSessionTrn::new("company123", "github-user456", "sess_abc123def456").unwrap();
        let trn_str = trn.to_trn_string();
        assert_eq!(
            trn_str,
            "trn:authflow:company123:session:github-user456:sess_abc123def456"
        );

        let parsed = AuthSessionTrn::parse(&trn_str).unwrap();
        assert_eq!(parsed.tenant, "company123");
        assert_eq!(parsed.connection_id, "github-user456");
        assert_eq!(parsed.session_id, "sess_abc123def456");
    }

    #[test]
    fn test_auth_execution_trn() {
        let trn =
            AuthExecutionTrn::new("company123", "github-oauth2", "exec_xyz789abc123").unwrap();
        let trn_str = trn.to_trn_string();
        assert_eq!(
            trn_str,
            "trn:authflow:company123:execution:github-oauth2:exec_xyz789abc123"
        );

        let parsed = AuthExecutionTrn::parse(&trn_str).unwrap();
        assert_eq!(parsed.tenant, "company123");
        assert_eq!(parsed.flow_name, "github-oauth2");
        assert_eq!(parsed.execution_id, "exec_xyz789abc123");
    }

    #[test]
    fn test_connection_key() {
        let trn = AuthConnectionTrn::new("company123", "github", "user@example.com").unwrap();
        let key = trn.connection_key();
        assert!(key.contains("github:"));
        assert!(key.contains("user_40_example.com"));
    }
}
