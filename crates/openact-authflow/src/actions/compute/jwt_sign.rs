use anyhow::{anyhow, Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::actions::{MemorySecretsProvider, SecretsResolveHandler};
use crate::engine::TaskHandler;

#[derive(Default)]
pub struct ComputeJwtSignHandler;

#[derive(Serialize, Deserialize, Default)]
struct GenericClaims(serde_json::Map<String, serde_json::Value>);

impl TaskHandler for ComputeJwtSignHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // Extract the algorithm from the context, defaulting to "HS256" if not provided
        let alg = ctx.get("alg").and_then(|v| v.as_str()).unwrap_or("HS256");
        // Clone the claims from the context or use an empty JSON object if not provided
        let claims = ctx.get("claims").cloned().unwrap_or_else(|| json!({}));
        // Ensure that claims is a JSON object
        if !claims.is_object() {
            return Err(anyhow!("claims must be object"));
        }

        // Retrieve the key from the context, ensuring it is present
        let key_in = ctx.get("key").and_then(|v| v.as_str()).context("key required")?;
        // Resolve the key material, either from a vault or directly from the input
        let key_material = if key_in.starts_with("vault://") {
            let out = SecretsResolveHandler::<MemorySecretsProvider>::default().execute(
                "secrets.resolve",
                _state_name,
                &json!({"uri": key_in}),
            )?;
            out.get("value")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow!("secret value must be string"))?
        } else {
            key_in.to_string()
        };

        // Initialize the JWT header with the specified algorithm
        let mut header = Header::new(match alg {
            "HS256" => Algorithm::HS256,
            "HS384" => Algorithm::HS384,
            "HS512" => Algorithm::HS512,
            "RS256" => Algorithm::RS256,
            _ => return Err(anyhow!("unsupported alg: {alg}")),
        });
        // Optionally set additional header fields if provided in the context
        if let Some(h) = ctx.get("header").and_then(|v| v.as_object()) {
            if let Some(kid) = h.get("kid").and_then(|v| v.as_str()) {
                header.kid = Some(kid.to_string());
            }
            // Optionally set the "typ" field in the header
            if let Some(typ) = h.get("typ").and_then(|v| v.as_str()) {
                header.typ = Some(typ.to_string());
            }
            // Optionally set the "cty" field in the header
            if let Some(cty) = h.get("cty").and_then(|v| v.as_str()) {
                header.cty = Some(cty.to_string());
            }
        }

        // Encode the JWT using the appropriate key based on the algorithm
        let token = match header.alg {
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
                let key = EncodingKey::from_secret(key_material.as_bytes());
                let claims_map: Map<String, Value> =
                    claims.as_object().cloned().unwrap_or_default();
                let gc = GenericClaims(claims_map);
                encode(&header, &gc, &key).map_err(|e| anyhow!("jwt encode failed: {e}"))?
            }
            Algorithm::RS256 => {
                // Expect key_material to be a PEM private key
                let key = EncodingKey::from_rsa_pem(key_material.as_bytes())
                    .map_err(|e| anyhow!("invalid rsa pem: {e}"))?;
                let claims_map: Map<String, Value> =
                    claims.as_object().cloned().unwrap_or_default();
                let gc = GenericClaims(claims_map);
                encode(&header, &gc, &key).map_err(|e| anyhow!("jwt encode failed: {e}"))?
            }
            _ => return Err(anyhow!("unsupported alg: {alg}")),
        };

        // Return the encoded JWT token as a JSON object
        Ok(json!({"token": token}))
    }
}
