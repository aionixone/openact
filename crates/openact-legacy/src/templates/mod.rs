//! Provider template system for managing Connection and Task configurations
//!
//! This module provides a simple template system that allows loading predefined
//! configurations for various providers (GitHub, Slack, etc.) and instantiating
//! them with user-provided secrets and inputs.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::interface::dto::{ConnectionUpsertRequest, TaskUpsertRequest};

/// Template metadata for documentation and validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_secrets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_connection: Option<String>,
}

/// Connection template structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTemplate {
    pub provider: String,
    pub template_type: String, // "connection"
    pub template_version: String,
    pub metadata: TemplateMetadata,
    pub config: Value, // Raw JSON config that will be merged and converted to ConnectionUpsertRequest
}

/// Task template structure  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub provider: String,
    pub template_type: String, // "task"
    pub action: String,
    pub template_version: String,
    pub metadata: TemplateMetadata,
    pub config: Value, // Raw JSON config that will be merged and converted to TaskUpsertRequest
}

/// Input parameters for template instantiation
#[derive(Debug, Clone, Default)]
pub struct TemplateInputs {
    /// Secrets to inject (e.g., client_id, client_secret, api_key_value)
    pub secrets: HashMap<String, String>,
    /// Non-sensitive runtime inputs (e.g., scope, timeout, user_agent)
    pub inputs: HashMap<String, Value>,
    /// Explicit field overrides (highest priority)
    pub overrides: HashMap<String, Value>,
}

/// Template metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateListItem {
    pub provider: String,
    pub template_type: String,
    pub name: String,
    pub action: Option<String>, // Only for task templates
    pub template_version: String,
    pub metadata: TemplateMetadata,
}

/// Template loader for managing provider templates
#[derive(Clone)]
pub struct TemplateLoader {
    template_root: String,
}

impl TemplateLoader {
    pub fn new(template_root: impl Into<String>) -> Self {
        Self {
            template_root: template_root.into(),
        }
    }

    /// Load a connection template
    pub fn load_connection_template(
        &self,
        provider: &str,
        template_name: &str,
    ) -> Result<ConnectionTemplate> {
        let path = format!(
            "{}/providers/{}/connections/{}.json",
            self.template_root, provider, template_name
        );
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read connection template {}: {}", path, e))?;

        let template: ConnectionTemplate = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse connection template {}: {}", path, e))?;

        Ok(template)
    }

    /// Load a task template
    pub fn load_task_template(&self, provider: &str, action: &str) -> Result<TaskTemplate> {
        let path = format!(
            "{}/providers/{}/tasks/{}.json",
            self.template_root, provider, action
        );
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read task template {}: {}", path, e))?;

        let template: TaskTemplate = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse task template {}: {}", path, e))?;

        Ok(template)
    }

    /// Instantiate a connection template with provided inputs
    pub fn instantiate_connection(
        &self,
        template: &ConnectionTemplate,
        tenant: &str,
        connection_name: &str,
        inputs: &TemplateInputs,
    ) -> Result<ConnectionUpsertRequest> {
        // Validate required secrets are provided
        if let Some(required) = &template.metadata.required_secrets {
            let missing: Vec<_> = required
                .iter()
                .filter(|key| !inputs.secrets.contains_key(*key))
                .collect();

            if !missing.is_empty() {
                return Err(anyhow!(
                    "Missing required secrets for {}/{} template: {}. Required secrets: {}",
                    template.provider,
                    template.template_type,
                    missing
                        .iter()
                        .map(|s| format!("'{}'", s))
                        .collect::<Vec<_>>()
                        .join(", "),
                    required.join(", ")
                ));
            }
        }

        // Start with template config
        let mut config = template.config.clone();

        // Apply inputs (non-sensitive overrides)
        if !inputs.inputs.is_empty() {
            merge_json_objects(&mut config, &serde_json::to_value(&inputs.inputs)?)?;
        }

        // Apply overrides (highest priority)
        if !inputs.overrides.is_empty() {
            merge_json_objects(&mut config, &serde_json::to_value(&inputs.overrides)?)?;
        }

        // Inject secrets based on template provider
        self.inject_connection_secrets(&mut config, &template.provider, &inputs.secrets)?;

        // Generate TRN
        let trn = format!("trn:openact:{}:connection/{}@v1", tenant, connection_name);
        config["trn"] = Value::String(trn);

        // Convert to ConnectionUpsertRequest DTO
        let connection_request: ConnectionUpsertRequest =
            serde_json::from_value(config).map_err(|e| {
                anyhow!(
                    "Failed to convert template to ConnectionUpsertRequest: {}",
                    e
                )
            })?;

        Ok(connection_request)
    }

