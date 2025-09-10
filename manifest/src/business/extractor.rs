use crate::utils::error::{Result, OpenApiToolError};
use crate::spec::*;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// OpenAPI endpoint information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiEndpoint {
    /// HTTP method (GET, POST, PUT, DELETE, etc.)
    pub method: String,
    /// API path (e.g., /pet/{petId})
    pub path: String,
    /// Operation ID (if any)
    pub operation_id: Option<String>,
    /// Operation summary
    pub summary: Option<String>,
    /// Operation description
    pub description: Option<String>,
    /// List of tags
    pub tags: Vec<String>,
    /// Whether the endpoint is deprecated
    pub deprecated: bool,
    /// Parameter information
    pub parameters: Vec<EndpointParameter>,
    /// Request body information
    pub request_body: Option<RequestBodyInfo>,
    /// Response information
    pub responses: HashMap<String, ResponseInfo>,
}

/// Endpoint parameter information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointParameter {
    /// Parameter name
    pub name: String,
    /// Parameter location (path, query, header, cookie)
    pub location: String,
    /// Whether the parameter is required
    pub required: bool,
    /// Parameter type
    pub param_type: Option<String>,
    /// Parameter description
    pub description: Option<String>,
}

/// Request body information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestBodyInfo {
    /// Whether the request body is required
    pub required: bool,
    /// Content types and their schema information
    pub content_types: HashMap<String, SchemaInfo>,
    /// Description
    pub description: Option<String>,
}

/// Response information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInfo {
    /// Response description
    pub description: String,
    /// Content types and their schema information
    pub content_types: HashMap<String, SchemaInfo>,
}

/// Schema information summary
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaInfo {
    /// Data type
    pub data_type: Option<String>,
    /// Whether it is an array
    pub is_array: bool,
    /// Required fields
    pub required_fields: Vec<String>,
    /// Number of properties
    pub properties_count: usize,
}

/// TRN generation configuration
#[derive(Debug, Clone)]
pub struct TrnConfig {
    /// Platform identifier
    pub platform: String,
    /// Scope identifier
    pub scope: String,
    /// Tag
    pub tag: String,
    /// API name (used as a prefix for resource_id)
    pub api_name: String,
    /// API version
    pub api_version: String,
}

/// Generated TRN information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedTrn {
    /// TRN string
    pub trn: String,
    /// Corresponding endpoint
    pub endpoint: ApiEndpoint,
}

/// OpenAPI endpoint extractor
#[derive(Debug)]
pub struct EndpointExtractor {
    /// List of extracted endpoints
    endpoints: Vec<ApiEndpoint>,
    /// List of generated TRNs
    trns: Vec<GeneratedTrn>,
}

