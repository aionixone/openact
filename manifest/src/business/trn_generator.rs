use crate::utils::error::{OpenApiToolError, Result};
use crate::spec::OpenApi30Spec;
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use trn_rust::{Trn, TrnBuilder, ExecutionTrnBuilder};

/// Action TRN generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTrnConfig {
    /// Enable strict validation using trn-rust crate
    pub strict_validation: bool,
    /// Handle TRN conflicts automatically
    pub auto_resolve_conflicts: bool,
    /// Maximum resource_id length
    pub max_resource_id_length: usize,
    /// Default tenant if not provided
    pub default_tenant: Option<String>,
}

impl Default for ActionTrnConfig {
    fn default() -> Self {
        Self {
            strict_validation: true,
            auto_resolve_conflicts: true,
            max_resource_id_length: 64, // TRN-Rust RESOURCE_ID_MAX_LENGTH
            default_tenant: None,
        }
    }
}

/// Action TRN generation metadata and statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActionTrnMetadata {
    /// Total actions processed
    pub total_actions: usize,
    /// Successfully generated TRNs
    pub successful_trns: usize,
    /// Failed TRN generations
    pub failed_trns: usize,
    /// Conflicts detected and resolved
    pub conflicts_resolved: usize,
    /// Generated TRNs by provider
    pub trns_by_provider: HashMap<String, usize>,
    /// Generated TRNs by tenant
    pub trns_by_tenant: HashMap<String, usize>,
}

/// Action TRN generation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTrnResult {
    /// Successfully generated TRNs
    pub generated_trns: Vec<ActionTrn>,
    /// Failed generations with errors
    pub failed_generations: Vec<ActionTrnError>,
    /// Generation metadata and statistics
    pub metadata: ActionTrnMetadata,
}

/// Action TRN generation error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTrnError {
    /// The action that failed
    pub action_name: String,
    /// Error message
    pub error: String,
    /// Error type
    pub error_type: String,
}

/// Action TRN information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTrn {
    /// The generated TRN string
    pub trn: String,
    /// Action name
    pub action_name: String,
    /// Provider name
    pub provider: String,
    /// Tenant
    pub tenant: String,
}

/// Action TRN Generator with trn-rust integration
#[derive(Debug)]
pub struct ActionTrnGenerator {
    /// Generation configuration
    config: ActionTrnConfig,
    /// Generated TRNs for conflict detection
    generated_trns: HashSet<String>,
    /// Resource ID usage counter for conflict resolution
    resource_id_counter: HashMap<String, usize>,
    /// Generation metadata
    metadata: ActionTrnMetadata,
}

impl ActionTrnGenerator {
    /// Create a new Action TRN generator with default configuration
    pub fn new() -> Self {
        Self {
            config: ActionTrnConfig::default(),
            generated_trns: HashSet::new(),
            resource_id_counter: HashMap::new(),
            metadata: ActionTrnMetadata::default(),
        }
    }

    /// Create a new Action TRN generator with custom configuration
    pub fn new_with_config(config: ActionTrnConfig) -> Self {
        Self {
            config,
            generated_trns: HashSet::new(),
            resource_id_counter: HashMap::new(),
            metadata: ActionTrnMetadata::default(),
        }
    }

    /// Configure the generator
    pub fn with_config(mut self, config: ActionTrnConfig) -> Self {
        self.config = config;
        self
    }

    /// Enable or disable strict validation
    pub fn with_strict_validation(mut self, enabled: bool) -> Self {
        self.config.strict_validation = enabled;
        self
    }

    /// Set maximum resource ID length
    pub fn with_max_resource_id_length(mut self, length: usize) -> Self {
        self.config.max_resource_id_length = length;
        self
    }

    /// Set default tenant
    pub fn with_default_tenant(mut self, tenant: String) -> Self {
        self.config.default_tenant = Some(tenant);
        self
    }

