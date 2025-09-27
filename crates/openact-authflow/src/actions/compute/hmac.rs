use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Sha256, Sha384, Sha512};

use crate::actions::{MemorySecretsProvider, SecretsResolveHandler};
use crate::engine::TaskHandler;

#[derive(Default)]
pub struct ComputeHmacHandler;

impl TaskHandler for ComputeHmacHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let algo = ctx
            .get("algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("SHA256")
            .to_uppercase();

        let encoding = ctx
            .get("encoding")
            .and_then(|v| v.as_str())
            .unwrap_or("hex")
            .to_lowercase();

        // Resolve key (supports vault:// via secrets.resolve default provider)
        let key_in = ctx
            .get("key")
            .and_then(|v| v.as_str())
            .context("key required")?;
        let key_str = if key_in.starts_with("vault://") {
            let out = SecretsResolveHandler::<MemorySecretsProvider>::default().execute(
                "secrets.resolve",
                _state_name,
                &json!({"uri": key_in}),
            )?;
            match out.get("value") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => return Err(anyhow!("secrets.resolve returned no value")),
            }
        } else {
            key_in.to_string()
        };
        let key_bytes = key_str.as_bytes();

        // Message bytes
        let msg_bytes: Vec<u8> =
            if let Some(b64) = ctx.get("messageBase64").and_then(|v| v.as_str()) {
                STANDARD
                    .decode(b64)
                    .map_err(|e| anyhow!("invalid base64 message: {e}"))?
            } else if let Some(m) = ctx.get("message").and_then(|v| v.as_str()) {
                m.as_bytes().to_vec()
            } else {
                return Err(anyhow!("message or messageBase64 required"));
            };

        // Compute
        let sig = match algo.as_str() {
            "SHA256" => {
                let mut mac = Hmac::<Sha256>::new_from_slice(key_bytes)
                    .map_err(|e| anyhow!("bad key: {e}"))?;
                mac.update(&msg_bytes);
                mac.finalize().into_bytes().to_vec()
            }
            "SHA384" => {
                let mut mac = Hmac::<Sha384>::new_from_slice(key_bytes)
                    .map_err(|e| anyhow!("bad key: {e}"))?;
                mac.update(&msg_bytes);
                mac.finalize().into_bytes().to_vec()
            }
            "SHA512" => {
                let mut mac = Hmac::<Sha512>::new_from_slice(key_bytes)
                    .map_err(|e| anyhow!("bad key: {e}"))?;
                mac.update(&msg_bytes);
                mac.finalize().into_bytes().to_vec()
            }
            other => return Err(anyhow!("unsupported algorithm: {other}")),
        };

        let signature = match encoding.as_str() {
            "hex" => hex::encode(sig),
            "base64" => STANDARD.encode(sig),
            other => return Err(anyhow!("unsupported encoding: {other}")),
        };

        Ok(json!({"signature": signature}))
    }
}
