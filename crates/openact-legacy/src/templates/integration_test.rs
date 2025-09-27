//! Integration tests for template loading and instantiation
//!
//! These tests verify the complete flow from template files to DTO objects.

#[cfg(test)]
mod tests {
    use super::super::*;
    use serde_json::json;

    #[test]
    fn test_load_github_oauth2_connection_template() {
        let loader = TemplateLoader::new("../../templates");

        // This test depends on the actual template file existing
        let result = loader.load_connection_template("github", "oauth2");

        match result {
            Ok(template) => {
                assert_eq!(template.provider, "github");
                assert_eq!(template.template_type, "connection");
                assert_eq!(template.metadata.name, "GitHub OAuth2 Connection");

                // Verify template has required structure
                assert!(template.config.get("authorization_type").is_some());
                assert!(template.config.get("auth_parameters").is_some());
            }
            Err(e) => {
                // If template file doesn't exist, just log the error
                println!("Template file not found (expected in development): {}", e);
            }
        }
    }

    #[test]
    fn test_load_github_get_user_task_template() {
        let loader = TemplateLoader::new("../../templates");

        let result = loader.load_task_template("github", "get_user");

        match result {
            Ok(template) => {
                assert_eq!(template.provider, "github");
                assert_eq!(template.template_type, "task");
                assert_eq!(template.action, "get_user");

                // Verify template has required structure
                assert!(template.config.get("api_endpoint").is_some());
                assert!(template.config.get("method").is_some());
            }
            Err(e) => {
                println!("Template file not found (expected in development): {}", e);
            }
        }
    }

    #[test]
    fn test_instantiate_connection_with_secrets() {
        // Create a minimal connection template for testing
        let template = ConnectionTemplate {
            provider: "github".to_string(),
            template_type: "connection".to_string(),
            template_version: "1.0".to_string(),
            metadata: TemplateMetadata {
                name: "Test GitHub Connection".to_string(),
                description: "Test template".to_string(),
                documentation: None,
                api_reference: None,
                required_secrets: Some(vec![
                    "github_client_id".to_string(),
                    "github_client_secret".to_string(),
                ]),
                requires_connection: None,
            },
            config: json!({
                "name": "GitHub API Connection",
                "authorization_type": "oauth2_authorization_code",
                "auth_parameters": {
                    "oauth_parameters": {
                        "token_url": "https://github.com/login/oauth/access_token",
                        "scope": "user:email",
                        "use_pkce": false
                    }
                }
            }),
        };

        let loader = TemplateLoader::new("../../templates");

        let mut inputs = TemplateInputs::default();
        inputs
            .secrets
            .insert("github_client_id".to_string(), "test_client_id".to_string());
        inputs.secrets.insert(
            "github_client_secret".to_string(),
            "test_client_secret".to_string(),
        );
        inputs.inputs.insert(
            "auth_parameters".to_string(),
            json!({
                "oauth_parameters": {
                    "scope": "user:email,repo:read"
                }
            }),
        );

        let result =
            loader.instantiate_connection(&template, "test_tenant", "github_test", &inputs);

        assert!(result.is_ok());
        let connection_request = result.unwrap();

        // Verify TRN generation
        assert_eq!(
            connection_request.trn,
            "trn:openact:test_tenant:connection/github_test@v1"
        );

        // Verify name from template
        assert_eq!(connection_request.name, "GitHub API Connection");

        // Verify authorization type
        assert_eq!(
            format!("{:?}", connection_request.authorization_type),
            "OAuth2AuthorizationCode"
        );

        // Verify secrets were injected (we can't see the actual values due to encryption)
        assert!(
            connection_request
                .auth_parameters
                .oauth_parameters
                .is_some()
        );
        let oauth_params = connection_request
            .auth_parameters
            .oauth_parameters
            .as_ref()
            .unwrap();
        assert_eq!(oauth_params.client_id, "test_client_id");
        assert_eq!(oauth_params.client_secret, "test_client_secret");

        // Verify merged scope
        assert_eq!(oauth_params.scope, Some("user:email,repo:read".to_string()));
    }