    /// Instantiate a task template with provided inputs
    pub fn instantiate_task(
        &self,
        template: &TaskTemplate,
        tenant: &str,
        task_name: &str,
        connection_trn: &str,
        inputs: &TemplateInputs,
    ) -> Result<TaskUpsertRequest> {
        // Validate required secrets are provided (though tasks usually don't have secrets directly)
        if let Some(required) = &template.metadata.required_secrets {
            let missing: Vec<_> = required
                .iter()
                .filter(|key| !inputs.secrets.contains_key(*key))
                .collect();

            if !missing.is_empty() {
                return Err(anyhow!(
                    "Missing required secrets for {}/{} template: {}. Required secrets: {}",
                    template.provider,
                    template.action,
                    missing
                        .iter()
                        .map(|s| format!("'{}'", s))
                        .collect::<Vec<_>>()
                        .join(", "),
                    required.join(", ")
                ));
            }
        }

        // Start with template config
        let mut config = template.config.clone();

        // Apply inputs (non-sensitive overrides)
        if !inputs.inputs.is_empty() {
            merge_json_objects(&mut config, &serde_json::to_value(&inputs.inputs)?)?;
        }

        // Apply overrides (highest priority)
        if !inputs.overrides.is_empty() {
            merge_json_objects(&mut config, &serde_json::to_value(&inputs.overrides)?)?;
        }

        // Generate TRN and set connection reference
        let trn = format!("trn:openact:{}:task/{}@v1", tenant, task_name);
        config["trn"] = Value::String(trn);
        config["connection_trn"] = Value::String(connection_trn.to_string());

        // Convert to TaskUpsertRequest DTO
        let task_request: TaskUpsertRequest = serde_json::from_value(config)
            .map_err(|e| anyhow!("Failed to convert template to TaskUpsertRequest: {}", e))?;

        Ok(task_request)
    }

    /// Inject secrets into connection config based on provider and auth type
    fn inject_connection_secrets(
        &self,
        config: &mut Value,
        provider: &str,
        secrets: &HashMap<String, String>,
    ) -> Result<()> {
        // Check authorization type to determine which secrets to inject
        let auth_type = config
            .get("authorization_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match auth_type {
            "oauth2_authorization_code" | "oauth2_client_credentials" => {
                // Inject OAuth2 secrets
                let client_id_key = format!("{}_client_id", provider);
                let client_secret_key = format!("{}_client_secret", provider);

                if let Some(client_id) = secrets.get(&client_id_key) {
                    if let Some(oauth_params) = config
                        .get_mut("auth_parameters")
                        .and_then(|ap| ap.get_mut("oauth_parameters"))
                    {
                        oauth_params["client_id"] = Value::String(client_id.clone());
                    }
                }

                if let Some(client_secret) = secrets.get(&client_secret_key) {
                    if let Some(oauth_params) = config
                        .get_mut("auth_parameters")
                        .and_then(|ap| ap.get_mut("oauth_parameters"))
                    {
                        oauth_params["client_secret"] = Value::String(client_secret.clone());
                    }
                }
            }
            "api_key" => {
                // Inject API key secret
                let api_key_key = format!("{}_api_key", provider);
                if let Some(api_key) = secrets.get(&api_key_key) {
                    if let Some(api_key_params) = config
                        .get_mut("auth_parameters")
                        .and_then(|ap| ap.get_mut("api_key_auth_parameters"))
                    {
                        api_key_params["api_key_value"] = Value::String(api_key.clone());
                    }
                }
            }
            "basic" => {
                // Inject basic auth password
                let password_key = format!("{}_password", provider);
                if let Some(password) = secrets.get(&password_key) {
                    if let Some(basic_params) = config
                        .get_mut("auth_parameters")
                        .and_then(|ap| ap.get_mut("basic_auth_parameters"))
                    {
                        basic_params["password"] = Value::String(password.clone());
                    }
                }
            }
            _ => {
                return Err(anyhow!("Unsupported authorization type: {}", auth_type));
            }
        }

        Ok(())
    }

