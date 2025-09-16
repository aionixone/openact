//! 任务执行引擎
//! 
//! 负责执行 HTTP Task，包括认证、参数合并、重试等

pub mod executor;
pub mod result;

pub use executor::TaskExecutor;
pub use result::{ExecutionResult, ExecutionStatus};
