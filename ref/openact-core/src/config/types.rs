//! 配置类型定义 - 完全遵循 AWS EventBridge Connection 格式

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 多值支持 - 用于 Headers 和 Query Parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiValue {
    pub values: Vec<String>,
}

impl MultiValue {
    /// 创建新的多值
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// 创建单值
    pub fn single(value: impl Into<String>) -> Self {
        Self { values: vec![value.into()] }
    }

    /// 添加值
    pub fn add(&mut self, value: impl Into<String>) {
        self.values.push(value.into());
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// 获取第一个值（如果存在）
    pub fn first(&self) -> Option<&String> {
        self.values.first()
    }

    /// 获取所有值
    pub fn all(&self) -> &[String] {
        &self.values
    }
}


impl Default for MultiValue {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for MultiValue { fn from(s: String) -> Self { Self::single(s) } }
impl From<&str> for MultiValue { fn from(s: &str) -> Self { Self::single(s.to_string()) } }
impl From<Vec<String>> for MultiValue { fn from(values: Vec<String>) -> Self { Self { values } } }

/// 输出映射配置
// OutputMapping/JSONata 已下沉到上层解析层

/// HTTP 策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpPolicy {
    /// 禁用的头部（如 host、content-length）
    #[serde(default = "default_denied_headers")]
    pub denied_headers: Vec<String>,
    
    /// 保留头部（由系统/Connection 注入，如 authorization）
    #[serde(default)]
    pub reserved_headers: Vec<String>,
    
    /// 支持多值追加的头部（如 accept、cookie）
    #[serde(default = "default_multi_value_headers")]
    pub multi_value_append_headers: Vec<String>,
    
    /// 是否静默丢弃禁用头部（true=丢弃，false=报错）
    #[serde(default = "default_true")]
    pub drop_forbidden_headers: bool,
}

fn default_denied_headers() -> Vec<String> {
    vec![
        "host".to_string(),
        "content-length".to_string(),
        "transfer-encoding".to_string(),
        "expect".to_string(),
        "authorization".to_string(),
    ]
}

fn default_multi_value_headers() -> Vec<String> {
    vec![
        // 默认仅对 cookie 系列做追加，避免改变常见 Accept 语义
        "cookie".to_string(),
        "set-cookie".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            denied_headers: default_denied_headers(),
            reserved_headers: vec!["authorization".to_string()],
            multi_value_append_headers: default_multi_value_headers(),
            drop_forbidden_headers: true,
        }
    }
}

/// 超时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// 连接超时（毫秒）
    pub connect_ms: u64,
    
    /// 读取超时（毫秒）
    pub read_ms: u64,
    
    /// 总超时（毫秒）
    pub total_ms: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_ms: 10_000,  // 10秒
            read_ms: 30_000,     // 30秒
            total_ms: 60_000,    // 60秒
        }
    }
}

/// 抖动策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JitterStrategy {
    None,
    Full,
    Equal,
}

impl Default for JitterStrategy {
    fn default() -> Self {
        Self::Full
    }
}

/// 增强的重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: u32,
    
    /// 退避倍率
    pub backoff_rate: f64,
    
    /// 初始间隔（秒）
    pub interval_seconds: u64,
    
    /// 重试的 HTTP 状态码
    #[serde(default = "default_retry_status")]
    pub retry_on_status: Vec<u16>,
    
    /// 重试的错误类型
    #[serde(default = "default_retry_errors")]
    pub retry_on_errors: Vec<String>,
    
    /// 抖动策略
    #[serde(default)]
    pub jitter_strategy: JitterStrategy,
    
    /// 是否尊重 Retry-After 头
    #[serde(default = "default_true")]
    pub respect_retry_after: bool,
}

fn default_retry_status() -> Vec<u16> {
    vec![429, 503, 504]
}

fn default_retry_errors() -> Vec<String> {
    vec!["timeout".to_string(), "io".to_string(), "tls".to_string()]
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_rate: 2.0,
            interval_seconds: 1,
            retry_on_status: default_retry_status(),
            retry_on_errors: default_retry_errors(),
            jitter_strategy: JitterStrategy::default(),
            respect_retry_after: true,
        }
    }
}

/// 速率限制策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitPolicy {
    /// 每秒允许的请求数
    pub permit_per_second: f64,
    
    /// 突发容量
    pub burst: u32,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            permit_per_second: 10.0,
            burst: 10,
        }
    }
}

/// 熔断器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 失败阈值
    pub failure_threshold: u32,
    
    /// 恢复超时（秒）
    pub recovery_timeout_seconds: u64,
    
    /// 半开状态试探次数
    pub half_open_trial: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout_seconds: 60,
            half_open_trial: 3,
        }
    }
}

/// 安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    /// 是否启用幂等性
    #[serde(default)]
    pub idempotency: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            idempotency: false,
        }
    }
}

