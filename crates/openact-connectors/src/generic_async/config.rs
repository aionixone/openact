use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Supported execution modes for the generic async connector.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenericAsyncMode {
    Async,
    FireForget,
}

impl Default for GenericAsyncMode {
    fn default() -> Self {
        Self::Async
    }
}

impl GenericAsyncMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Async => "async",
            Self::FireForget => "fire_forget",
        }
    }

    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Async => "running",
            Self::FireForget => "accepted",
        }
    }

    pub fn phase_str(&self) -> &'static str {
        match self {
            Self::Async => "async_waiting",
            Self::FireForget => "fire_forget",
        }
    }
}

/// Action-level configuration describing how the external task should be tracked.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct GenericAsyncActionConfig {
    pub mode: Option<GenericAsyncMode>,
    pub heartbeat_timeout_seconds: Option<u64>,
    pub status_ttl_seconds: Option<u64>,
    pub fire_forget: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker: Option<GenericAsyncTrackerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl GenericAsyncActionConfig {
    pub fn resolved_mode(&self) -> GenericAsyncMode {
        if self.fire_forget.unwrap_or(false) {
            GenericAsyncMode::FireForget
        } else {
            self.mode.unwrap_or_default()
        }
    }
}

/// Tracker configuration describing how OpenAct should observe external progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GenericAsyncTrackerConfig {
    /// Do not perform any background tracking (noop stub).
    Noop,
    /// Complete the run after an optional delay with the provided result payload.
    MockComplete {
        #[serde(default)]
        delay_ms: Option<u64>,
        #[serde(default)]
        result: Value,
    },
    /// Mark the run failed after an optional delay.
    MockFail {
        #[serde(default)]
        delay_ms: Option<u64>,
        #[serde(default)]
        error: Value,
    },
    /// Placeholder for polling strategy; currently unimplemented.
    #[serde(other)]
    Unsupported,
}

impl Default for GenericAsyncTrackerConfig {
    fn default() -> Self {
        Self::Noop
    }
}
