//! TRN helpers aligned with trn-rust: trn:{tool}:{tenant}:{resource_type}/{resource_id}
use anyhow::{anyhow, Result};

const TOOL: &str = "openact";

pub fn make_connection_trn(tenant: &str, id: &str) -> String {
    format!("trn:{}:{}:connection/{}", TOOL, tenant, id)
}

pub fn make_task_trn(tenant: &str, id: &str) -> String {
    format!("trn:{}:{}:task/{}", TOOL, tenant, id)
}

pub fn make_auth_ac_trn(tenant: &str, provider: &str, user_id: &str) -> String {
    // resource_id uses provider-user_id
    format!("trn:{}:{}:auth/{}-{}", TOOL, tenant, provider, user_id)
}

pub fn make_auth_cc_token_trn(tenant: &str, connection_id: &str) -> String {
    format!("trn:{}:{}:token/{}", TOOL, tenant, connection_id)
}

pub fn parse_tenant(trn: &str) -> Result<String> {
    let parts: Vec<&str> = trn.split(':').collect();
    if parts.len() < 4 || parts[0] != "trn" || parts[1] != TOOL {
        return Err(anyhow!("invalid trn format"));
    }
    Ok(parts[2].to_string())
}

pub fn parse_connection_trn(trn: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = trn.split(':').collect();
    if parts.len() < 4 || parts[0] != "trn" || parts[1] != TOOL {
        return Err(anyhow!("invalid trn format"));
    }
    let tenant = parts[2];
    // parts[3] should be like "connection/{id}"
    let rt_and_id = parts[3];
    let mut it = rt_and_id.splitn(2, '/');
    let rt = it.next().unwrap_or("");
    let id = it.next().unwrap_or("");
    if rt != "connection" || id.is_empty() {
        return Err(anyhow!("not a connection trn"));
    }
    Ok((tenant.to_string(), id.to_string()))
}

pub fn parse_task_trn(trn: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = trn.split(':').collect();
    if parts.len() < 4 || parts[0] != "trn" || parts[1] != TOOL {
        return Err(anyhow!("invalid trn format"));
    }
    let tenant = parts[2];
    // parts[3] should be like "task/{id}"
    let rt_and_id = parts[3];
    let mut it = rt_and_id.splitn(2, '/');
    let rt = it.next().unwrap_or("");
    let id = it.next().unwrap_or("");
    if rt != "task" || id.is_empty() {
        return Err(anyhow!("not a task trn"));
    }
    Ok((tenant.to_string(), id.to_string()))
}

pub fn validate_trn(trn: &str) -> Result<()> {
    let parts: Vec<&str> = trn.split(':').collect();
    if parts.len() < 4 {
        return Err(anyhow!("trn must have at least 4 parts separated by ':'"));
    }
    if parts[0] != "trn" {
        return Err(anyhow!("trn must start with 'trn:'"));
    }
    if parts[1] != TOOL {
        return Err(anyhow!("trn tool must be '{}'", TOOL));
    }
    if parts[2].is_empty() {
        return Err(anyhow!("trn tenant cannot be empty"));
    }
    let rt_and_id = parts[3];
    if !rt_and_id.contains('/') {
        return Err(anyhow!("trn resource must contain '/' to separate type and id"));
    }
    let mut it = rt_and_id.splitn(2, '/');
    let rt = it.next().unwrap_or("");
    let id = it.next().unwrap_or("");
    if rt.is_empty() || id.is_empty() {
        return Err(anyhow!("trn resource type and id cannot be empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trn_validation() {
        // Valid TRNs
        assert!(validate_trn("trn:openact:test-tenant:connection/mock").is_ok());
        assert!(validate_trn("trn:openact:test-tenant:task/ping@v1").is_ok());
        
        // Invalid TRNs
        assert!(validate_trn("invalid").is_err());
        assert!(validate_trn("trn:wrong:tenant:resource/id").is_err());
        assert!(validate_trn("trn:openact::resource/id").is_err()); // empty tenant
        assert!(validate_trn("trn:openact:tenant:resource").is_err()); // no slash
        assert!(validate_trn("trn:openact:tenant:/id").is_err()); // empty type
        assert!(validate_trn("trn:openact:tenant:type/").is_err()); // empty id
    }

    #[test]
    fn test_parse_connection_trn() {
        let (tenant, id) = parse_connection_trn("trn:openact:test-tenant:connection/mock@v1").unwrap();
        assert_eq!(tenant, "test-tenant");
        assert_eq!(id, "mock@v1");
        
        assert!(parse_connection_trn("trn:openact:tenant:task/id").is_err());
    }

    #[test]
    fn test_parse_task_trn() {
        let (tenant, id) = parse_task_trn("trn:openact:test-tenant:task/ping@v1").unwrap();
        assert_eq!(tenant, "test-tenant");
        assert_eq!(id, "ping@v1");
        
        assert!(parse_task_trn("trn:openact:tenant:connection/id").is_err());
    }
}