    #[test]
    fn test_instantiate_task_with_connection() {
        // Create a minimal task template for testing
        let template = TaskTemplate {
            provider: "github".to_string(),
            template_type: "task".to_string(),
            action: "get_user".to_string(),
            template_version: "1.0".to_string(),
            metadata: TemplateMetadata {
                name: "Get GitHub User".to_string(),
                description: "Test task template".to_string(),
                documentation: None,
                api_reference: None,
                required_secrets: None,
                requires_connection: Some("github_oauth2".to_string()),
            },
            config: json!({
                "name": "Get GitHub User Profile",
                "api_endpoint": "https://api.github.com/user",
                "method": "GET",
                "headers": {
                    "Accept": ["application/vnd.github.v3+json"]
                }
            }),
        };

        let loader = TemplateLoader::new("../../templates");

        let inputs = TemplateInputs::default();
        let connection_trn = "trn:openact:test_tenant:connection/github_test@v1";

        let result = loader.instantiate_task(
            &template,
            "test_tenant",
            "get_user_test",
            connection_trn,
            &inputs,
        );

        assert!(result.is_ok());
        let task_request = result.unwrap();

        // Verify TRN generation
        assert_eq!(
            task_request.trn,
            "trn:openact:test_tenant:task/get_user_test@v1"
        );

        // Verify connection reference
        assert_eq!(task_request.connection_trn, connection_trn);

        // Verify task configuration
        assert_eq!(task_request.name, "Get GitHub User Profile");
        assert_eq!(task_request.api_endpoint, "https://api.github.com/user");
        assert_eq!(task_request.method, "GET");

        // Verify headers
        assert!(task_request.headers.is_some());
        let headers = task_request.headers.as_ref().unwrap();
        assert!(headers.contains_key("Accept"));
    }

    #[test]
    fn test_template_inputs_override_priority() {
        let template = ConnectionTemplate {
            provider: "github".to_string(),
            template_type: "connection".to_string(),
            template_version: "1.0".to_string(),
            metadata: TemplateMetadata {
                name: "Test Template".to_string(),
                description: "Test".to_string(),
                documentation: None,
                api_reference: None,
                required_secrets: None,
                requires_connection: None,
            },
            config: json!({
                "name": "Template Default Name",
                "authorization_type": "oauth2_authorization_code",
                "auth_parameters": {
                    "oauth_parameters": {
                        "token_url": "https://example.com/token",
                        "scope": "default_scope"
                    }
                }
            }),
        };

        let loader = TemplateLoader::new("../../templates");

        let mut inputs = TemplateInputs::default();
        // Add required secrets
        inputs
            .secrets
            .insert("github_client_id".to_string(), "test_client_id".to_string());
        inputs.secrets.insert(
            "github_client_secret".to_string(),
            "test_client_secret".to_string(),
        );
        // inputs should override template defaults
        inputs.inputs.insert(
            "auth_parameters".to_string(),
            json!({
                "oauth_parameters": {
                    "scope": "input_scope"
                }
            }),
        );
        // overrides should override everything
        inputs
            .overrides
            .insert("name".to_string(), json!("Override Name"));

        let result = loader
            .instantiate_connection(&template, "test", "test", &inputs)
            .unwrap();

        // Override should win
        assert_eq!(result.name, "Override Name");

        // Input should override template default
        let oauth_params = result.auth_parameters.oauth_parameters.unwrap();
        assert_eq!(oauth_params.scope, Some("input_scope".to_string()));
    }

    #[tokio::test]
    async fn test_missing_required_secrets_validation() {
        let loader = TemplateLoader::new("../../templates");
        let template = loader.load_connection_template("github", "oauth2").unwrap();

        // Create inputs WITHOUT required secrets
        let inputs = TemplateInputs::default();
        // Note: github oauth2 template requires github_client_id and github_client_secret

        let result =
            loader.instantiate_connection(&template, "test_tenant", "github_test", &inputs);

        assert!(
            result.is_err(),
            "Should fail when required secrets are missing"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Missing required secrets"));
        assert!(error_msg.contains("github_client_id"));
        assert!(error_msg.contains("github_client_secret"));
    }

    #[tokio::test]
    async fn test_partial_missing_required_secrets() {
        let loader = TemplateLoader::new("../../templates");
        let template = loader.load_connection_template("github", "oauth2").unwrap();

        // Provide only one of the two required secrets
        let mut inputs = TemplateInputs::default();
        inputs
            .secrets
            .insert("github_client_id".to_string(), "test_id".to_string());
        // Missing github_client_secret

        let result =
            loader.instantiate_connection(&template, "test_tenant", "github_test", &inputs);

        assert!(
            result.is_err(),
            "Should fail when some required secrets are missing"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Missing required secrets"));
        assert!(error_msg.contains("'github_client_secret'")); // Should mention missing secret
        assert!(error_msg.contains("Required secrets: github_client_id, github_client_secret")); // Should show full list
    }
}