/// TLS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// 是否验证对等方证书
    #[serde(default = "default_true")]
    pub verify_peer: bool,
    
    /// 自定义 CA 证书（PEM 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_pem: Option<Vec<u8>>,
    
    /// 客户端证书（PEM 格式，用于 mTLS）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_cert_pem: Option<Vec<u8>>,
    
    /// 客户端私钥（PEM 格式，用于 mTLS）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key_pem: Option<Vec<u8>>,
    
    /// 服务器名称（用于 SNI）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            verify_peer: true,
            ca_pem: None,
            client_cert_pem: None,
            client_key_pem: None,
            server_name: None,
        }
    }
}

/// 网络配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// 代理 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    
    /// TLS 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
}

/// 响应策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePolicy {
    /// 是否允许二进制响应
    #[serde(default)]
    pub allow_binary: bool,
    
    /// 最大响应体大小（字节）
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    
    /// 二进制数据存储 TRN（超过大小限制时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_sink_trn: Option<String>,
}

fn default_max_body_bytes() -> usize {
    8 * 1024 * 1024  // 8MB
}

impl Default for ResponsePolicy {
    fn default() -> Self {
        Self {
            allow_binary: false,
            max_body_bytes: default_max_body_bytes(),
            binary_sink_trn: None,
        }
    }
}

/// 分页配置占位（动态表达式已下沉到上层）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationConfig {
    /// 最大页数限制
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pages: Option<u32>,
}

/// 检查级别
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InspectionLevel {
    Info,
    Debug,
    Trace,
}

impl Default for InspectionLevel {
    fn default() -> Self {
        Self::Info
    }
}

/// 测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// 检查级别
    #[serde(default)]
    pub inspection_level: InspectionLevel,
    
    /// 是否显示敏感信息
    #[serde(default)]
    pub reveal_secrets: bool,
    
    /// 是否为演练模式（不发送真实请求）
    #[serde(default)]
    pub dry_run: bool,
    
    /// 是否保存执行样例
    #[serde(default)]
    pub save_examples: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            inspection_level: InspectionLevel::default(),
            reveal_secrets: false,
            dry_run: false,
            save_examples: false,
        }
    }
}

/// 密钥引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    /// 存储密钥
    pub key: String,
    
    /// 版本号
    pub version: String,
}

impl SecretRef {
    pub fn new(key: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            version: version.into(),
        }
    }

    pub fn latest(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            version: "latest".to_string(),
        }
    }
}

/// 凭据类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Credential {
    /// 内联加密数据
    InlineEncrypted(String),
    
    /// 引用密钥存储
    Secret(SecretRef),
}

impl Credential {
    /// 创建内联加密凭据
    pub fn inline_encrypted(encrypted_data: impl Into<String>) -> Self {
        Self::InlineEncrypted(encrypted_data.into())
    }

    /// 创建密钥引用凭据
    pub fn secret_ref(key: impl Into<String>, version: impl Into<String>) -> Self {
        Self::Secret(SecretRef::new(key, version))
    }

    /// 创建最新版本的密钥引用
    pub fn secret_latest(key: impl Into<String>) -> Self {
        Self::Secret(SecretRef::latest(key))
    }

    /// 检查是否为内联凭据
    pub fn is_inline(&self) -> bool {
        matches!(self, Self::InlineEncrypted(_))
    }

    /// 检查是否为密钥引用
    pub fn is_secret_ref(&self) -> bool {
        matches!(self, Self::Secret(_))
    }

    /// 获取密钥引用（如果是）
    pub fn as_secret_ref(&self) -> Option<&SecretRef> {
        match self {
            Self::Secret(secret_ref) => Some(secret_ref),
            _ => None,
        }
    }
}

impl From<String> for Credential {
    fn from(s: String) -> Self {
        Self::InlineEncrypted(s)
    }
}

impl From<&str> for Credential {
    fn from(s: &str) -> Self {
        Self::InlineEncrypted(s.to_string())
    }
}

impl From<SecretRef> for Credential {
    fn from(secret_ref: SecretRef) -> Self {
        Self::Secret(secret_ref)
    }
}

/// OAuth2 授权类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OAuthGrantType {
    ClientCredentials,
    AuthorizationCode,
}

impl Default for OAuthGrantType {
    fn default() -> Self {
        Self::ClientCredentials
    }
}

/// AWS EventBridge 兼容的认证类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorizationType {
    ApiKey,
    OAuth,
    Basic,
}

/// 认证参数配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_auth_parameters: Option<ApiKeyAuthParameters>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub o_auth_parameters: Option<OAuthParameters>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basic_auth_parameters: Option<BasicAuthParameters>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_http_parameters: Option<InvocationHttpParameters>,
}

/// API Key 认证参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ApiKeyAuthParameters {
    pub api_key_name: String,
    pub api_key_value: Credential,
}

/// OAuth2 认证参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct OAuthParameters {
    pub client_id: Credential,
    pub client_secret: Credential,
    pub token_url: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    
    #[serde(default)]
    pub use_p_k_c_e: bool,
    
    #[serde(default)]
    pub grant_type: OAuthGrantType,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<Credential>,
    
    #[serde(default = "default_token_cache_ttl")]
    pub token_cache_ttl_sec: u64,
}