    /// List all available templates
    pub fn list_templates(
        &self,
        provider_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Result<Vec<TemplateListItem>> {
        let mut templates = Vec::new();
        let providers_dir = std::path::Path::new(&self.template_root).join("providers");

        if !providers_dir.exists() {
            return Ok(templates);
        }

        // Iterate through provider directories
        for provider_entry in std::fs::read_dir(&providers_dir)? {
            let provider_entry = provider_entry?;
            let provider_name = provider_entry.file_name().to_string_lossy().to_string();

            // Apply provider filter
            if let Some(filter) = provider_filter {
                if provider_name != filter {
                    continue;
                }
            }

            let provider_path = provider_entry.path();
            if !provider_path.is_dir() {
                continue;
            }

            // Check connections
            if type_filter.is_none() || type_filter == Some("connection") {
                let connections_dir = provider_path.join("connections");
                if connections_dir.exists() {
                    for conn_entry in std::fs::read_dir(&connections_dir)? {
                        let conn_entry = conn_entry?;
                        let conn_name = conn_entry.file_name().to_string_lossy().to_string();
                        if conn_name.ends_with(".json") {
                            let name = conn_name.strip_suffix(".json").unwrap().to_string();
                            if let Ok(template) =
                                self.load_connection_template(&provider_name, &name)
                            {
                                templates.push(TemplateListItem {
                                    provider: template.provider,
                                    template_type: template.template_type,
                                    name,
                                    action: None,
                                    template_version: template.template_version,
                                    metadata: template.metadata,
                                });
                            }
                        }
                    }
                }
            }

            // Check tasks
            if type_filter.is_none() || type_filter == Some("task") {
                let tasks_dir = provider_path.join("tasks");
                if tasks_dir.exists() {
                    for task_entry in std::fs::read_dir(&tasks_dir)? {
                        let task_entry = task_entry?;
                        let task_name = task_entry.file_name().to_string_lossy().to_string();
                        if task_name.ends_with(".json") {
                            let name = task_name.strip_suffix(".json").unwrap().to_string();
                            if let Ok(template) = self.load_task_template(&provider_name, &name) {
                                templates.push(TemplateListItem {
                                    provider: template.provider,
                                    template_type: template.template_type,
                                    name,
                                    action: Some(template.action),
                                    template_version: template.template_version,
                                    metadata: template.metadata,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sort by provider, then type, then name
        templates.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then(a.template_type.cmp(&b.template_type))
                .then(a.name.cmp(&b.name))
        });

        Ok(templates)
    }

    /// Show detailed template information
    pub fn show_template(
        &self,
        provider: &str,
        template_type: &str,
        name: &str,
    ) -> Result<serde_json::Value> {
        match template_type {
            "connection" => {
                let template = self.load_connection_template(provider, name)?;
                Ok(serde_json::to_value(&template)?)
            }
            "task" => {
                let template = self.load_task_template(provider, name)?;
                Ok(serde_json::to_value(&template)?)
            }
            _ => Err(anyhow!(
                "Invalid template type '{}'. Must be 'connection' or 'task'",
                template_type
            )),
        }
    }
}

/// Deep merge two JSON objects, with the second taking precedence
fn merge_json_objects(target: &mut Value, source: &Value) -> Result<()> {
    match (target.as_object_mut(), source.as_object()) {
        (Some(target_map), Some(source_map)) => {
            for (key, value) in source_map {
                if let Some(existing) = target_map.get_mut(key) {
                    merge_json_objects(existing, value)?;
                } else {
                    target_map.insert(key.clone(), value.clone());
                }
            }
        }
        _ => {
            // For non-objects, replace entirely
            *target = source.clone();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_json_objects() {
        let mut target = json!({
            "a": 1,
            "b": {
                "c": 2,
                "d": 3
            }
        });

        let source = json!({
            "b": {
                "d": 4,
                "e": 5
            },
            "f": 6
        });

        merge_json_objects(&mut target, &source).unwrap();

        assert_eq!(
            target,
            json!({
                "a": 1,
                "b": {
                    "c": 2,
                    "d": 4,
                    "e": 5
                },
                "f": 6
            })
        );
    }

    #[test]
    fn test_template_inputs_creation() {
        let mut inputs = TemplateInputs::default();
        inputs
            .secrets
            .insert("github_client_id".to_string(), "test_id".to_string());
        inputs
            .inputs
            .insert("scope".to_string(), json!("user:email,repo:read"));
        inputs
            .overrides
            .insert("name".to_string(), json!("Custom GitHub Connection"));

        assert_eq!(
            inputs.secrets.get("github_client_id"),
            Some(&"test_id".to_string())
        );
        assert_eq!(
            inputs.inputs.get("scope"),
            Some(&json!("user:email,repo:read"))
        );
        assert_eq!(
            inputs.overrides.get("name"),
            Some(&json!("Custom GitHub Connection"))
        );
    }
}

#[cfg(test)]
mod integration_test;
