use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub paused_state: String,
    pub context: Value,
    pub await_meta: Value,
}

pub trait RunStore: Send + Sync {
    fn put(&self, c: Checkpoint);
    fn get(&self, run_id: &str) -> Option<Checkpoint>;
    fn del(&self, run_id: &str);
}

#[derive(Default, Clone)]
pub struct MemoryRunStore {
    inner: Arc<RwLock<HashMap<String, Checkpoint>>>,
}

impl RunStore for MemoryRunStore {
    fn put(&self, c: Checkpoint) {
        self.inner.write().unwrap().insert(c.run_id.clone(), c);
    }
    fn get(&self, run_id: &str) -> Option<Checkpoint> {
        self.inner.read().unwrap().get(run_id).cloned()
    }
    fn del(&self, run_id: &str) {
        self.inner.write().unwrap().remove(run_id);
    }
}
