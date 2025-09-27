//! HTTP request body builder with support for different content types

use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use base64::{Engine as _, engine::general_purpose};

/// Supported content types for HTTP request bodies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBodyType {
    /// JSON content with automatic serialization
    Json {
        /// JSON data to serialize
        data: JsonValue,
    },
    /// URL-encoded form data
    Form {
        /// Form fields as key-value pairs
        fields: HashMap<String, String>,
    },
    /// Multipart form data (supports files and fields)
    Multipart {
        /// Text fields
        #[serde(default)]
        fields: HashMap<String, String>,
        /// File fields with metadata
        #[serde(default)]
        files: HashMap<String, FileField>,
    },
    /// Raw bytes with custom content type
    Raw {
        /// Raw data as base64 encoded string
        data: String,
        /// Content type header value
        content_type: String,
    },
    /// Plain text content
    Text {
        /// Text content
        content: String,
        /// Optional charset (defaults to utf-8)
        #[serde(default)]
        charset: Option<String>,
    },
}

/// File field for multipart uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileField {
    /// File name
    pub filename: String,
    /// MIME type
    pub mime_type: String,
    /// File content as base64 encoded string
    pub content: String,
}

/// Built HTTP request body ready for reqwest
#[derive(Debug)]
pub enum HttpRequestBody {
    /// Regular body with bytes
    Body {
        body: reqwest::Body,
        content_type: String,
        content_length: Option<u64>,
    },
    /// Multipart form (handled differently by reqwest)
    Multipart {
        form: Form,
    },
}

/// Body builder for creating HTTP request bodies
pub struct BodyBuilder;

impl BodyBuilder {
    /// Build request body from RequestBodyType
    pub async fn build(body_type: &RequestBodyType) -> Result<HttpRequestBody, BodyBuilderError> {
        match body_type {
            RequestBodyType::Json { data } => Self::build_json(data).await,
            RequestBodyType::Form { fields } => Self::build_form(fields).await,
            RequestBodyType::Multipart { fields, files } => Self::build_multipart(fields, files).await,
            RequestBodyType::Raw { data, content_type } => Self::build_raw(data, content_type).await,
            RequestBodyType::Text { content, charset } => Self::build_text(content, charset.as_deref()).await,
        }
    }

    /// Build JSON request body
    async fn build_json(data: &JsonValue) -> Result<HttpRequestBody, BodyBuilderError> {
        let json_bytes = serde_json::to_vec(data)
            .map_err(|e| BodyBuilderError::Serialization(format!("JSON serialization failed: {}", e)))?;
        
        let content_length = json_bytes.len() as u64;
        
        Ok(HttpRequestBody::Body {
            body: reqwest::Body::from(json_bytes),
            content_type: "application/json".to_string(),
            content_length: Some(content_length),
        })
    }

    /// Build form-encoded request body
    async fn build_form(fields: &HashMap<String, String>) -> Result<HttpRequestBody, BodyBuilderError> {
        let mut form_data = Vec::new();
        let mut first = true;
        
        for (key, value) in fields {
            if !first {
                form_data.push(b'&');
            }
            first = false;
            
            // URL encode key and value
            let encoded_key = urlencoding::encode(key);
            let encoded_value = urlencoding::encode(value);
            form_data.extend_from_slice(encoded_key.as_bytes());
            form_data.push(b'=');
            form_data.extend_from_slice(encoded_value.as_bytes());
        }
        
        let content_length = form_data.len() as u64;
        
        Ok(HttpRequestBody::Body {
            body: reqwest::Body::from(form_data),
            content_type: "application/x-www-form-urlencoded".to_string(),
            content_length: Some(content_length),
        })
    }

    /// Build multipart request body
    async fn build_multipart(
        fields: &HashMap<String, String>,
        files: &HashMap<String, FileField>,
    ) -> Result<HttpRequestBody, BodyBuilderError> {
        let mut form = Form::new();
        
        // Add text fields
        for (key, value) in fields {
            form = form.text(key.clone(), value.clone());
        }
        
        // Add file fields
        for (field_name, file_field) in files {
            // Decode base64 content
            let file_bytes = general_purpose::STANDARD.decode(&file_field.content)
                .map_err(|e| BodyBuilderError::Encoding(format!("Base64 decode failed: {}", e)))?;
            
            let part = Part::bytes(file_bytes)
                .file_name(file_field.filename.clone())
                .mime_str(&file_field.mime_type)
                .map_err(|e| BodyBuilderError::InvalidMimeType(format!("Invalid MIME type '{}': {}", file_field.mime_type, e)))?;
            
            form = form.part(field_name.clone(), part);
        }
        
        // Return the multipart form directly
        Ok(HttpRequestBody::Multipart { form })
    }

    /// Build raw request body
    async fn build_raw(data: &str, content_type: &str) -> Result<HttpRequestBody, BodyBuilderError> {
        // Decode base64 data
        let raw_bytes = general_purpose::STANDARD.decode(data)
            .map_err(|e| BodyBuilderError::Encoding(format!("Base64 decode failed: {}", e)))?;
        
        let content_length = raw_bytes.len() as u64;
        
        Ok(HttpRequestBody::Body {
            body: reqwest::Body::from(raw_bytes),
            content_type: content_type.to_string(),
            content_length: Some(content_length),
        })
    }