    /// Generate Action TRN from OpenAPI specification
    pub fn generate_action_trn(
        &mut self,
        spec: &OpenApi30Spec,
        path: &str,
        method: &str,
        tenant: Option<&str>,
    ) -> Result<ActionTrn> {
        let tenant = tenant
            .or_else(|| self.config.default_tenant.as_deref())
            .ok_or_else(|| OpenApiToolError::validation("Tenant is required".to_string()))?;

        // Generate action name from path and method
        let action_name = self.generate_action_name(path, method);
        
        // Extract provider from OpenAPI spec
        let provider = self.extract_provider_name(spec);
        
        // Generate TRN
        let trn_string = self.build_action_trn(&action_name, tenant, &provider)?;
        
        // Track generated TRN
        self.generated_trns.insert(trn_string.clone());
        self.metadata.successful_trns += 1;
        *self.metadata.trns_by_provider.entry(provider.clone()).or_insert(0) += 1;
        *self.metadata.trns_by_tenant.entry(tenant.to_string()).or_insert(0) += 1;

        Ok(ActionTrn {
            trn: trn_string,
            action_name,
            provider,
            tenant: tenant.to_string(),
        })
    }

    /// Generate execution TRN for an action
    pub fn generate_execution_trn(
        &self,
        action_trn: &str,
        execution_id: &str,
    ) -> Result<String> {
        // Parse Action TRN to extract tenant
        let parsed_action = ParsedTrn::parse(action_trn)?;
        
        let exec_trn = ExecutionTrnBuilder::new()
            .tenant(&parsed_action.tenant)
            .workflow_name("action-execution")
            .execution_id(execution_id)
            .build()
            .map_err(|e| OpenApiToolError::validation(format!("Execution TRN validation failed: {}", e)))?;
        
        Ok(exec_trn.to_string())
    }

    /// Generate action name from path and method
    fn generate_action_name(&self, path: &str, method: &str) -> String {
        let mut action_name = path.trim_start_matches('/').to_string();
        
        // Replace path parameters {owner} -> owner
        action_name = action_name.replace("{", "").replace("}", "");
        
        // Replace slashes with dots
        action_name = action_name.replace("/", ".");
        
        // If path doesn't contain dots and method is not GET, add method
        if !action_name.contains('.') && method.to_uppercase() != "GET" {
            action_name = format!("{}.{}", action_name, method.to_lowercase());
        }
        
        action_name
    }

    /// Extract provider name from OpenAPI specification
    /// 完全基于 OpenAPI 文档的元数据，不硬编码任何提供商
    fn extract_provider_name(&self, spec: &OpenApi30Spec) -> String {
        // 1. 优先从 x-provider 扩展字段获取（推荐方式）
        if let Some(provider) = spec.extensions.get("x-provider") {
            if let Some(provider_str) = provider.as_str() {
                return provider_str.to_string();
            }
        }
        
        // 2. 从 x-vendor 扩展字段获取（备选方式）
        if let Some(vendor) = spec.extensions.get("x-vendor") {
            if let Some(vendor_str) = vendor.as_str() {
                return vendor_str.to_string();
            }
        }
        
        // 3. 从 x-service 扩展字段获取（备选方式）
        if let Some(service) = spec.extensions.get("x-service") {
            if let Some(service_str) = service.as_str() {
                return service_str.to_string();
            }
        }
        
        // 4. 从 servers 中提取域名作为提供商标识
        if let Some(domain) = self.extract_primary_domain(&spec.servers) {
            return self.sanitize_domain_to_provider(&domain);
        }
        
        // 5. 从 title 中提取（作为最后手段）
        let title_lower = spec.info.title.to_lowercase();
        return self.sanitize_title_to_provider(&title_lower);
    }
    
    /// 从 servers 列表中提取主要域名
    fn extract_primary_domain(&self, servers: &[crate::spec::Server]) -> Option<String> {
        if servers.is_empty() {
            return None;
        }
        
        // 取第一个 server 的域名
        if let Ok(parsed_url) = url::Url::parse(&servers[0].url) {
            if let Some(host) = parsed_url.host_str() {
                return Some(host.to_string());
            }
        }
        
        None
    }
    
    /// 将域名转换为提供商名称（通用方法，不硬编码特定提供商）
    fn sanitize_domain_to_provider(&self, domain: &str) -> String {
        let domain_lower = domain.to_lowercase();
        
        // 移除常见的 API 前缀和后缀
        let mut provider = domain_lower
            .replace("api.", "")
            .replace("www.", "")
            .replace(".com", "")
            .replace(".org", "")
            .replace(".net", "")
            .replace(".io", "")
            .replace(".so", "")
            .replace("-api", "")
            .replace("_api", "");
        
        // 如果域名包含多个部分，取主要部分
        if let Some(first_part) = provider.split('.').next() {
            provider = first_part.to_string();
        }
        
        // 清理特殊字符
        provider = provider
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        
        if provider.is_empty() {
            "unknown".to_string()
        } else {
            provider
        }
    }
    
