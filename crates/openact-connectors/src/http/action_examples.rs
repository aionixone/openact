//! HTTP Action usage examples and patterns

#[cfg(test)]
mod examples {
    use crate::http::actions::HttpAction;
    use crate::http::connection::{HttpConnection, AuthorizationType, TimeoutConfig, RetryPolicy, HttpPolicy};
    use crate::http::executor::HttpExecutor;
    use serde_json::json;
    use std::collections::HashMap;

    /// Demonstrates basic GET request action
    #[tokio::test]
    async fn example_get_user_profile() {
        println!("\n=== GET User Profile Example ===\n");

        // 1. Create Connection (represents GitHub API)
        let connection = HttpConnection::new(
            "https://api.github.com".to_string(),
            AuthorizationType::ApiKey,
        );
        
        // Set up default headers through invocation_http_parameters
        // Note: In real usage, headers would be set via invocation_http_parameters
        // For demo purposes, we'll just show the structure

        // 2. Create Action (get specific user)
        let mut action = HttpAction::new("GET".to_string(), "/user".to_string());
        
        // Add action-specific headers
        let mut action_headers = HashMap::new();
        action_headers.insert("Authorization".to_string(), vec!["token ghp_xxxxxxxxxxxx".to_string()]);
        action.headers = Some(action_headers);

        println!("📋 Action Configuration:");
        println!("  Method: {}", action.method);
        println!("  Path: {}", action.path);
        println!("  Headers: {:?}", action.headers);

        // 3. Execute Action
        let _executor = HttpExecutor::new();
        
        // In real usage, this would make an HTTP request
        // For demo, we just show the action structure
        println!("\n⚙️ Action Ready for Execution:");
        println!("  Connection: {}", connection.base_url);
        println!("  Action: {} {}", action.method, action.path);
        
        // Demo: would execute as: GET https://api.github.com/user
        println!("  Final URL: {}{}", connection.base_url, action.path);
    }