    /// Build text request body
    async fn build_text(content: &str, charset: Option<&str>) -> Result<HttpRequestBody, BodyBuilderError> {
        let text_bytes = content.as_bytes().to_vec();
        let content_length = text_bytes.len() as u64;
        
        let content_type = match charset {
            Some(charset) => format!("text/plain; charset={}", charset),
            None => "text/plain; charset=utf-8".to_string(),
        };
        
        Ok(HttpRequestBody::Body {
            body: reqwest::Body::from(text_bytes),
            content_type,
            content_length: Some(content_length),
        })
    }

    /// Detect content type from JsonValue (legacy support)
    pub fn detect_content_type_from_json(value: &JsonValue) -> RequestBodyType {
        // Try to detect if this looks like form data
        if let Some(obj) = value.as_object() {
            // If all values are strings, treat as form data
            let all_strings = obj.values().all(|v| v.is_string());
            if all_strings {
                let fields: HashMap<String, String> = obj
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
                
                return RequestBodyType::Form { fields };
            }
        }
        
        // Default to JSON
        RequestBodyType::Json { data: value.clone() }
    }

    /// Create a simple JSON body
    pub fn json(data: JsonValue) -> RequestBodyType {
        RequestBodyType::Json { data }
    }

    /// Create a form body from key-value pairs
    pub fn form(fields: HashMap<String, String>) -> RequestBodyType {
        RequestBodyType::Form { fields }
    }

    /// Create a multipart body
    pub fn multipart(fields: HashMap<String, String>, files: HashMap<String, FileField>) -> RequestBodyType {
        RequestBodyType::Multipart { fields, files }
    }

    /// Create a raw body
    pub fn raw(data: String, content_type: String) -> RequestBodyType {
        RequestBodyType::Raw { data, content_type }
    }

    /// Create a text body
    pub fn text(content: String) -> RequestBodyType {
        RequestBodyType::Text { content, charset: None }
    }
}

/// Errors that can occur during body building
#[derive(Debug, thiserror::Error)]
pub enum BodyBuilderError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Encoding error: {0}")]
    Encoding(String),
    
    #[error("Invalid MIME type: {0}")]
    InvalidMimeType(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_json_body() {
        let data = json!({"name": "test", "value": 42});
        let body_type = RequestBodyType::Json { data };
        
        let result = BodyBuilder::build(&body_type).await.unwrap();
        match result {
            HttpRequestBody::Body { content_type, content_length, .. } => {
                assert_eq!(content_type, "application/json");
                assert!(content_length.is_some());
            }
            _ => panic!("Expected Body variant"),
        }
    }

    #[tokio::test]
    async fn test_form_body() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), "test value".to_string());
        fields.insert("email".to_string(), "test@example.com".to_string());
        
        let body_type = RequestBodyType::Form { fields };
        let result = BodyBuilder::build(&body_type).await.unwrap();
        
        match result {
            HttpRequestBody::Body { content_type, content_length, .. } => {
                assert_eq!(content_type, "application/x-www-form-urlencoded");
                assert!(content_length.is_some());
            }
            _ => panic!("Expected Body variant"),
        }
    }

    #[tokio::test]
    async fn test_text_body() {
        let content = "Hello, World!".to_string();
        let body_type = RequestBodyType::Text { content, charset: None };
        
        let result = BodyBuilder::build(&body_type).await.unwrap();
        match result {
            HttpRequestBody::Body { content_type, content_length, .. } => {
                assert_eq!(content_type, "text/plain; charset=utf-8");
                assert_eq!(content_length, Some(13));
            }
            _ => panic!("Expected Body variant"),
        }
    }

    #[tokio::test]
    async fn test_raw_body() {
        // Base64 encoded "Hello"
        let data = "SGVsbG8=".to_string();
        let content_type = "application/octet-stream".to_string();
        let body_type = RequestBodyType::Raw { data, content_type: content_type.clone() };
        
        let result = BodyBuilder::build(&body_type).await.unwrap();
        match result {
            HttpRequestBody::Body { content_type: ct, content_length, .. } => {
                assert_eq!(ct, content_type);
                assert_eq!(content_length, Some(5));
            }
            _ => panic!("Expected Body variant"),
        }
    }

    #[tokio::test]
    async fn test_multipart_body() {
        let mut fields = HashMap::new();
        fields.insert("description".to_string(), "Test upload".to_string());
        
        let mut files = HashMap::new();
        files.insert("file".to_string(), FileField {
            filename: "test.txt".to_string(),
            mime_type: "text/plain".to_string(),
            content: "SGVsbG8gV29ybGQ=".to_string(), // "Hello World" in base64
        });
        
        let body_type = RequestBodyType::Multipart { fields, files };
        let result = BodyBuilder::build(&body_type).await.unwrap();
        
        match result {
            HttpRequestBody::Multipart { .. } => {
                // Success - multipart form was created
            }
            _ => panic!("Expected Multipart variant"),
        }
    }

    #[test]
    fn test_content_type_detection() {
        // Should detect form data
        let form_like = json!({"name": "test", "email": "test@example.com"});
        let detected = BodyBuilder::detect_content_type_from_json(&form_like);
        matches!(detected, RequestBodyType::Form { .. });
        
        // Should default to JSON
        let complex_json = json!({"user": {"name": "test", "age": 25}});
        let detected = BodyBuilder::detect_content_type_from_json(&complex_json);
        matches!(detected, RequestBodyType::Json { .. });
    }
}
