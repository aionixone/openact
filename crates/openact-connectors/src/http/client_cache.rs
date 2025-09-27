//! HTTP Client caching for connection-specific configurations

use crate::error::{ConnectorError, ConnectorResult};
use crate::http::connection::HttpConnection;
use crate::http::timeout_manager::TimeoutManager;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Configuration hash for client caching
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClientConfig {
    connect_timeout_ms: u64,
    total_timeout_ms: u64,
    proxy_url: Option<String>,
    verify_peer: bool,
    // TODO: Add more fields for custom CA, client certificates when supported
}

impl ClientConfig {
    /// Extract client configuration from HttpConnection
    fn from_connection(connection: &HttpConnection) -> Self {
        let (connect_timeout_ms, total_timeout_ms) = if let Some(timeout_config) = &connection.timeout_config {
            (timeout_config.connect_ms, timeout_config.total_ms)
        } else {
            (30000, 300000) // Default timeouts
        };

        let (proxy_url, verify_peer) = if let Some(network_config) = &connection.network_config {
            let proxy_url = network_config.proxy_url.clone();
            let verify_peer = network_config.tls.as_ref()
                .map(|tls| tls.verify_peer)
                .unwrap_or(true);
            (proxy_url, verify_peer)
        } else {
            (None, true) // Default network config
        };

        Self {
            connect_timeout_ms,
            total_timeout_ms,
            proxy_url,
            verify_peer,
        }
    }

    /// Build a reqwest::Client from this configuration
    fn build_client(&self) -> ConnectorResult<Client> {
        // Create timeout manager and apply connection-level timeouts
        let timeout_config = super::connection::TimeoutConfig {
            connect_ms: self.connect_timeout_ms,
            read_ms: 0, // Read timeout will be handled per-request
            total_ms: self.total_timeout_ms,
        };
        let timeout_manager = TimeoutManager::new(timeout_config);
        
        // Validate the timeout configuration
        timeout_manager.validate()?;
        
        // Apply timeouts to client builder (connection timeout only)
        let mut builder = timeout_manager.apply_to_client_builder(Client::builder());

        // Apply proxy configuration
        if let Some(proxy_url) = &self.proxy_url {
            if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                builder = builder.proxy(proxy);
            } else {
                return Err(ConnectorError::InvalidConfig(format!(
                    "Invalid proxy URL: {}",
                    proxy_url
                )));
            }
        }

        // Apply TLS configuration
        builder = builder.danger_accept_invalid_certs(!self.verify_peer);

        Ok(builder.build()?)
    }
}

/// Client cache that maintains connection-specific HTTP clients
#[derive(Debug, Clone)]
pub struct ClientCache {
    cache: Arc<RwLock<HashMap<ClientConfig, Arc<Client>>>>,
}

impl ClientCache {
    /// Create a new empty client cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a client for the given connection
    pub fn get_client(&self, connection: &HttpConnection) -> ConnectorResult<Arc<Client>> {
        let config = ClientConfig::from_connection(connection);

        // Try to get from cache first (read lock)
        {
            let cache = self.cache.read().unwrap();
            if let Some(client) = cache.get(&config) {
                return Ok(client.clone());
            }
        }

        // Not in cache, need to build and insert (write lock)
        let mut cache = self.cache.write().unwrap();
        
        // Double-check in case another thread added it while we were waiting
        if let Some(client) = cache.get(&config) {
            return Ok(client.clone());
        }

        // Build new client and cache it
        let client = Arc::new(config.build_client()?);
        cache.insert(config, client.clone());
        
        Ok(client)
    }

    /// Get cache statistics for monitoring
    pub fn stats(&self) -> ClientCacheStats {
        let cache = self.cache.read().unwrap();
        ClientCacheStats {
            cached_clients: cache.len(),
        }
    }

    /// Clear the cache (useful for testing or configuration changes)
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }
}

impl Default for ClientCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the client cache
#[derive(Debug, Clone)]
pub struct ClientCacheStats {
    pub cached_clients: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::connection::{NetworkConfig, TimeoutConfig, TlsConfig};

    #[test]
    fn test_client_config_equality() {
        let config1 = ClientConfig {
            connect_timeout_ms: 5000,
            total_timeout_ms: 30000,
            proxy_url: Some("http://proxy:8080".to_string()),
            verify_peer: true,
        };

        let config2 = ClientConfig {
            connect_timeout_ms: 5000,
            total_timeout_ms: 30000,
            proxy_url: Some("http://proxy:8080".to_string()),
            verify_peer: true,
        };

        let config3 = ClientConfig {
            connect_timeout_ms: 10000, // Different timeout
            total_timeout_ms: 30000,
            proxy_url: Some("http://proxy:8080".to_string()),
            verify_peer: true,
        };

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_client_cache_basic() {
        let cache = ClientCache::new();

        let mut connection1 = HttpConnection::default();
        connection1.timeout_config = Some(TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 30000,
        });

        let mut connection2 = HttpConnection::default();
        connection2.timeout_config = Some(TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 30000,
        });

        // Same configuration should return the same client instance
        let client1 = cache.get_client(&connection1).unwrap();
        let client2 = cache.get_client(&connection2).unwrap();
        
        assert!(Arc::ptr_eq(&client1, &client2));

        // Cache should have 1 entry
        assert_eq!(cache.stats().cached_clients, 1);
    }

    #[test]
    fn test_client_cache_different_configs() {
        let cache = ClientCache::new();

        let mut connection1 = HttpConnection::default();
        connection1.timeout_config = Some(TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 30000,
        });

        let mut connection2 = HttpConnection::default();
        connection2.timeout_config = Some(TimeoutConfig {
            connect_ms: 10000, // Different timeout
            read_ms: 10000,
            total_ms: 30000,
        });

        let client1 = cache.get_client(&connection1).unwrap();
        let client2 = cache.get_client(&connection2).unwrap();
        
        // Different configurations should return different clients
        assert!(!Arc::ptr_eq(&client1, &client2));

        // Cache should have 2 entries
        assert_eq!(cache.stats().cached_clients, 2);
    }

    #[test]
    fn test_client_cache_with_proxy() {
        let cache = ClientCache::new();

        let mut connection = HttpConnection::default();
        connection.network_config = Some(NetworkConfig {
            proxy_url: Some("http://proxy:8080".to_string()),
            tls: Some(TlsConfig {
                verify_peer: false,
                ca_pem: None,
                client_cert_pem: None,
                client_key_pem: None,
                server_name: None,
            }),
        });

        let client = cache.get_client(&connection).unwrap();
        assert!(client.as_ref() as *const Client as usize != 0); // Just check it's not null

        // Second call should return cached client
        let client2 = cache.get_client(&connection).unwrap();
        assert!(Arc::ptr_eq(&client, &client2));
    }
}
