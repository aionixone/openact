//! HTTP Client 池（按 Timeout/Network/TLS 组合复用）

use anyhow::{Context, Result, anyhow};
use reqwest::{Client, Proxy};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::debug;

use crate::models::{ConnectionConfig, TaskConfig};

// Key -> (Client, last_access)
static CLIENT_POOL: OnceLock<Mutex<HashMap<String, (Client, Instant)>>> = OnceLock::new();

fn pool_capacity() -> usize {
    const DEFAULT_CAP: usize = 64;
    match std::env::var("OPENACT_CLIENT_POOL_CAPACITY") {
        Ok(v) => v
            .parse::<usize>()
            .ok()
            .filter(|c| *c > 0)
            .unwrap_or(DEFAULT_CAP),
        Err(_) => DEFAULT_CAP,
    }
}

fn pool_ttl_secs() -> u64 {
    const DEFAULT_TTL: u64 = 300; // 5 minutes
    match std::env::var("OPENACT_CLIENT_POOL_TTL_SECS") {
        Ok(v) => v.parse::<u64>().ok().unwrap_or(DEFAULT_TTL),
        Err(_) => DEFAULT_TTL,
    }
}

fn client_key(connection: &ConnectionConfig, task: &TaskConfig) -> String {
    let timeout = connection
        .timeout_config
        .as_ref()
        .or(task.timeout_config.as_ref());
    let network = connection
        .network_config
        .as_ref()
        .or(task.network_config.as_ref());
    let mut key = String::from("ua=OpenAct/0.1.0;");
    if let Some(t) = timeout {
        key.push_str(&format!(
            "ct={} rt={} tt={};",
            t.connect_ms, t.read_ms, t.total_ms
        ));
    }
    if let Some(n) = network {
        if let Some(p) = &n.proxy_url {
            key.push_str(&format!("proxy={};", p));
        }
        if let Some(tls) = &n.tls {
            key.push_str(&format!("vp={};", tls.verify_peer));
            key.push_str(&format!(
                "ca={};",
                tls.ca_pem.as_ref().map(|v| v.len()).unwrap_or(0)
            ));
            key.push_str(&format!("sn={};", tls.server_name.as_deref().unwrap_or("")));
        }
    }
    key
}

// Metrics
static HITS: AtomicU64 = AtomicU64::new(0);
static BUILDS: AtomicU64 = AtomicU64::new(0);
static EVICTIONS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct ClientPoolStats {
    pub hits: u64,
    pub builds: u64,
    pub evictions: u64,
    pub size: usize,
    pub capacity: usize,
}

pub fn get_stats() -> ClientPoolStats {
    let size = CLIENT_POOL
        .get()
        .and_then(|m| m.try_lock().ok().map(|g| g.len()))
        .unwrap_or(0);
    ClientPoolStats {
        hits: HITS.load(Ordering::Relaxed),
        builds: BUILDS.load(Ordering::Relaxed),
        evictions: EVICTIONS.load(Ordering::Relaxed),
        size,
        capacity: pool_capacity(),
    }
}

pub fn get_client_for(connection: &ConnectionConfig, task: &TaskConfig) -> Result<Client> {
    let pool = CLIENT_POOL.get_or_init(|| Mutex::new(HashMap::new()));
    let key = client_key(connection, task);
    // fast path: try lock and get
    if let Ok(mut guard) = pool.try_lock() {
        if let Some((c, ts)) = guard.get_mut(&key) {
            // update last access to improve LRU accuracy
            *ts = Instant::now();
            let client = c.clone();
            let size = guard.len();
            drop(guard);
            HITS.fetch_add(1, Ordering::Relaxed);
            debug!(target: "client_pool", hit=true, size=%size, "reuse http client");
            return Ok(client);
        }
    }

    // build new client
    let mut builder = Client::builder().user_agent("OpenAct/0.1.0");

    if let Some(t) = connection
        .timeout_config
        .as_ref()
        .or(task.timeout_config.as_ref())
    {
        builder = builder
            .connect_timeout(std::time::Duration::from_millis(t.connect_ms))
            .timeout(std::time::Duration::from_millis(t.total_ms));
    }

    if let Some(n) = connection
        .network_config
        .as_ref()
        .or(task.network_config.as_ref())
    {
        if let Some(p) = &n.proxy_url {
            builder = builder.proxy(Proxy::all(p).map_err(|e| anyhow!("invalid proxy: {}", e))?);
        }
        if let Some(tls) = &n.tls {
            if !tls.verify_peer {
                builder = builder.danger_accept_invalid_certs(true);
            }
            if let Some(ca) = &tls.ca_pem {
                let cert = reqwest::Certificate::from_pem(ca)
                    .map_err(|e| anyhow!("invalid ca pem: {}", e))?;
                builder = builder.add_root_certificate(cert);
            }
            // mTLS: 需要同时提供 client_cert_pem 与 client_key_pem
            if let (Some(cert_pem), Some(key_pem)) = (&tls.client_cert_pem, &tls.client_key_pem) {
                // 将 cert 和 key 拼接为一个 PEM 文本，供 reqwest::Identity 解析
                let mut combined = Vec::new();
                combined.extend_from_slice(cert_pem);
                if !combined.ends_with(b"\n") {
                    combined.extend_from_slice(b"\n");
                }
                combined.extend_from_slice(key_pem);
                let id = reqwest::Identity::from_pem(&combined)
                    .map_err(|e| anyhow!("invalid client cert/key pem: {}", e))?;
                builder = builder.identity(id);
            }
        }
    }

    let client = builder.build().context("Failed to create HTTP client")?;
    BUILDS.fetch_add(1, Ordering::Relaxed);
    debug!(target: "client_pool", build=true, "build new http client");
    // store in pool with LRU eviction (best-effort, avoid blocking in async context)
    if let Ok(mut guard) = pool.try_lock() {
        // cleanup stale entries by TTL
        let ttl = std::time::Duration::from_secs(pool_ttl_secs());
        let now = Instant::now();
        let mut stale: Vec<String> = Vec::new();
        for (k, (_c, ts)) in guard.iter() {
            if now.duration_since(*ts) > ttl {
                stale.push(k.clone());
            }
        }
        for k in stale {
            let _ = guard.remove(&k);
        }

        // insert current
        guard.insert(key.clone(), (client.clone(), Instant::now()));
        let cap = pool_capacity();
        if guard.len() > cap {
            // evict least-recently used (oldest last_access)
            if let Some(evict_key) = guard
                .iter()
                .min_by_key(|(_k, (_c, ts))| *ts)
                .map(|(k, _)| k.clone())
            {
                if evict_key != key {
                    let _ = guard.remove(&evict_key);
                    EVICTIONS.fetch_add(1, Ordering::Relaxed);
                    debug!(target: "client_pool", evict=true, size=%guard.len(), capacity=%cap, key=%evict_key, "evict least-recently used client");
                }
            }
        }
    }
    Ok(client)
}