    /// Demonstrates POST request with body
    #[tokio::test]
    async fn example_create_issue() {
        println!("\n=== POST Create Issue Example ===\n");

        // 1. GitHub API Connection
        let connection = HttpConnection::new(
            "https://api.github.com".to_string(),
            AuthorizationType::ApiKey,
        );

        // 2. Create Issue Action
        let mut action = HttpAction::new("POST".to_string(), "/repos/owner/repo/issues".to_string());
        
        // Set request body
        action.request_body = Some(json!({
            "title": "Bug: Application crashes on startup",
            "body": "Detailed description of the issue...",
            "labels": ["bug", "priority-high"],
            "assignees": ["developer1"]
        }));

        // Set content type
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), vec!["application/json".to_string()]);
        action.headers = Some(headers);

        println!("📝 Create Issue Action:");
        println!("  Method: {}", action.method);
        println!("  Path: {}", action.path);
        println!("  Body: {}", action.request_body.as_ref().unwrap());

        let _executor = HttpExecutor::new();
        
        println!("\n✅ Ready to Execute:");
        println!("  URL: {}{}", connection.base_url, action.path);
        println!("  Has Body: {}", action.request_body.is_some());
    }

    /// Demonstrates action with custom retry policy
    #[tokio::test]
    async fn example_with_custom_retry() {
        println!("\n=== Action with Custom Retry Policy ===\n");

        let _connection = HttpConnection::new(
            "https://api.unreliable-service.com".to_string(),
            AuthorizationType::ApiKey,
        );

        // Action with aggressive retry for critical operation
        let mut action = HttpAction::new("POST".to_string(), "/critical-operation".to_string());
        
        // Custom retry policy for this specific action
        action.retry_policy = Some(RetryPolicy {
            max_retries: 5,           // More retries for critical operation
            initial_delay_ms: 2000,   // Start with 2 seconds
            max_delay_ms: 60000,      // Up to 1 minute
            backoff_multiplier: 2.0,
            retry_on_status_codes: vec![408, 429, 500, 502, 503, 504],
        });

        // Custom timeout for this action
        action.timeout_config = Some(TimeoutConfig {
            connect_ms: 10000,
            read_ms: 120000,  // 2 minutes for critical operation
            total_ms: 180000, // 3 minutes total
        });

        println!("⚙️ Critical Operation Action:");
        println!("  Max Retries: {}", action.retry_policy.as_ref().unwrap().max_retries);
        println!("  Read Timeout: {}ms", action.timeout_config.as_ref().unwrap().read_ms);

        let _executor = HttpExecutor::new();
        
        println!("\n🔧 Action Configuration:");
        println!("  Total Timeout: {}ms", action.timeout_config.as_ref().unwrap().total_ms);
        println!("  Max Retries: {}", action.retry_policy.as_ref().unwrap().max_retries);
    }

    /// Demonstrates action with custom HTTP policy
    #[tokio::test]
    async fn example_with_http_policy() {
        println!("\n=== Action with Custom HTTP Policy ===\n");

        // Connection with strict policy
        let mut connection = HttpConnection::new(
            "https://secure-api.com".to_string(),
            AuthorizationType::ApiKey,
        );
        
        connection.http_policy = Some(HttpPolicy {
            denied_headers: vec!["x-debug".to_string(), "x-internal".to_string()],
            reserved_headers: vec!["authorization".to_string()],
            multi_value_append_headers: vec!["accept".to_string()],
            drop_forbidden_headers: true,
            normalize_header_names: true,
            max_header_value_length: 1000,
            max_total_headers: 20,
            allowed_content_types: vec!["application/json".to_string()],
        });

        // Action tries to set forbidden header
        let mut action = HttpAction::new("GET".to_string(), "/secure-data".to_string());
        
        let mut headers = HashMap::new();
        headers.insert("X-Debug".to_string(), vec!["true".to_string()]);     // Will be denied
        headers.insert("X-Custom".to_string(), vec!["allowed".to_string()]); // Will be allowed
        headers.insert("Accept".to_string(), vec!["text/plain".to_string()]); // Will be appended
        action.headers = Some(headers);

        println!("🛡️ Secure Action with Policy:");
        println!("  Attempted Headers: X-Debug, X-Custom, Accept");

        let _executor = HttpExecutor::new();
        
        println!("\n🔍 Policy Will Apply:");
        println!("  Attempted Headers: {:?}", action.headers.as_ref().unwrap().keys().collect::<Vec<_>>());
        println!("  Note: X-Debug header will be filtered out by policy");
    }

    /// Demonstrates query parameter handling
    #[tokio::test]
    async fn example_with_query_params() {
        println!("\n=== Action with Query Parameters ===\n");

        let connection = HttpConnection::new(
            "https://api.example.com".to_string(),
            AuthorizationType::ApiKey,
        );

        // Search action with query parameters
        let mut action = HttpAction::new("GET".to_string(), "/search".to_string());
        
        let mut query_params = HashMap::new();
        query_params.insert("q".to_string(), vec!["rust programming".to_string()]);
        query_params.insert("sort".to_string(), vec!["stars".to_string()]);
        query_params.insert("order".to_string(), vec!["desc".to_string()]);
        query_params.insert("per_page".to_string(), vec!["50".to_string()]);
        action.query_params = Some(query_params);

        println!("🔍 Search Action:");
        println!("  Query: {:?}", action.query_params);

        let _executor = HttpExecutor::new();
        
        println!("\n🌐 Action with Query Params:");
        println!("  Base URL: {}{}", connection.base_url, action.path);
        println!("  Query Params:");
        if let Some(params) = &action.query_params {
            for (key, values) in params {
                if let Some(value) = values.first() {
                    println!("    {}={}", key, value);
                }
            }
        }
    }

    /// Demonstrates real-world API patterns
    #[tokio::test]
    async fn example_real_world_patterns() {
        println!("\n=== Real-World API Patterns ===\n");

        // 1. List Resources (GET with pagination)
        println!("1️⃣ List Users (Paginated):");
        let _list_action = HttpAction::new("GET".to_string(), "/users".to_string());
        println!("   GET /users?page=1&limit=20");

        // 2. Get Resource by ID (GET with path param)
        println!("\n2️⃣ Get User by ID:");
        let _get_action = HttpAction::new("GET".to_string(), "/users/123".to_string());
        println!("   GET /users/123");

        // 3. Create Resource (POST with JSON body)
        println!("\n3️⃣ Create New User:");
        let mut create_action = HttpAction::new("POST".to_string(), "/users".to_string());
        create_action.request_body = Some(json!({
            "name": "John Doe",
            "email": "john@example.com",
            "role": "developer"
        }));
        println!("   POST /users + JSON body");

        // 4. Update Resource (PUT with JSON body)
        println!("\n4️⃣ Update User:");
        let mut update_action = HttpAction::new("PUT".to_string(), "/users/123".to_string());
        update_action.request_body = Some(json!({
            "name": "John Smith",
            "role": "senior_developer"
        }));
        println!("   PUT /users/123 + JSON body");

        // 5. Delete Resource (DELETE)
        println!("\n5️⃣ Delete User:");
        let _delete_action = HttpAction::new("DELETE".to_string(), "/users/123".to_string());
        println!("   DELETE /users/123");

        // 6. Bulk Operation (POST with array)
        println!("\n6️⃣ Bulk Create Users:");
        let mut bulk_action = HttpAction::new("POST".to_string(), "/users/bulk".to_string());
        bulk_action.request_body = Some(json!([
            {"name": "User 1", "email": "user1@example.com"},
            {"name": "User 2", "email": "user2@example.com"},
            {"name": "User 3", "email": "user3@example.com"}
        ]));
        println!("   POST /users/bulk + JSON array");

        println!("\n✅ All patterns supported by HttpAction!");
    }

    /// Demonstrates action composition and reuse
    #[tokio::test]
    async fn example_action_composition() {
        println!("\n=== Action Composition and Reuse ===\n");

        // Base action template for GitHub API
        fn github_action(method: &str, path: &str) -> HttpAction {
            let mut action = HttpAction::new(method.to_string(), path.to_string());
            
            // Common headers for all GitHub actions
            let mut headers = HashMap::new();
            headers.insert("User-Agent".to_string(), vec!["OpenAct-Bot/1.0".to_string()]);
            headers.insert("Accept".to_string(), vec!["application/vnd.github.v3+json".to_string()]);
            action.headers = Some(headers);
            
            // Common timeout policy
            action.timeout_config = Some(TimeoutConfig {
                connect_ms: 5000,
                read_ms: 30000,
                total_ms: 45000,
            });
            
            action
        }

        // Specific actions using the template
        let list_repos = github_action("GET", "/user/repos");
        let get_repo = github_action("GET", "/repos/owner/repo");
        let create_issue = github_action("POST", "/repos/owner/repo/issues");

        println!("🏗️ Action Template Pattern:");
        println!("  All actions inherit: User-Agent, Accept, Timeout");
        println!("  Specific actions: {} {}", list_repos.method, list_repos.path);
        println!("                   {} {}", get_repo.method, get_repo.path);
        println!("                   {} {}", create_issue.method, create_issue.path);

        // Action with environment-specific overrides
        fn production_action(base_action: HttpAction) -> HttpAction {
            let mut action = base_action;
            
            // Override retry policy for production
            action.retry_policy = Some(RetryPolicy {
                max_retries: 3,
                initial_delay_ms: 1000,
                max_delay_ms: 30000,
                backoff_multiplier: 2.0,
                retry_on_status_codes: vec![429, 500, 502, 503, 504],
            });
            
            action
        }

        let prod_action = production_action(github_action("GET", "/user"));
        println!("\n🏭 Production Action:");
        println!("  Max Retries: {}", prod_action.retry_policy.unwrap().max_retries);
    }
}
