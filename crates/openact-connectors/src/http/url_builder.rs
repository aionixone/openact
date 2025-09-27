//! URL building utilities for HTTP connector

use crate::error::{ConnectorError, ConnectorResult};
use url::Url;

/// URL builder that handles proper URL joining and encoding
pub struct UrlBuilder;

impl UrlBuilder {
    /// Join base URL with a path, handling slashes and encoding properly
    /// 
    /// Examples:
    /// - `join("https://api.example.com", "/users")` -> `https://api.example.com/users`
    /// - `join("https://api.example.com/", "users")` -> `https://api.example.com/users`
    /// - `join("https://api.example.com/v1", "/users")` -> `https://api.example.com/users`
    /// - `join("https://api.example.com/v1/", "users")` -> `https://api.example.com/v1/users`
    /// - `join("https://api.example.com", "users/{id}")` -> `https://api.example.com/users/%7Bid%7D`
    pub fn join(base_url: &str, path: &str) -> ConnectorResult<String> {
        // Parse the base URL
        let mut base = Url::parse(base_url)
            .map_err(|e| ConnectorError::InvalidConfig(format!("Invalid base URL '{}': {}", base_url, e)))?;

        // Handle path joining
        if path.is_empty() {
            return Ok(base.to_string());
        }

        // Use url::Url::join for proper path handling
        let result = if path.starts_with('/') {
            // Absolute path - replace the base path completely
            base.join(path)
        } else {
            // Relative path - append to existing path
            // Ensure base path ends with '/' for proper joining
            let base_path = base.path();
            if !base_path.ends_with('/') {
                base.set_path(&format!("{}/", base_path));
            }
            base.join(path)
        }.map_err(|e| ConnectorError::InvalidConfig(format!("Failed to join URL '{}' with path '{}': {}", base_url, path, e)))?;

        Ok(result.to_string())
    }

    /// Join base URL with path and apply query parameters
    pub fn join_with_query(
        base_url: &str,
        path: &str,
        query_params: &std::collections::HashMap<String, String>,
    ) -> ConnectorResult<String> {
        let mut url = Url::parse(&Self::join(base_url, path)?)
            .map_err(|e| ConnectorError::InvalidConfig(format!("Invalid joined URL: {}", e)))?;

        // Add query parameters
        {
            let mut query_pairs = url.query_pairs_mut();
            for (key, value) in query_params {
                query_pairs.append_pair(key, value);
            }
        }

        Ok(url.to_string())
    }

    /// Validate that a URL is well-formed
    pub fn validate(url: &str) -> ConnectorResult<()> {
        Url::parse(url)
            .map_err(|e| ConnectorError::InvalidConfig(format!("Invalid URL '{}': {}", url, e)))?;
        Ok(())
    }

    /// Extract components from a URL for debugging/logging
    pub fn parse_components(url: &str) -> ConnectorResult<UrlComponents> {
        let parsed = Url::parse(url)
            .map_err(|e| ConnectorError::InvalidConfig(format!("Invalid URL '{}': {}", url, e)))?;

        Ok(UrlComponents {
            scheme: parsed.scheme().to_string(),
            host: parsed.host_str().map(|s| s.to_string()),
            port: parsed.port(),
            path: parsed.path().to_string(),
            query: parsed.query().map(|s| s.to_string()),
            fragment: parsed.fragment().map(|s| s.to_string()),
        })
    }
}

