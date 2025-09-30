/// Common policy/error messages and small helpers shared across REST/MCP.
pub mod messages {
    /// Standard message when a caller refers to an action by name but omits version.
    /// Keep this text consistent across REST and MCP to align UX and docs.
    pub fn version_required_message() -> &'static str {
        "When specifying an action by name, include 'version' (integer) or set it to 'latest'; alternatively, provide 'action_trn' with explicit @vN"
    }
}

/// Tool name parsing and validation utilities
pub mod tools {
    use once_cell::sync::Lazy;
    use regex::Regex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ToolName {
        pub connector: String,
        pub action: String,
    }

    /// Parse `connector.action` with basic validation. Lowercases both parts.
    /// Allowed chars: [a-z0-9-] for connector, and [a-z0-9-._] for action (allow dots in action names if needed).
    pub fn parse_tool_name(input: &str) -> Result<ToolName, String> {
        if !input.contains('.') {
            return Err("Invalid tool name: expected 'connector.action'".to_string());
        }
        let mut parts = input.splitn(2, '.');
        let connector = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let action = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        if connector.is_empty() || action.is_empty() {
            return Err("Invalid tool name: connector or action is empty".to_string());
        }
        // Basic validation via regex
        static CONNECTOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-z0-9-]+$").unwrap());
        static ACTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-z0-9-._]+$").unwrap());
        if !CONNECTOR_RE.is_match(&connector) {
            return Err("Invalid connector name: only [a-z0-9-] allowed".to_string());
        }
        if !ACTION_RE.is_match(&action) {
            return Err("Invalid action name: only [a-z0-9-._] allowed".to_string());
        }
        Ok(ToolName { connector, action })
    }
}