impl EndpointExtractor {
    /// Create a new extractor
    pub fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            trns: Vec::new(),
        }
    }

    /// Extract all endpoints from the OpenAPI specification
    pub fn extract_endpoints(&mut self, spec: &OpenApi30Spec) -> Result<Vec<ApiEndpoint>> {
        self.endpoints.clear();

        for (path, path_item) in &spec.paths.paths {
            // Extract path-level parameters
            let path_parameters = self.extract_parameters(&path_item.parameters);

            // Handle various HTTP methods
            if let Some(operation) = &path_item.get {
                self.process_operation("GET", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.post {
                self.process_operation("POST", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.put {
                self.process_operation("PUT", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.delete {
                self.process_operation("DELETE", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.patch {
                self.process_operation("PATCH", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.head {
                self.process_operation("HEAD", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.options {
                self.process_operation("OPTIONS", path, operation, &path_parameters)?;
            }
            if let Some(operation) = &path_item.trace {
                self.process_operation("TRACE", path, operation, &path_parameters)?;
            }
        }

        println!("📊 Extraction complete! Found {} API endpoints", self.endpoints.len());
        Ok(self.endpoints.clone())
    }

    /// Generate TRNs for all endpoints (deprecated - use ActionTrnGenerator instead)
    pub fn generate_trns(&mut self, _config: &TrnConfig) -> Result<Vec<GeneratedTrn>> {
        // This method is deprecated and should not be used
        // Use ActionTrnGenerator::generate_action_trn instead
        Err(OpenApiToolError::validation("This method is deprecated. Use ActionTrnGenerator instead.".to_string()))
    }

    /// Get extracted endpoints
    pub fn endpoints(&self) -> &Vec<ApiEndpoint> {
        &self.endpoints
    }

    /// Get generated TRNs
    pub fn trns(&self) -> &Vec<GeneratedTrn> {
        &self.trns
    }

    /// Process a single operation
    fn process_operation(
        &mut self, 
        method: &str, 
        path: &str, 
        operation: &Operation,
        path_parameters: &[EndpointParameter]
    ) -> Result<()> {
        // Merge path-level and operation-level parameters
        let mut all_parameters = path_parameters.to_vec();
        let operation_parameters = self.extract_parameters(&operation.parameters);
        all_parameters.extend(operation_parameters);

        // Extract request body information
        let request_body = if let Some(req_body_ref) = &operation.request_body {
            match req_body_ref {
                OrReference::Item(req_body) => {
                    Some(self.extract_request_body_info(req_body)?)
                }
                OrReference::Reference(_) => {
                    // There should be no references after dereferencing
                    None
                }
            }
        } else {
            None
        };

        // Extract response information
        let mut responses = HashMap::new();
        
        // Handle default response
        if let Some(default_response_ref) = &operation.responses.default {
            if let OrReference::Item(response) = default_response_ref {
                let response_info = self.extract_response_info(response)?;
                responses.insert("default".to_string(), response_info);
            }
        }

        // Handle status code responses
        for (status, response_ref) in &operation.responses.responses {
            if let OrReference::Item(response) = response_ref {
                let response_info = self.extract_response_info(response)?;
                responses.insert(status.clone(), response_info);
            }
        }

        let endpoint = ApiEndpoint {
            method: method.to_uppercase(),
            path: path.to_string(),
            operation_id: operation.operation_id.clone(),
            summary: operation.summary.clone(),
            description: operation.description.clone(),
            tags: operation.tags.clone(),
            deprecated: operation.deprecated,
            parameters: all_parameters,
            request_body,
            responses,
        };

        self.endpoints.push(endpoint);
        Ok(())
    }

    /// Extract parameter information
    fn extract_parameters(&self, params: &[ParameterOrReference]) -> Vec<EndpointParameter> {
        let mut result = Vec::new();
        
        for param_ref in params {
            if let OrReference::Item(param) = param_ref {
                let param_type = if let Some(OrReference::Item(schema)) = &param.schema {
                    schema.r#type.clone()
                } else {
                    None
                };

                result.push(EndpointParameter {
                    name: param.name.clone(),
                    location: param.location.clone(),
                    required: param.required,
                    param_type,
                    description: param.description.clone(),
                });
            }
        }

        result
    }

    /// Extract request body information
    fn extract_request_body_info(&self, req_body: &RequestBody) -> Result<RequestBodyInfo> {
        let mut content_types = HashMap::new();

        for (content_type, media_type) in &req_body.content {
            if let Some(OrReference::Item(schema)) = &media_type.schema {
                let schema_info = self.extract_schema_info(schema);
                content_types.insert(content_type.clone(), schema_info);
            }
        }

        Ok(RequestBodyInfo {
            required: req_body.required,
            content_types,
            description: req_body.description.clone(),
        })
    }

    /// Extract response information
    fn extract_response_info(&self, response: &Response) -> Result<ResponseInfo> {
        let mut content_types = HashMap::new();

        for (content_type, media_type) in &response.content {
            if let Some(OrReference::Item(schema)) = &media_type.schema {
                let schema_info = self.extract_schema_info(schema);
                content_types.insert(content_type.clone(), schema_info);
            }
        }

        Ok(ResponseInfo {
            description: response.description.clone(),
            content_types,
        })
    }

    /// Extract schema information summary
    fn extract_schema_info(&self, schema: &Schema) -> SchemaInfo {
        let is_array = schema.r#type.as_deref() == Some("array");
        let properties_count = schema.properties.len();

        SchemaInfo {
            data_type: schema.r#type.clone(),
            is_array,
            required_fields: schema.required.clone(),
            properties_count,
        }
    }

    /// Print extraction statistics
    pub fn print_extraction_stats(&self) {
        println!("\n📊 Endpoint extraction statistics:");
        println!("  Total number of endpoints: {}", self.endpoints.len());
        
        // Statistics by HTTP method
        let mut method_counts = HashMap::new();
        for endpoint in &self.endpoints {
            *method_counts.entry(&endpoint.method).or_insert(0) += 1;
        }
        
        println!("  Distribution by method:");
        for (method, count) in method_counts {
            println!("    {}: {} endpoints", method, count);
        }

        // Statistics by tag
        let mut tag_counts = HashMap::new();
        for endpoint in &self.endpoints {
            for tag in &endpoint.tags {
                *tag_counts.entry(tag).or_insert(0) += 1;
            }
        }

        if !tag_counts.is_empty() {
            println!("  Distribution by tag:");
            for (tag, count) in tag_counts {
                println!("    {}: {} endpoints", tag, count);
            }
        }
    }

    /// Print TRN generation statistics
    pub fn print_trn_stats(&self) {
        println!("\n🏷️  TRN generation statistics:");
        println!("  Total number of TRNs: {}", self.trns.len());
        
        if let Some(sample_trn) = self.trns.get(0) {
            println!("  Sample TRN: {}", sample_trn.trn);
        }
    }
}

impl Default for EndpointExtractor {
    fn default() -> Self {
        Self::new()
    }
}
