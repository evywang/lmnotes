//! LLM Provider 抽象 + 路由 + 护栏（ADR-0005）。

pub mod provider;
pub mod ollama; // T2
pub mod openai; // T3
pub mod routing; // T4
pub mod guard; // T5
pub mod suggestion; // T6

pub use provider::*;
