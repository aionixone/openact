use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{Rng, distributions::Alphanumeric};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::authflow::engine::TaskHandler;

#[derive(Default)]
pub struct OAuth2AuthorizeRedirectHandler;

// Generate a random alphanumeric string of length n
fn rand_string(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

// Generate a PKCE code challenge from a code verifier
fn pkce(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

impl TaskHandler for OAuth2AuthorizeRedirectHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // Extract necessary fields from the context
        let authorize = ctx
            .get("authorizeUrl")
            .and_then(|v| v.as_str())
            .context("authorizeUrl required")?;
        let client_id = ctx
            .get("clientId")
            .and_then(|v| v.as_str())
            .context("clientId required")?;
        let redirect_uri = ctx
            .get("redirectUri")
            .and_then(|v| v.as_str())
            .context("redirectUri required")?;
        let scope = ctx.get("scope").and_then(|v| v.as_str()).unwrap_or("");
        let use_pkce = ctx.get("usePKCE").and_then(|v| v.as_bool()).unwrap_or(true);
        let state = ctx
            .get("state")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| rand_string(24));

        // Construct the authorization URL
        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}",
            authorize,
            urlencoding::encode(client_id),
            urlencoding::encode(redirect_uri)
        );
        if !scope.is_empty() {
            url.push_str(&format!("&scope={}", urlencoding::encode(scope)));
        }
        url.push_str(&format!("&state={}", urlencoding::encode(&state)));

        // Prepare the output JSON
        let mut out = json!({ "authorize_url": url, "state": state });

        // If PKCE is used, generate and append the code challenge
        if use_pkce {
            let verifier = rand_string(43);
            let challenge = pkce(&verifier);
            let url_with_pkce = format!(
                "{}&code_challenge_method=S256&code_challenge={}",
                out["authorize_url"].as_str().unwrap(),
                urlencoding::encode(&challenge)
            );
            out["authorize_url"] = Value::String(url_with_pkce);
            out["code_verifier"] = Value::String(verifier);
            out["code_challenge"] = Value::String(challenge);
        }
        Ok(out)
    }
}

#[derive(Default)]
pub struct OAuth2AwaitCallbackHandler;

impl TaskHandler for OAuth2AwaitCallbackHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // Log the context for debugging
        println!("OAuth2AwaitCallbackHandler::execute called with context: {}", serde_json::to_string(ctx).unwrap_or_else(|_| "invalid json".to_string()));
        
        // Recursively find the code in the context
        fn find_code_recursive(ctx: &Value) -> Option<&str> {
            if let Some(code) = ctx.get("code").and_then(|v| v.as_str()) {
                return Some(code);
            }
            if let Some(input) = ctx.get("input") {
                if let Some(code) = find_code_recursive(input) {
                    return Some(code);
                }
            }
            if let Some(context) = ctx.get("context") {
                if let Some(code) = find_code_recursive(context) {
                    return Some(code);
                }
            }
            None
        }
        
        let code = find_code_recursive(ctx);
        println!("[await_cb] found code: {:?}", code);
        if code.is_none() {
            println!("[await_cb] no code found, returning PAUSE_FOR_CALLBACK");
            return Err(anyhow::anyhow!("PAUSE_FOR_CALLBACK"));
        }
        let code = code.unwrap();
        println!("[await_cb] using code: {}", code);

        // Recursively find the state in the context
        fn find_state_recursive(ctx: &Value) -> Option<&str> {
            if let Some(state) = ctx.get("state").and_then(|v| v.as_str()) {
                return Some(state);
            }
            if let Some(state) = ctx.get("returned_state").and_then(|v| v.as_str()) {
                return Some(state);
            }
            if let Some(input) = ctx.get("input") {
                if let Some(state) = find_state_recursive(input) {
                    return Some(state);
                }
            }
            if let Some(context) = ctx.get("context") {
                if let Some(state) = find_state_recursive(context) {
                    return Some(state);
                }
            }
            None
        }
        
        let returned = find_state_recursive(ctx);
        
        // Find the expected state, prioritizing the result from StartAuth
        fn find_expected_state(ctx: &Value) -> Option<&str> {
            // First, look for an explicit expected_state
            if let Some(state) = ctx.get("expected_state").and_then(|v| v.as_str()) {
                return Some(state);
            }
            
            // Then, look for the state in the StartAuth result
            fn find_start_auth_state(ctx: &Value) -> Option<&str> {
                if let Some(states) = ctx.get("states") {
                    if let Some(start_auth) = states.get("StartAuth") {
                        if let Some(result) = start_auth.get("result") {
                            if let Some(state) = result.get("state").and_then(|v| v.as_str()) {
                                return Some(state);
                            }
                        }
                    }
                }
                
                // Recursively search
                if let Some(obj) = ctx.as_object() {
                    for value in obj.values() {
                        if let Some(state) = find_start_auth_state(value) {
                            return Some(state);
                        }
                    }
                }
                None
            }
            
            find_start_auth_state(ctx)
        }
        
        let expected = find_expected_state(ctx);
        println!("[await_cb] state validation: returned={:?}, expected={:?}", returned, expected);
        
        // Validate the state only if an explicit expected_state is found
        if let (Some(r), Some(e)) = (returned, expected) {
            if r != e {
                println!("[await_cb] state mismatch: returned={}, expected={}", r, e);
                return Err(anyhow::anyhow!("state mismatch: returned={}, expected={}", r, e));
            }
            println!("[await_cb] state validation passed");
        } else {
            println!("[await_cb] skipping state validation (no expected state found)");
        }

        // Prepare the output JSON
        let mut out = json!({ "code": code });
        if let Some(v) = ctx
            .get("expected_pkce")
            .and_then(|o| o.get("code_verifier"))
            .and_then(|v| v.as_str())
        {
            out["code_verifier"] = Value::String(v.to_string());
        }
        println!("[await_cb] returning: {}", serde_json::to_string(&out).unwrap_or_else(|_| "invalid json".to_string()));
        Ok(out)
    }
}
