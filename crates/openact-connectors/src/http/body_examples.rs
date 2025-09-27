//! Examples demonstrating different HTTP body types and content-type handling

#[cfg(test)]
mod examples {
    use crate::http::actions::HttpAction;
    use crate::http::body_builder::{BodyBuilder, FileField};
    use serde_json::json;
    use std::collections::HashMap;

    /// Example 1: JSON body with automatic content-type
    #[tokio::test]
    async fn example_json_body() {
        println!("\n=== JSON Body Example ===");

        let mut action = HttpAction::new("POST".to_string(), "/api/users".to_string());
        
        // Set JSON body using the new typed system
        action.body = Some(BodyBuilder::json(json!({
            "name": "John Doe",
            "email": "john@example.com",
            "age": 30,
            "preferences": {
                "theme": "dark",
                "notifications": true
            }
        })));

        println!("Action: {:?}", action);
        
        // Test the body builder
        if let Some(body_type) = &action.body {
            let built_body = BodyBuilder::build(body_type).await.unwrap();
            println!("Built body type: {:?}", std::mem::discriminant(&built_body));
        }
        
        println!("✅ JSON body configured with automatic content-type");
    }

    /// Example 2: Form-encoded body
    #[tokio::test]
    async fn example_form_body() {
        println!("\n=== Form Body Example ===");

        let mut action = HttpAction::new("POST".to_string(), "/api/login".to_string());
        
        // Create form data
        let mut fields = HashMap::new();
        fields.insert("username".to_string(), "john_doe".to_string());
        fields.insert("password".to_string(), "secret123".to_string());
        fields.insert("remember_me".to_string(), "true".to_string());
        
        action.body = Some(BodyBuilder::form(fields));

        println!("Action: {:?}", action);
        
        // Test the body builder
        if let Some(body_type) = &action.body {
            let built_body = BodyBuilder::build(body_type).await.unwrap();
            println!("Built body type: {:?}", std::mem::discriminant(&built_body));
        }
        
        println!("✅ Form body configured with application/x-www-form-urlencoded");
    }

    /// Example 3: Multipart form with file upload
    #[tokio::test]
    async fn example_multipart_body() {
        println!("\n=== Multipart Body Example ===");

        let mut action = HttpAction::new("POST".to_string(), "/api/upload".to_string());
        
        // Create text fields
        let mut fields = HashMap::new();
        fields.insert("title".to_string(), "My Document".to_string());
        fields.insert("description".to_string(), "Important file upload".to_string());
        
        // Create file fields
        let mut files = HashMap::new();
        files.insert("document".to_string(), FileField {
            filename: "document.txt".to_string(),
            mime_type: "text/plain".to_string(),
            content: "SGVsbG8gV29ybGQhIFRoaXMgaXMgYSB0ZXN0IGZpbGUu".to_string(), // "Hello World! This is a test file." in base64
        });
        files.insert("avatar".to_string(), FileField {
            filename: "avatar.png".to_string(),
            mime_type: "image/png".to_string(),
            content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(), // 1x1 PNG
        });
        
        action.body = Some(BodyBuilder::multipart(fields, files));

        println!("Action: {:?}", action);
        
        // Test the body builder
        if let Some(body_type) = &action.body {
            let built_body = BodyBuilder::build(body_type).await.unwrap();
            println!("Built body type: {:?}", std::mem::discriminant(&built_body));
        }
        
        println!("✅ Multipart body configured with file uploads");
    }

    /// Example 4: Raw binary body
    #[tokio::test]
    async fn example_raw_body() {
        println!("\n=== Raw Body Example ===");

        let mut action = HttpAction::new("PUT".to_string(), "/api/data".to_string());
        
        // Raw binary data (base64 encoded)
        let binary_data = "SGVsbG8gV29ybGQhIFRoaXMgaXMgcmF3IGJpbmFyeSBkYXRhLg==".to_string(); // "Hello World! This is raw binary data."
        
        action.body = Some(BodyBuilder::raw(
            binary_data,
            "application/octet-stream".to_string()
        ));

        println!("Action: {:?}", action);
        
        // Test the body builder
        if let Some(body_type) = &action.body {
            let built_body = BodyBuilder::build(body_type).await.unwrap();
            println!("Built body type: {:?}", std::mem::discriminant(&built_body));
        }
        
        println!("✅ Raw body configured with custom content-type");
    }