fn default_token_cache_ttl() -> u64 {
    3600  // 1小时
}

/// Basic 认证参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BasicAuthParameters {
    pub username: Credential,
    pub password: Credential,
}

/// Connection 级别的 HTTP 调用参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InvocationHttpParameters {
    #[serde(default)]
    pub header_parameters: Vec<HttpParameter>,
    
    #[serde(default)]
    pub query_string_parameters: Vec<HttpParameter>,
    
    #[serde(default)]
    pub body_parameters: Vec<HttpParameter>,
}

/// HTTP 参数的键值对（数组格式，用于配置导入/导出）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HttpParameter {
    pub key: String,
    pub value: String,
}

/// 键值对参数（简单字符串，兼容 AWS 格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValueParameter {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: String,
}

/// 数组格式的 HTTP 调用参数（用于 AWS EventBridge 兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ArrayInvocationHttpParameters {
    #[serde(default)]
    pub body_parameters: Vec<KeyValueParameter>,
    
    #[serde(default)]
    pub header_parameters: Vec<KeyValueParameter>,
    
    #[serde(default)]
    pub query_string_parameters: Vec<KeyValueParameter>,
}

/// Task 级别的 HTTP 参数（对象格式，支持多值）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHttpParameters {
    #[serde(default)]
    pub headers: HashMap<String, MultiValue>,
    
    #[serde(default)]
    pub query_parameters: HashMap<String, MultiValue>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<serde_json::Value>,
}

/// 转换函数：数组格式 -> 内部格式
impl From<ArrayInvocationHttpParameters> for InvocationHttpParameters {
    fn from(array_params: ArrayInvocationHttpParameters) -> Self {
        Self {
            header_parameters: convert_kv_array_to_http_params(array_params.header_parameters),
            query_string_parameters: convert_kv_array_to_http_params(array_params.query_string_parameters),
            body_parameters: convert_kv_array_to_http_params(array_params.body_parameters),
        }
    }
}

/// 转换函数：内部格式 -> 数组格式
impl From<InvocationHttpParameters> for ArrayInvocationHttpParameters {
    fn from(params: InvocationHttpParameters) -> Self {
        Self {
            header_parameters: convert_http_params_to_kv_array(params.header_parameters),
            query_string_parameters: convert_http_params_to_kv_array(params.query_string_parameters),
            body_parameters: convert_http_params_to_kv_array(params.body_parameters),
        }
    }
}

/// 辅助函数：将 KeyValueParameter 数组转换为 HttpParameter 数组
fn convert_kv_array_to_http_params(kv_params: Vec<KeyValueParameter>) -> Vec<HttpParameter> {
    kv_params
        .into_iter()
        .map(|kv| HttpParameter {
            key: kv.key,
            value: kv.value,
        })
        .collect()
}

/// 转换工具函数：String HashMap -> MultiValue HashMap
pub fn string_map_to_multivalue_map(map: HashMap<String, String>) -> HashMap<String, MultiValue> {
    map.into_iter().map(|(k, v)| (k, MultiValue::single(v))).collect()
}

/// 转换工具函数：MultiValue HashMap -> String HashMap（取第一个值）
pub fn multivalue_map_to_string_map(map: HashMap<String, MultiValue>) -> HashMap<String, String> {
    map.into_iter()
        .filter_map(|(k, mv)| mv.first().map(|v| (k, v.clone())))
        .collect()
}

/// Transform 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformConfig {
    pub request_body_encoding: RequestBodyEncoding,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_encoding_options: Option<RequestEncodingOptions>,
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            request_body_encoding: RequestBodyEncoding::default(),
            request_encoding_options: None,
        }
    }
}

/// 请求体编码类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestBodyEncoding {
    None,        // 默认 JSON 格式
    UrlEncoded,  // URL 编码格式
}

impl Default for RequestBodyEncoding {
    fn default() -> Self {
        Self::None
    }
}

/// 请求编码选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestEncodingOptions {
    pub array_format: ArrayFormat,
}

impl Default for RequestEncodingOptions {
    fn default() -> Self {
        Self {
            array_format: ArrayFormat::default(),
        }
    }
}

/// 数组编码格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArrayFormat {
    Indices,   // array[0]=a&array[1]=b&array[2]=c&array[3]=d
    Repeat,    // array=a&array=b&array=c&array=d
    Commas,    // array=a,b,c,d
    Brackets,  // array[]=a&array[]=b&array[]=c&array[]=d
}

impl Default for ArrayFormat {
    fn default() -> Self {
        Self::Indices
    }
}

/// 辅助函数：将 HttpParameter 数组转换为 KeyValueParameter 数组
fn convert_http_params_to_kv_array(http_params: Vec<HttpParameter>) -> Vec<KeyValueParameter> {
    http_params
        .into_iter()
        .map(|param| KeyValueParameter { key: param.key, value: param.value })
        .collect()
}