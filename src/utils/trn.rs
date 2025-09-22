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


