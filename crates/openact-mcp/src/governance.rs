//! Governance and security controls for MCP server

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Governance configuration for MCP server
#[derive(Debug, Clone)]
pub struct GovernanceConfig {
    /// Allowed tool patterns (e.g., ["http.*", "postgres.query"])
    pub allow_patterns: Vec<String>,
    /// Denied tool patterns (e.g., ["http.delete", "*.admin"])
    pub deny_patterns: Vec<String>,
    /// Maximum concurrent executions
    pub max_concurrency: usize,
    /// Tool execution timeout
    pub timeout: Duration,
    /// Semaphore for concurrency control
    pub concurrency_limiter: Arc<Semaphore>,
}

impl GovernanceConfig {
    /// Create new governance configuration
    pub fn new(
        allow_patterns: Vec<String>,
        deny_patterns: Vec<String>,
        max_concurrency: usize,
        timeout_secs: u64,
    ) -> Self {
        Self {
            allow_patterns,
            deny_patterns,
            max_concurrency,
            timeout: Duration::from_secs(timeout_secs),
            concurrency_limiter: Arc::new(Semaphore::new(max_concurrency)),
        }
    }

    /// Check if a tool is allowed by governance policies
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // If allow patterns are specified, tool must match at least one
        if !self.allow_patterns.is_empty() {
            let allowed =
                self.allow_patterns.iter().any(|pattern| self.matches_pattern(tool_name, pattern));
            if !allowed {
                return false;
            }
        }

        // Tool must not match any deny pattern
        let denied =
            self.deny_patterns.iter().any(|pattern| self.matches_pattern(tool_name, pattern));

        !denied
    }

    /// Simple pattern matching with wildcard support
    /// Supports patterns like "http.*", "*.admin", "exact.match"
    fn matches_pattern(&self, tool_name: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern == tool_name {
            return true;
        }

        if pattern.ends_with(".*") {
            let prefix = &pattern[..pattern.len() - 1]; // Remove "*", keep "."
            return tool_name.starts_with(prefix);
        }

        if pattern.starts_with("*.") {
            let suffix = &pattern[1..]; // Remove "*", keep "."
            return tool_name.ends_with(suffix);
        }

        false
    }
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self::new(vec![], vec![], 10, 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        let config = GovernanceConfig::default();

        // Exact match
        assert!(config.matches_pattern("http.get", "http.get"));
        assert!(!config.matches_pattern("http.post", "http.get"));

        // Prefix wildcard
        assert!(config.matches_pattern("http.get", "http.*"));
        assert!(config.matches_pattern("http.post", "http.*"));
        assert!(!config.matches_pattern("postgres.query", "http.*"));

        // Suffix wildcard
        assert!(config.matches_pattern("http.admin", "*.admin"));
        assert!(config.matches_pattern("postgres.admin", "*.admin"));
        assert!(!config.matches_pattern("http.get", "*.admin"));

        // Universal wildcard
        assert!(config.matches_pattern("anything", "*"));
    }

    #[test]
    fn test_allow_patterns() {
        let config = GovernanceConfig::new(
            vec!["http.*".to_string(), "postgres.query".to_string()],
            vec![],
            10,
            30,
        );

        // Allowed patterns
        assert!(config.is_tool_allowed("http.get"));
        assert!(config.is_tool_allowed("http.post"));
        assert!(config.is_tool_allowed("postgres.query"));

        // Not explicitly allowed
        assert!(!config.is_tool_allowed("postgres.delete"));
        assert!(!config.is_tool_allowed("redis.set"));
    }

    #[test]
    fn test_deny_patterns() {
        let config = GovernanceConfig::new(
            vec![], // No allow patterns = allow all
            vec!["*.delete".to_string(), "http.admin".to_string()],
            10,
            30,
        );

        // Allowed (not denied)
        assert!(config.is_tool_allowed("http.get"));
        assert!(config.is_tool_allowed("postgres.query"));

        // Denied
        assert!(!config.is_tool_allowed("http.delete"));
        assert!(!config.is_tool_allowed("postgres.delete"));
        assert!(!config.is_tool_allowed("http.admin"));
    }

    #[test]
    fn test_allow_and_deny_combined() {
        let config =
            GovernanceConfig::new(vec!["http.*".to_string()], vec!["*.delete".to_string()], 10, 30);

        // Allowed and not denied
        assert!(config.is_tool_allowed("http.get"));
        assert!(config.is_tool_allowed("http.post"));

        // Allowed but denied
        assert!(!config.is_tool_allowed("http.delete"));

        // Not allowed
        assert!(!config.is_tool_allowed("postgres.query"));
    }
}
