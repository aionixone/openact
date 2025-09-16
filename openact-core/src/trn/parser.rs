//! OpenAct TRN 解析器

use serde::{Deserialize, Serialize};
use crate::error::{OpenActError, Result};

/// OpenAct TRN 结构
/// 格式: trn:openact:{tenant}:{resource_type}/{resource_name}@{version}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenActTrn {
    pub tenant: String,
    pub resource_type: String,  // "connection" 或 "task"
    pub resource_name: String,
    pub version: String,
}

impl OpenActTrn {
    /// 创建新的 OpenAct TRN
    pub fn new(
        tenant: impl Into<String>,
        resource_type: impl Into<String>,
        resource_name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            resource_type: resource_type.into(),
            resource_name: resource_name.into(),
            version: version.into(),
        }
    }

    /// 转换为 TRN 字符串
    pub fn to_string(&self) -> String {
        format!(
            "trn:openact:{}:{}/{}@{}",
            self.tenant, self.resource_type, self.resource_name, self.version
        )
    }

    /// 检查是否为 Connection TRN
    pub fn is_connection(&self) -> bool {
        self.resource_type == "connection"
    }

    /// 检查是否为 Task TRN
    pub fn is_task(&self) -> bool {
        self.resource_type == "task"
    }

    /// 生成对应的 AuthFlow TRN
    /// OpenAct: trn:openact:tenant1:connection/github@v1
    ///     ↓
    /// AuthFlow: trn:authflow:tenant1:connection/github-{user_id}
    pub fn to_authflow_trn(&self, user_id: &str) -> Result<String> {
        if !self.is_connection() {
            return Err(OpenActError::trn("Only connection TRNs can be converted to AuthFlow TRNs"));
        }

        Ok(format!(
            "trn:authflow:{}:connection/{}-{}",
            self.tenant, self.resource_name, user_id
        ))
    }
}

/// TRN 解析器
pub struct TrnParser;

impl TrnParser {
    /// 解析 TRN 字符串
    pub fn parse(trn_str: &str) -> Result<OpenActTrn> {
        // 验证基本格式
        if !trn_str.starts_with("trn:openact:") {
            return Err(OpenActError::trn("TRN must start with 'trn:openact:'"));
        }

        // 分割组件
        let parts: Vec<&str> = trn_str.split(':').collect();
        if parts.len() != 4 {
            return Err(OpenActError::trn("Invalid TRN format, expected 4 parts separated by ':'"));
        }

        let tenant = parts[2];
        let resource_part = parts[3];

        // 解析资源部分: {resource_type}/{resource_name}@{version}
        let resource_parts: Vec<&str> = resource_part.split('/').collect();
        if resource_parts.len() != 2 {
            return Err(OpenActError::trn("Invalid resource format, expected 'type/name@version'"));
        }

        let resource_type = resource_parts[0];
        let name_version = resource_parts[1];

        // 解析名称和版本
        let name_version_parts: Vec<&str> = name_version.split('@').collect();
        if name_version_parts.len() != 2 {
            return Err(OpenActError::trn("Invalid name@version format"));
        }

        let resource_name = name_version_parts[0];
        let version = name_version_parts[1];

        // 验证资源类型
        match resource_type {
            "connection" | "task" => {}
            _ => return Err(OpenActError::trn(format!("Unsupported resource type: {}", resource_type))),
        }

        // 验证组件非空
        if tenant.is_empty() || resource_name.is_empty() || version.is_empty() {
            return Err(OpenActError::trn("TRN components cannot be empty"));
        }

        Ok(OpenActTrn::new(tenant, resource_type, resource_name, version))
    }

    /// 验证 TRN 字符串格式
    pub fn validate(trn_str: &str) -> Result<()> {
        Self::parse(trn_str)?;
        Ok(())
    }

    /// 检查两个 TRN 是否匹配模式
    pub fn matches_pattern(trn: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        // 支持简单的通配符匹配
        if pattern.contains('*') {
            // 将模式转换为正则表达式
            let regex_pattern = pattern.replace("*", ".*");
            if let Ok(regex) = regex::Regex::new(&regex_pattern) {
                return regex.is_match(trn);
            }
        }

        // 精确匹配或包含匹配
        trn == pattern || trn.contains(pattern)
    }

    /// 提取 TRN 的基础名称（不含版本）
    pub fn extract_base_name(trn_str: &str) -> Result<String> {
        let parsed = Self::parse(trn_str)?;
        Ok(format!(
            "trn:openact:{}:{}/{}",
            parsed.tenant, parsed.resource_type, parsed.resource_name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_trn() {
        let trn_str = "trn:openact:tenant1:connection/github@v1";
        let trn = TrnParser::parse(trn_str).unwrap();

        assert_eq!(trn.tenant, "tenant1");
        assert_eq!(trn.resource_type, "connection");
        assert_eq!(trn.resource_name, "github");
        assert_eq!(trn.version, "v1");
        assert!(trn.is_connection());
        assert!(!trn.is_task());
    }

    #[test]
    fn test_parse_task_trn() {
        let trn_str = "trn:openact:tenant1:task/list-repos@v2";
        let trn = TrnParser::parse(trn_str).unwrap();

        assert_eq!(trn.tenant, "tenant1");
        assert_eq!(trn.resource_type, "task");
        assert_eq!(trn.resource_name, "list-repos");
        assert_eq!(trn.version, "v2");
        assert!(!trn.is_connection());
        assert!(trn.is_task());
    }

    #[test]
    fn test_to_string() {
        let trn = OpenActTrn::new("tenant1", "connection", "github", "v1");
        assert_eq!(trn.to_string(), "trn:openact:tenant1:connection/github@v1");
    }

    #[test]
    fn test_to_authflow_trn() {
        let trn = OpenActTrn::new("tenant1", "connection", "github", "v1");
        let authflow_trn = trn.to_authflow_trn("user123").unwrap();
        assert_eq!(authflow_trn, "trn:authflow:tenant1:connection/github-user123");
    }

    #[test]
    fn test_invalid_trn_formats() {
        assert!(TrnParser::parse("invalid").is_err());
        assert!(TrnParser::parse("trn:openact:tenant1").is_err());
        assert!(TrnParser::parse("trn:openact:tenant1:invalid").is_err());
        assert!(TrnParser::parse("trn:openact:tenant1:connection/github").is_err());
        assert!(TrnParser::parse("trn:openact:tenant1:invalid_type/github@v1").is_err());
    }

    #[test]
    fn test_pattern_matching() {
        assert!(TrnParser::matches_pattern("trn:openact:tenant1:connection/github@v1", "*"));
        assert!(TrnParser::matches_pattern("trn:openact:tenant1:connection/github@v1", "trn:openact:tenant1:connection/*"));
        assert!(TrnParser::matches_pattern("trn:openact:tenant1:connection/github@v1", "*github*"));
        assert!(!TrnParser::matches_pattern("trn:openact:tenant1:connection/github@v1", "*slack*"));
    }

    #[test]
    fn test_extract_base_name() {
        let base = TrnParser::extract_base_name("trn:openact:tenant1:connection/github@v1").unwrap();
        assert_eq!(base, "trn:openact:tenant1:connection/github");
    }
}
