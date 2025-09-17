use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{Rng, distributions::Alphanumeric};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::engine::TaskHandler;

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
        // Expect code/state in canonical location written by callback handler
        let code = ctx
            .pointer("/vars/meta/oauth/code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("PAUSE_FOR_CALLBACK"))?;

        let returned_state = ctx.pointer("/vars/meta/oauth/state").and_then(|v| v.as_str());
        let expected_state = ctx
            .pointer("/vars/meta/oauth/expected_state")
            .and_then(|v| v.as_str());

        if let (Some(r), Some(e)) = (returned_state, expected_state) {
            if r != e {
                return Err(anyhow::anyhow!(
                    "state mismatch: returned={}, expected={}",
                    r, e
                ));
            }
        }

        let mut out = json!({ "code": code });
        if let Some(verifier) = ctx
            .pointer("/vars/meta/oauth/code_verifier")
            .and_then(|v| v.as_str())
        {
            out["code_verifier"] = Value::String(verifier.to_string());
        }
        Ok(out)
    }
}