/// URL components for debugging and analysis
#[derive(Debug, Clone, PartialEq)]
pub struct UrlComponents {
    pub scheme: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_url_joining() {
        // Basic cases
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "/users").unwrap(),
            "https://api.example.com/users"
        );

        assert_eq!(
            UrlBuilder::join("https://api.example.com/", "users").unwrap(),
            "https://api.example.com/users"
        );

        assert_eq!(
            UrlBuilder::join("https://api.example.com", "users").unwrap(),
            "https://api.example.com/users"
        );
    }

    #[test]
    fn test_path_with_base_path() {
        // Base URL with existing path
        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1", "/users").unwrap(),
            "https://api.example.com/users"  // Absolute path replaces /v1
        );

        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1/", "users").unwrap(),
            "https://api.example.com/v1/users"  // Relative path appends to /v1/
        );

        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1", "users").unwrap(),
            "https://api.example.com/v1/users"  // Relative path appends to /v1
        );
    }

    #[test]
    fn test_complex_paths() {
        // Path with parameters (should be encoded)
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "users/{id}").unwrap(),
            "https://api.example.com/users/%7Bid%7D"
        );

        // Path with query-like content (gets parsed as query)
        // Note: Url::join treats this as a URL with query, not as a path to be encoded
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "search?q=test").unwrap(),
            "https://api.example.com/search?q=test"
        );

        // Multiple path segments
        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1", "users/123/profile").unwrap(),
            "https://api.example.com/v1/users/123/profile"
        );
    }

    #[test]
    fn test_edge_cases() {
        // Empty path
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "").unwrap(),
            "https://api.example.com/"
        );

        // Root path
        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1/resource", "/").unwrap(),
            "https://api.example.com/"
        );

        // Path with special characters
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "path with spaces").unwrap(),
            "https://api.example.com/path%20with%20spaces"
        );
    }

    #[test]
    fn test_with_query_parameters() {
        let mut params = std::collections::HashMap::new();
        params.insert("q".to_string(), "search term".to_string());
        params.insert("page".to_string(), "1".to_string());

        let result = UrlBuilder::join_with_query("https://api.example.com", "/search", &params).unwrap();
        
        // The order of query parameters may vary, so check that both are present
        assert!(result.starts_with("https://api.example.com/search?"));
        // URL encoding replaces space with + in query parameters
        assert!(result.contains("q=search+term"));
        assert!(result.contains("page=1"));
    }

    #[test]
    fn test_invalid_urls() {
        // Invalid base URL
        assert!(UrlBuilder::join("not-a-url", "/path").is_err());

        // Invalid characters that can't be fixed
        assert!(UrlBuilder::join("https://", "/path").is_err());
    }

    #[test]
    fn test_url_validation() {
        assert!(UrlBuilder::validate("https://api.example.com/path").is_ok());
        assert!(UrlBuilder::validate("http://localhost:8080").is_ok());
        assert!(UrlBuilder::validate("not-a-url").is_err());
        assert!(UrlBuilder::validate("").is_err());
    }

    #[test]
    fn test_url_components() {
        let components = UrlBuilder::parse_components("https://api.example.com:8080/v1/users?page=1#section").unwrap();
        
        assert_eq!(components.scheme, "https");
        assert_eq!(components.host, Some("api.example.com".to_string()));
        assert_eq!(components.port, Some(8080));
        assert_eq!(components.path, "/v1/users");
        assert_eq!(components.query, Some("page=1".to_string()));
        assert_eq!(components.fragment, Some("section".to_string()));
    }

    #[test]
    fn test_github_api_examples() {
        // GitHub API base URL examples
        assert_eq!(
            UrlBuilder::join("https://api.github.com", "/user").unwrap(),
            "https://api.github.com/user"
        );

        assert_eq!(
            UrlBuilder::join("https://api.github.com", "/repos/{owner}/{repo}").unwrap(),
            "https://api.github.com/repos/%7Bowner%7D/%7Brepo%7D"
        );

        assert_eq!(
            UrlBuilder::join("https://api.github.com/", "user/repos").unwrap(),
            "https://api.github.com/user/repos"
        );
    }

    #[test]
    fn test_enterprise_api_examples() {
        // Enterprise API with base path
        assert_eq!(
            UrlBuilder::join("https://company.example.com/api/v2", "/users").unwrap(),
            "https://company.example.com/users"  // Absolute path
        );

        assert_eq!(
            UrlBuilder::join("https://company.example.com/api/v2", "users").unwrap(),
            "https://company.example.com/api/v2/users"  // Relative path
        );

        assert_eq!(
            UrlBuilder::join("https://company.example.com/api/v2/", "users/profile").unwrap(),
            "https://company.example.com/api/v2/users/profile"
        );
    }
}
