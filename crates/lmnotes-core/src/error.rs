use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("YAML frontmatter parse error: {0}")]
    Yaml(String),

    #[error("OKF conformance error: {0}")]
    Conformance(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("UTF-8 decode error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
