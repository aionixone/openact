use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl Usage {
    pub fn total(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// A minimal streaming assembler that collects text deltas and usage, and reports stats on finish.
#[derive(Debug)]
pub struct StreamAssembler {
    started_at: Instant,
    buffer: String,
    usage: Usage,
}

impl StreamAssembler {
    pub fn new() -> Self {
        Self { started_at: Instant::now(), buffer: String::new(), usage: Usage::default() }
    }

    pub fn push_text(&mut self, delta: &str) {
        self.buffer.push_str(delta);
    }

    pub fn add_usage(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        self.usage.prompt_tokens = self.usage.prompt_tokens.saturating_add(prompt_tokens);
        self.usage.completion_tokens =
            self.usage.completion_tokens.saturating_add(completion_tokens);
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn finish(self) -> (String, Usage, Duration) {
        (self.buffer, self.usage, self.started_at.elapsed())
    }
}
