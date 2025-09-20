use anyhow::{Context, Result, anyhow};
use aws_credential_types::Credentials;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings};
use aws_sigv4::sign::v4;
use serde_json::{Value, json};
use std::time::SystemTime;

use crate::authflow::actions::secrets::{MemorySecretsProvider, SecretsResolveHandler};
use crate::authflow::engine::TaskHandler;

#[derive(Default)]
pub struct ComputeSigV4Handler;

impl TaskHandler for ComputeSigV4Handler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // input: { region, service, accessKey, secretKey|vault://, sessionToken?, request:{ method, url, headers?:{}, body?:string } }
        let region = ctx
            .get("region")
            .and_then(|v| v.as_str())
            .context("region required")?;
        let service = ctx
            .get("service")
            .and_then(|v| v.as_str())
            .context("service required")?;
        let access_key = ctx
            .get("accessKey")
            .and_then(|v| v.as_str())
            .context("accessKey required")?;
        let secret_key_in = ctx
            .get("secretKey")
            .and_then(|v| v.as_str())
            .context("secretKey required")?;

        // Resolve secret key if it's a vault reference
        let secret_key = if secret_key_in.starts_with("vault://") {
            let out = SecretsResolveHandler::<MemorySecretsProvider>::default().execute(
                "secrets.resolve",
                _state_name,
                &json!({"uri": secret_key_in}),
            )?;
            out.get("value")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow!("secret resolve empty"))?
        } else {
            secret_key_in.to_string()
        };

        let session_token = ctx.get("sessionToken").and_then(|v| v.as_str());

        // Parse request details
        let req = ctx
            .get("request")
            .and_then(|v| v.as_object())
            .context("request required")?;
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let url = req
            .get("url")
            .and_then(|v| v.as_str())
            .context("request.url required")?;

        // Parse headers - collect into owned strings to avoid lifetime issues
        let mut header_pairs = Vec::new();
        if let Some(h) = req.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in h.iter() {
                let val_s = v
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string());
                header_pairs.push((k.clone(), val_s));
            }
        }

        // Convert to references for SignableRequest
        let headers: Vec<(&str, &str)> = header_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Parse body
        let body = req
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .as_bytes();

        // Create signable request
        let signable = SignableRequest::new(
            method,
            url,
            headers.iter().map(|(k, v)| (*k, *v)),
            SignableBody::Bytes(body),
        )?;

        // Create credentials
        let credentials = Credentials::new(
            access_key,
            &secret_key,
            session_token.map(|s| s.to_string()),
            None,
            "openact",
        );

        // Create signing params using the builder pattern
        let identity = &credentials.into();
        let v4_params = v4::SigningParams::builder()
            .identity(identity)
            .region(region)
            .name(service)
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()?;
        let params = aws_sigv4::http_request::SigningParams::from(v4_params);

        // Sign the request
        let signing_output = aws_sigv4::http_request::sign(signable, &params)?;

        // Extract signed headers from instructions
        let mut out_headers = serde_json::Map::new();
        for (name, value) in signing_output.output().headers() {
            out_headers.insert(name.to_string(), json!(value));
        }

        Ok(json!({"headers": Value::Object(out_headers)}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compute_sigv4_basic() {
        let handler = ComputeSigV4Handler;
        let input = json!({
            "region": "us-east-1",
            "service": "s3",
            "accessKey": "AKIAIOSFODNN7EXAMPLE",
            "secretKey": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "request": {
                "method": "GET",
                "url": "https://examplebucket.s3.amazonaws.com/test.txt",
                "headers": {
                    "Host": "examplebucket.s3.amazonaws.com"
                }
            }
        });

        let result = handler.execute("compute.sigv4", "TestState", &input);
        assert!(result.is_ok());

        let output = result.unwrap();
        let headers = output.get("headers").unwrap().as_object().unwrap();

        // Should have Authorization header
        assert!(headers.contains_key("authorization"));
        let auth = headers.get("authorization").unwrap().as_str().unwrap();
        assert!(auth.starts_with("AWS4-HMAC-SHA256"));

        // Should have x-amz-date header
        assert!(headers.contains_key("x-amz-date"));
    }

    #[test]
    fn test_compute_sigv4_with_session_token() {
        let handler = ComputeSigV4Handler;
        let input = json!({
            "region": "us-west-2",
            "service": "dynamodb",
            "accessKey": "AKIAIOSFODNN7EXAMPLE",
            "secretKey": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "sessionToken": "AQoDYXdzEJr...",
            "request": {
                "method": "POST",
                "url": "https://dynamodb.us-west-2.amazonaws.com/",
                "headers": {
                    "Content-Type": "application/x-amz-json-1.0",
                    "X-Amz-Target": "DynamoDB_20120810.ListTables"
                },
                "body": "{}"
            }
        });

        let result = handler.execute("compute.sigv4", "TestState", &input);
        assert!(result.is_ok());

        let output = result.unwrap();
        let headers = output.get("headers").unwrap().as_object().unwrap();

        // Should have Authorization and x-amz-security-token headers
        assert!(headers.contains_key("authorization"));
        assert!(headers.contains_key("x-amz-security-token"));
    }

    #[test]
    fn test_compute_sigv4_missing_region() {
        let handler = ComputeSigV4Handler;
        let input = json!({
            "service": "s3",
            "accessKey": "test",
            "secretKey": "test",
            "request": {
                "method": "GET",
                "url": "https://example.com"
            }
        });

        let result = handler.execute("compute.sigv4", "TestState", &input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("region required"));
    }
}
