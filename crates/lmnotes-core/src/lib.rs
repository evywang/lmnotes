//! lmnotes-core: LMNotes 业务核心库（无 UI 依赖）。
//!
//! 严格遵循 Google OKF v0.1（见 docs/okf/SPEC.v0.1.md）。

pub mod backend;
pub mod error;
pub mod id;
pub mod index;
pub mod indexer;
pub mod llm;
pub mod okf;
pub mod qa;
pub mod search;
pub mod vault;

pub use error::{CoreError, Result};