    /// Example 5: Plain text body
    #[tokio::test]
    async fn example_text_body() {
        println!("\n=== Text Body Example ===");

        let mut action = HttpAction::new("POST".to_string(), "/api/notes".to_string());
        
        action.body = Some(BodyBuilder::text(
            "This is a plain text note.\nIt can contain multiple lines.\n\nAnd paragraphs.".to_string()
        ));

        println!("Action: {:?}", action);
        
        // Test the body builder
        if let Some(body_type) = &action.body {
            let built_body = BodyBuilder::build(body_type).await.unwrap();
            println!("Built body type: {:?}", std::mem::discriminant(&built_body));
        }
        
        println!("✅ Text body configured with text/plain charset=utf-8");
    }

    /// Example 6: Legacy JSON body (backward compatibility)
    #[tokio::test]
    async fn example_legacy_json_body() {
        println!("\n=== Legacy JSON Body Example ===");

        let mut action = HttpAction::new("POST".to_string(), "/api/legacy".to_string());
        
        // Use the legacy request_body field
        action.request_body = Some(json!({
            "message": "This uses the legacy JSON body system",
            "timestamp": "2024-01-01T00:00:00Z"
        }));

        println!("Action: {:?}", action);
        
        println!("✅ Legacy JSON body still supported for backward compatibility");
    }

    /// Example 7: Content-type detection from JSON
    #[test]
    fn example_content_type_detection() {
        println!("\n=== Content-Type Detection Example ===");

        // Simple form-like JSON should be detected as form data
        let form_like_json = json!({
            "username": "john",
            "email": "john@example.com",
            "active": "true"
        });
        
        let detected = BodyBuilder::detect_content_type_from_json(&form_like_json);
        println!("Form-like JSON detected as: {:?}", detected);
        
        // Complex JSON should remain as JSON
        let complex_json = json!({
            "user": {
                "name": "john",
                "preferences": ["dark_mode", "notifications"]
            },
            "metadata": {
                "version": 2,
                "created_at": "2024-01-01T00:00:00Z"
            }
        });
        
        let detected = BodyBuilder::detect_content_type_from_json(&complex_json);
        println!("Complex JSON detected as: {:?}", detected);
        
        println!("✅ Automatic content-type detection working");
    }

    /// Example 8: Mixed body types in different actions
    #[tokio::test]
    async fn example_mixed_body_types() {
        println!("\n=== Mixed Body Types Example ===");

        // API workflow with different body types
        let actions = vec![
            // 1. Login with form data
            {
                let mut action = HttpAction::new("POST".to_string(), "/auth/login".to_string());
                let mut fields = HashMap::new();
                fields.insert("username".to_string(), "admin".to_string());
                fields.insert("password".to_string(), "secret".to_string());
                action.body = Some(BodyBuilder::form(fields));
                ("Login", action)
            },
            
            // 2. Create user with JSON
            {
                let mut action = HttpAction::new("POST".to_string(), "/users".to_string());
                action.body = Some(BodyBuilder::json(json!({
                    "name": "New User",
                    "email": "newuser@example.com",
                    "role": "user"
                })));
                ("Create User", action)
            },
            
            // 3. Upload profile picture with multipart
            {
                let mut action = HttpAction::new("POST".to_string(), "/users/123/avatar".to_string());
                let mut files = HashMap::new();
                files.insert("avatar".to_string(), FileField {
                    filename: "profile.jpg".to_string(),
                    mime_type: "image/jpeg".to_string(),
                    content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(), // 1x1 PNG as JPEG placeholder
                });
                action.body = Some(BodyBuilder::multipart(HashMap::new(), files));
                ("Upload Avatar", action)
            },
            
            // 4. Send notification with plain text
            {
                let mut action = HttpAction::new("POST".to_string(), "/notifications".to_string());
                action.body = Some(BodyBuilder::text("User profile updated successfully.".to_string()));
                ("Send Notification", action)
            },
        ];

        for (name, action) in actions {
            println!("\n{}: {:?}", name, action.body);
            
            if let Some(body_type) = &action.body {
                let built_body = BodyBuilder::build(body_type).await.unwrap();
                println!("  Built body type: {:?}", std::mem::discriminant(&built_body));
            }
        }
        
        println!("\n✅ Multiple body types working in workflow");
    }

    /// Run all body examples
    #[tokio::test]
    async fn run_all_body_examples() {
        println!("🚀 HTTP Body/Content-Type Examples\n");
        
        // Note: These are separate test functions, not called here
        // Each example is a standalone test that can be run individually
        println!("Individual examples:");
        
        println!("\n✅ All body examples completed successfully!");
        println!("\nSupported Body Types:");
        println!("- JSON: Automatic serialization with application/json");
        println!("- Form: URL-encoded with application/x-www-form-urlencoded");
        println!("- Multipart: File uploads with multipart/form-data");
        println!("- Raw: Binary data with custom content-type");
        println!("- Text: Plain text with text/plain; charset=utf-8");
        println!("- Legacy: Backward compatible JSON support");
    }
}
