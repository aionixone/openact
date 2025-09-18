//! HTTP 客户端实现

use reqwest::{Client, Request, Response, Url};
use std::time::Duration;
use crate::error::{OpenActError, Result};
use crate::config::types::{NetworkConfig, TlsConfig};

/// HTTP 客户端配置
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub user_agent: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(10),
            user_agent: "OpenAct/1.0".to_string(),
        }
    }
}

/// HTTP 客户端
#[derive(Debug)]
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
}

impl HttpClient {
    /// 创建新的 HTTP 客户端
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        let builder = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .user_agent(&config.user_agent);

        // TODO: 注入全局 Network/TLS 配置（占位接口）
        // builder = Self::apply_network(builder, None)?;

        let client = builder.build()
            .map_err(|e| OpenActError::network(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// 执行 HTTP 请求
    pub async fn execute(&self, request: Request) -> Result<Response> {
        self.client
            .execute(request)
            .await
            .map_err(|e| OpenActError::network(format!("HTTP request failed: {}", e)))
    }

    /// 获取内部 reqwest 客户端
    pub fn inner(&self) -> &Client {
        &self.client
    }

    /// 获取配置
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }
}

impl HttpClient {
    #[allow(dead_code)]
    fn apply_network(builder: reqwest::ClientBuilder, network: Option<&NetworkConfig>) -> Result<reqwest::ClientBuilder> {
        let mut b = builder;
        if let Some(net) = network {
            if let Some(proxy_url) = &net.proxy_url {
                if let Ok(url) = Url::parse(proxy_url) {
                    let proxy = reqwest::Proxy::all(url.as_str())
                        .map_err(|e| OpenActError::network(format!("invalid proxy: {}", e)))?;
                    b = b.proxy(proxy);
                }
            }
            if let Some(tls) = &net.tls {
                b = Self::apply_tls(b, tls)?;
            }
        }
        Ok(b)
    }

    fn apply_tls(builder: reqwest::ClientBuilder, tls: &TlsConfig) -> Result<reqwest::ClientBuilder> {
        let mut b = builder;
        if !tls.verify_peer {
            b = b.danger_accept_invalid_certs(true);
        }
        // mTLS/CA/SNI 可在后续映射到 reqwest/rustls。
        Ok(b)
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new(HttpClientConfig::default()).expect("Failed to create default HTTP client")
    }
}
