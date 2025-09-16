//! TRN (Tool Resource Name) 管理系统
//! 
//! 提供统一的资源标识和管理功能

pub mod manager;
pub mod parser;

pub use manager::TrnManager;
pub use parser::{OpenActTrn, TrnParser};