    /// 从标题中提取提供商名称（通用方法，不硬编码特定提供商）
    fn sanitize_title_to_provider(&self, title: &str) -> String {
        let title_lower = title.to_lowercase();
        
        // 移除常见的 API 相关词汇
        let mut provider = title_lower
            .replace(" api", "")
            .replace("api ", "")
            .replace(" rest", "")
            .replace("rest ", "")
            .replace(" web", "")
            .replace("web ", "")
            .replace(" service", "")
            .replace("service ", "");
        
        // 取第一个单词作为提供商名称
        if let Some(first_word) = provider.split_whitespace().next() {
            provider = first_word.to_string();
        }
        
        // 清理特殊字符
        provider = provider
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        
        if provider.is_empty() {
            "unknown".to_string()
        } else {
            provider
        }
    }

    /// Build Action TRN string
    fn build_action_trn(&self, action_name: &str, tenant: &str, provider: &str) -> Result<String> {
        let trn = TrnBuilder::new()
            .tool("openact")
            .tenant(tenant)
            .resource_type("action")
            .resource_id(action_name)
            .add_metadata("provider", provider)
            .build()
            .map_err(|e| OpenApiToolError::validation(format!("Action TRN validation failed: {}", e)))?;
        
        Ok(trn.to_string())
    }

    /// Reset generator state for new generation batch
    #[allow(dead_code)]
    fn reset_state(&mut self) {
        self.generated_trns.clear();
        self.resource_id_counter.clear();
        self.metadata = ActionTrnMetadata::default();
    }

    /// Get current generation statistics
    pub fn get_metadata(&self) -> &ActionTrnMetadata {
        &self.metadata
    }

    /// Print generation statistics
    pub fn print_generation_stats(&self) {
        println!("\n🏷️ Action TRN Generation Statistics:");
        println!("  Total actions processed: {}", self.metadata.total_actions);
        println!("  Successfully generated TRNs: {}", self.metadata.successful_trns);
        println!("  Failed generations: {}", self.metadata.failed_trns);
        println!("  Conflicts resolved: {}", self.metadata.conflicts_resolved);

        if !self.metadata.trns_by_provider.is_empty() {
            println!("  TRNs by provider:");
            for (provider, count) in &self.metadata.trns_by_provider {
                println!("    {}: {}", provider, count);
            }
        }

        if !self.metadata.trns_by_tenant.is_empty() {
            println!("  TRNs by tenant:");
            for (tenant, count) in &self.metadata.trns_by_tenant {
                println!("    {}: {}", tenant, count);
            }
        }
    }
}

impl Default for ActionTrnGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a TRN string using trn-rust crate
pub fn validate_action_trn(trn_string: &str) -> Result<bool> {
    match Trn::parse(trn_string) {
        Ok(_) => Ok(true),
        Err(e) => Err(OpenApiToolError::validation(format!("Action TRN validation failed: {}", e))),
    }
}

/// Generate a complete Action TRN using default configuration
pub fn generate_action_trn(
    spec: &OpenApi30Spec,
    path: &str,
    method: &str,
    tenant: &str,
) -> Result<ActionTrn> {
    let mut generator = ActionTrnGenerator::new();
    generator.generate_action_trn(spec, path, method, Some(tenant))
}

/// Parse TRN string and extract components
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTrn {
    pub tool: String,
    pub tenant: String,
    pub resource_type: String,
    pub resource_id: String,
    pub metadata: HashMap<String, String>,
}

impl ParsedTrn {
    /// Parse a TRN string into components
    pub fn parse(trn_string: &str) -> Result<Self> {
        // Use trn-rust to parse the TRN
        let trn = Trn::parse(trn_string)
            .map_err(|e| OpenApiToolError::validation(format!("Failed to parse TRN: {}", e)))?;
        
        Ok(Self {
            tool: trn.tool,
            tenant: trn.tenant,
            resource_type: trn.resource_type,
            resource_id: trn.resource_id,
            metadata: trn.metadata,
        })
    }
    
    /// Get a specific metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

