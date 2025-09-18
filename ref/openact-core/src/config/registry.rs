use std::fs;
use std::path::Path;
use anyhow::Result;

use crate::config::connection::ConnectionConfig;

pub fn load_connections_from_dir(dir: &str) -> Result<Vec<ConnectionConfig>> {
    let mut results = Vec::new();
    let path = Path::new(dir);
    if !path.exists() {
        return Ok(results);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext == "json" || ext == "yaml" || ext == "yml" {
                let content = fs::read_to_string(&p)?;
                let conn: ConnectionConfig = if ext == "json" {
                    serde_json::from_str(&content)?
                } else {
                    // 允许 yaml 先转 json，或直接解析 yaml（此处走 serde_yaml）
                    serde_yaml::from_str::<ConnectionConfig>(&content)?
                };
                results.push(conn);
            }
        }
    }
    Ok(results)
}


