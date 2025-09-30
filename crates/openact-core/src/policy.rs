/// Common policy/error messages and small helpers shared across REST/MCP.
pub mod messages {
    /// Standard message when a caller refers to an action by name but omits version.
    /// Keep this text consistent across REST and MCP to align UX and docs.
    pub fn version_required_message() -> &'static str {
        "When specifying an action by name, include 'version' (integer) or set it to 'latest'; alternatively, provide 'action_trn' with explicit @vN"
    }
}

