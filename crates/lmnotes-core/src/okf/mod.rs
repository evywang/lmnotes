//! OKF (Open Knowledge Format) v0.1 实现，严格遵循 Google 官方规范。
//! 见 docs/okf/SPEC.v0.1.md。

pub mod concept;
pub mod frontmatter;
pub mod validator;

pub use frontmatter::Frontmatter;
