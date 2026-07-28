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

    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// 云端转录端点返回非 2xx（ADR-0007）。
    /// 与 `Http` 区分：reqwest 对 HTTP 状态码不报错（返回 Ok(response)），
    /// 应用层（whisper.rs）检测到非 2xx 后用此变体携带状态码，
    /// 供 `classify_transcribe_error` 判断 5xx 降级 / 4xx 不降级。
    #[error("transcribe HTTP {status}: {body}")]
    TranscribeHttp { status: u16, body: String },

    #[error("UTF-8 decode error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
