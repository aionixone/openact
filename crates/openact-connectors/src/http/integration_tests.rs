//! Integration tests for HTTP executor with URL building

#[cfg(test)]
mod tests {
    use crate::http::url_builder::UrlBuilder;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_url_building_real_world_scenarios() {
        // Test scenarios that would commonly occur in real HTTP actions

        // GitHub API patterns
        assert_eq!(
            UrlBuilder::join("https://api.github.com", "/user").unwrap(),
            "https://api.github.com/user"
        );

        assert_eq!(
            UrlBuilder::join("https://api.github.com", "/repos/owner/repo").unwrap(),
            "https://api.github.com/repos/owner/repo"
        );

        // Enterprise API with base path
        assert_eq!(
            UrlBuilder::join("https://company.example.com/api/v2", "users").unwrap(),
            "https://company.example.com/api/v2/users"
        );

        // Absolute path overwrites base path
        assert_eq!(
            UrlBuilder::join("https://company.example.com/api/v2", "/webhooks").unwrap(),
            "https://company.example.com/webhooks"
        );

        // Complex nested paths
        assert_eq!(
            UrlBuilder::join("https://api.slack.com/api", "conversations.history").unwrap(),
            "https://api.slack.com/api/conversations.history"
        );

        // Path with parameters (should be encoded)
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "users/{userId}/posts/{postId}").unwrap(),
            "https://api.example.com/users/%7BuserId%7D/posts/%7BpostId%7D"
        );
    }

    #[tokio::test]
    async fn test_url_with_query_parameters() {
        let mut params = HashMap::new();
        params.insert("per_page".to_string(), "50".to_string());
        params.insert("page".to_string(), "1".to_string());
        params.insert("sort".to_string(), "created".to_string());

        let result = UrlBuilder::join_with_query(
            "https://api.github.com", 
            "/repos/owner/repo/issues", 
            &params
        ).unwrap();

        // Check that all parameters are present (order may vary)
        assert!(result.starts_with("https://api.github.com/repos/owner/repo/issues?"));
        assert!(result.contains("per_page=50"));
        assert!(result.contains("page=1"));
        assert!(result.contains("sort=created"));
    }

    #[tokio::test]
    async fn test_common_api_patterns() {
        // REST API CRUD operations
        let base_url = "https://api.example.com/v1";
        
        // GET /users
        assert_eq!(
            UrlBuilder::join(base_url, "users").unwrap(),
            "https://api.example.com/v1/users"
        );

        // GET /users/123
        assert_eq!(
            UrlBuilder::join(base_url, "users/123").unwrap(),
            "https://api.example.com/v1/users/123"
        );

        // POST /users/123/posts
        assert_eq!(
            UrlBuilder::join(base_url, "users/123/posts").unwrap(),
            "https://api.example.com/v1/users/123/posts"
        );

        // Webhook endpoints (absolute paths)
        assert_eq!(
            UrlBuilder::join(base_url, "/webhooks/github").unwrap(),
            "https://api.example.com/webhooks/github"
        );

        // Health check endpoint
        assert_eq!(
            UrlBuilder::join(base_url, "/health").unwrap(),
            "https://api.example.com/health"
        );
    }

    #[tokio::test]
    async fn test_problematic_url_combinations() {
        // These are common mistakes that the URL builder should handle gracefully

        // Double slashes in base URL
        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1/", "/users").unwrap(),
            "https://api.example.com/users"  // Absolute path replaces /v1/
        );

        // Missing slash in base URL with relative path
        assert_eq!(
            UrlBuilder::join("https://api.example.com/v1", "users").unwrap(),
            "https://api.example.com/v1/users"
        );

        // Base URL ending with slash, path starting with slash
        assert_eq!(
            UrlBuilder::join("https://api.example.com/", "/users").unwrap(),
            "https://api.example.com/users"
        );

        // Special characters that need encoding
        assert_eq!(
            UrlBuilder::join("https://api.example.com", "search/projects with spaces").unwrap(),
            "https://api.example.com/search/projects%20with%20spaces"
        );
    }

    #[tokio::test]
    async fn test_error_scenarios() {
        // Test that invalid URLs are properly rejected

        // Invalid base URL
        assert!(UrlBuilder::join("not-a-url", "/path").is_err());
        
        // Empty base URL
        assert!(UrlBuilder::join("", "/path").is_err());
        
        // Base URL without scheme
        assert!(UrlBuilder::join("example.com", "/path").is_err());
    }
}