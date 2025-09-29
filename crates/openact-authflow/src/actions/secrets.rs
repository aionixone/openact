use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::engine::TaskHandler;

// SecretsProvider provides the minimal capability: resolving secret values based on a URI
pub trait SecretsProvider: Send + Sync + 'static {
    fn resolve(&self, uri: &str) -> Result<String>;
}

// An in-memory implementation for testing and local use
#[derive(Default, Clone)]
pub struct MemorySecretsProvider {
    items: std::sync::Arc<std::collections::HashMap<String, String>>,
}

impl MemorySecretsProvider {
    pub fn from_pairs(pairs: Vec<(&str, &str)>) -> Self {
        let mut map = std::collections::HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v.to_string());
        }
        Self { items: std::sync::Arc::new(map) }
    }
}

impl SecretsProvider for MemorySecretsProvider {
    fn resolve(&self, uri: &str) -> Result<String> {
        self.items.get(uri).cloned().ok_or_else(|| anyhow!("secret not found for uri: {}", uri))
    }
}

// secrets.resolve handler: input { uri }, output { value }
pub struct SecretsResolveHandler<P: SecretsProvider = MemorySecretsProvider> {
    pub provider: P,
}

impl<P: SecretsProvider + Default> Default for SecretsResolveHandler<P> {
    fn default() -> Self {
        Self { provider: P::default() }
    }
}

impl<P: SecretsProvider> SecretsResolveHandler<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

impl<P: SecretsProvider> TaskHandler for SecretsResolveHandler<P> {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let uri = ctx.get("uri").and_then(|v| v.as_str()).context("uri required")?;
        let v = resolve_one(&self.provider, uri)?;
        Ok(json!({ "value": v }))
    }
}

// Optional: Vault backend implementation (feature = "vault")
#[cfg(feature = "vault")]
pub struct VaultSecretsProvider {
    client: std::sync::Arc<vaultrs::client::VaultClient>,
}

#[cfg(feature = "vault")]
impl VaultSecretsProvider {
    pub fn new(client: vaultrs::client::VaultClient) -> Self {
        Self { client: std::sync::Arc::new(client) }
    }
}

#[cfg(feature = "vault")]
impl SecretsProvider for VaultSecretsProvider {
    fn resolve(&self, uri: &str) -> Result<String> {
        // Convention: vault://<mount>/<path>[#pointer]
        let (base, _ptr) = match uri.split_once('#') {
            Some((b, p)) => (b, Some(p)),
            None => (uri, None),
        };
        let path = base.strip_prefix("vault://").ok_or_else(|| anyhow!("invalid vault uri"))?;
        // Simplification: assume KV v2, mount is the first segment
        let mut parts = path.splitn(2, '/');
        let mount = parts.next().ok_or_else(|| anyhow!("invalid vault uri mount"))?;
        let rest = parts.next().ok_or_else(|| anyhow!("invalid vault uri path"))?;
        // Fetch secret data
        let rt = tokio::runtime::Runtime::new()?;
        let data: serde_json::Value = rt.block_on(async {
            use vaultrs::kv2;
            kv2::read::<serde_json::Value>(&*self.client, mount, rest).await
        })?;
        Ok(data.to_string())
    }
}

fn resolve_one<P: SecretsProvider>(provider: &P, uri: &str) -> Result<Value> {
    // Support uri#json.pointer syntax to extract sub-paths from JSON
    let (base, pointer_opt) = match uri.split_once('#') {
        Some((b, p)) => (b, Some(p)),
        None => (uri, None),
    };
    let raw = provider.resolve(base)?;
    if let Some(ptr) = pointer_opt {
        let val: Value = serde_json::from_str(&raw)
            .map_err(|_| anyhow!("secret at {base} is not valid JSON for pointer"))?;
        let sub =
            val.pointer(ptr).cloned().ok_or_else(|| anyhow!("json pointer not found: {}", ptr))?;
        Ok(sub)
    } else {
        // Attempt to parse the string as JSON; return as plain text if it fails
        match serde_json::from_str::<Value>(&raw) {
            Ok(v) => Ok(v),
            Err(_) => Ok(Value::String(raw)),
        }
    }
}

// Batch resolve: input { items: { k1: uri1, k2: uri2 } }, output { values: { k1: value1, k2: value2 } }
pub struct SecretsResolveManyHandler<P: SecretsProvider = MemorySecretsProvider> {
    pub provider: P,
}

impl<P: SecretsProvider + Default> Default for SecretsResolveManyHandler<P> {
    fn default() -> Self {
        Self { provider: P::default() }
    }
}

impl<P: SecretsProvider> SecretsResolveManyHandler<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

impl<P: SecretsProvider> TaskHandler for SecretsResolveManyHandler<P> {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let items =
            ctx.get("items").and_then(|v| v.as_object()).context("items required as object")?;
        let mut out = serde_json::Map::new();
        for (k, v) in items.iter() {
            let uri = v.as_str().ok_or_else(|| anyhow!("invalid uri for key {}", k))?;
            let val = resolve_one(&self.provider, uri)
                .with_context(|| format!("resolve failed for {}", k))?;
            out.insert(k.clone(), val);
        }
        Ok(json!({ "values": Value::Object(out) }))
    }
}

// no re-exports from engine; this file is the source of truth now
