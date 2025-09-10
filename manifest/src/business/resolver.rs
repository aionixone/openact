use crate::utils::error::{OpenApiToolError, Result};
use crate::spec::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Structure of the resolved document
#[derive(Debug, Clone)]
pub struct ResolvedDocument {
    pub spec: OpenApi30Spec,
    pub resolved_refs: HashMap<String, Value>,
    pub circular_refs: Vec<String>,
}

/// A new JSON reference resolver - simple but 100% effective
///
/// Core idea:
/// 1. Directly operate on the JSON level, without relying on the type system
/// 2. Recursively traverse and replace when encountering $ref
/// 3. Finally convert to a struct
#[derive(Debug)]
pub struct ReferenceResolver {
    /// Original document JSON
    source_doc: Value,
    /// Resolution statistics
    resolved_count: usize,
    /// Circular reference detection stack
    resolution_stack: HashSet<String>,
    /// List of circular references
    circular_refs: Vec<String>,
}

impl ReferenceResolver {
    /// Create a new resolver
    pub fn new() -> Self {
        Self {
            source_doc: Value::Null,
            resolved_count: 0,
            resolution_stack: HashSet::new(),
            circular_refs: Vec::new(),
        }
    }

    /// Resolve all references in the OpenAPI document
    pub fn resolve(&mut self, spec: OpenApi30Spec, spec_json: Value) -> Result<ResolvedDocument> {
        // Store the original document for reference target lookup
        self.source_doc = spec_json.clone();
        
        // Convert spec to JSON for dereferencing
        let mut working_json = serde_json::to_value(&spec)
            .map_err(|e| OpenApiToolError::ParseError(format!("Failed to serialize spec: {}", e)))?;
        
        // Recursively resolve all references
        self.resolve_references(&mut working_json)?;
        
        // Convert back to struct
        let resolved_spec: OpenApi30Spec = serde_json::from_value(working_json)
            .map_err(|e| OpenApiToolError::ParseError(format!("Failed to deserialize resolved spec: {}", e)))?;
        
        Ok(ResolvedDocument {
            spec: resolved_spec,
            resolved_refs: HashMap::new(), // Simplified version, no caching needed
            circular_refs: self.circular_refs.clone(),
        })
    }

    /// Recursively resolve all references in JSON
    fn resolve_references(&mut self, json: &mut Value) -> Result<()> {
        match json {
            Value::Object(obj) => {
                // Check if it's a reference object
                if let Some(Value::String(ref_path)) = obj.get("$ref").cloned() {
                    // This is a reference, needs to be resolved
                    println!("🔗 Found reference: {}", ref_path);
                    
                    // Detect circular reference
                    if self.resolution_stack.contains(&ref_path) {
                        println!("⚠️  Circular reference detected: {}", ref_path);
                        self.circular_refs.push(ref_path.clone());
                        return Ok(()); // Keep the original reference, do not continue resolving
                    }
                    
                    // Add to resolution stack
                    self.resolution_stack.insert(ref_path.clone());
                    
                    // Resolve reference target
                    match self.resolve_json_pointer(&ref_path) {
                        Ok(mut target_value) => {
                            // Recursively resolve references in the target value
                            self.resolve_references(&mut target_value)?;
                            
                            // Replace the current object
                            *json = target_value;
                            self.resolved_count += 1;
                            println!("✅ Resolved reference: {}", ref_path);
                        }
                        Err(e) => {
                            println!("❌ Failed to resolve reference {}: {}", ref_path, e);
                            // Keep the original reference
                        }
                    }
                    
                    // Remove from resolution stack
                    self.resolution_stack.remove(&ref_path);
                } else {
                    // Regular object, recursively process all values
                    for value in obj.values_mut() {
                        self.resolve_references(value)?;
                    }
                }
            }
            Value::Array(arr) => {
                // Array, recursively process all elements
                for item in arr.iter_mut() {
                    self.resolve_references(item)?;
                }
            }
            _ => {
                // Primitive type, no processing needed
            }
        }
        
        Ok(())
    }

    /// Resolve JSON pointer path
    fn resolve_json_pointer(&self, ref_path: &str) -> Result<Value> {
        if !ref_path.starts_with("#/") {
            return Err(OpenApiToolError::ValidationError(
                format!("Only internal references starting with '#/' are supported, got: {}", ref_path)
            ));
        }

        let path = &ref_path[2..]; // Remove "#/"
        let segments: Vec<&str> = path.split('/').collect();

        let mut current = &self.source_doc;
        for segment in segments {
            match current {
                Value::Object(obj) => {
                    // Object, look for key
                    current = obj.get(segment).ok_or_else(|| {
                        OpenApiToolError::ValidationError(
                            format!("Reference path not found: {} at segment '{}'", ref_path, segment)
                        )
                    })?;
                }
                Value::Array(arr) => {
                    // Array, parse index
                    let index: usize = segment.parse().map_err(|_| {
                        OpenApiToolError::ValidationError(
                            format!("Invalid array index in path: {} at '{}'", ref_path, segment)
                        )
                    })?;
                    current = arr.get(index).ok_or_else(|| {
                        OpenApiToolError::ValidationError(
                            format!("Array index out of bounds: {} at index {}", ref_path, index)
                        )
                    })?;
                }
                _ => {
                    return Err(OpenApiToolError::ValidationError(
                        format!("Cannot traverse into non-container at path: {} segment '{}'", ref_path, segment)
                    ));
                }
            }
        }

        Ok(current.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_reference_resolution() {
        let mut resolver = ReferenceResolver::new();
        
        // Create test document
        let source = serde_json::json!({
            "components": {
                "schemas": {
                    "Pet": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"}
                        }
                    }
                }
            },
            "paths": {
                "/pets": {
                    "get": {
                        "responses": {
                            "200": {
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$ref": "#/components/schemas/Pet"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        
        resolver.source_doc = source.clone();
        let mut test_json = source;
        
        // Perform resolution
        resolver.resolve_references(&mut test_json).unwrap();
        
        // Verify that the reference has been resolved
        let response_schema = &test_json["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(response_schema["type"], "object");
        assert_eq!(response_schema["properties"]["name"]["type"], "string");
        assert!(response_schema.get("$ref").is_none()); // Confirm that $ref has been removed
    }
}
