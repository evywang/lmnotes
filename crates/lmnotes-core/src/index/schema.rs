//! 三层索引共享的 schema 常量与数据结构。

/// SQLite concepts 表：concept 元数据。
pub const CREATE_CONCEPTS: &str = "
CREATE TABLE IF NOT EXISTS concepts (
    id          TEXT PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    type_       TEXT NOT NULL,
    title       TEXT,
    mtime       INTEGER NOT NULL,
    content_hash TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_concepts_path ON concepts(path);
";

/// SQLite edges 表：图谱邻接（增量，见 ADR-0003 F5）。
pub const CREATE_EDGES: &str = "
CREATE TABLE IF NOT EXISTS edges (
    src_id  TEXT NOT NULL,
    dst_id  TEXT,
    dst_path TEXT NOT NULL,
    link_text TEXT,
    PRIMARY KEY (src_id, dst_path)
);
CREATE INDEX IF NOT EXISTS idx_edges_src ON edges(src_id);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst_id);
";

/// sqlite-vec 向量虚拟表（M1b 接 embed 后填充）。
///
/// 维度由运行时配置决定（评审修正：原硬编码 768 与 GLM 的 1024 不匹配）。
/// 不同 Provider 的 embed 维度：Ollama nomic-embed-text=768，GLM Embedding-2=1024，
/// OpenAI text-embedding-3-large 可配任意 ≤3072。
pub fn create_vec_sql(dim: usize) -> String {
    format!(
        "
CREATE VIRTUAL TABLE IF NOT EXISTS vec_concepts USING vec0(
    id TEXT PRIMARY KEY,
    embedding float[{dim}]
);
"
    )
}

/// suggestions 表：LLM 建议队列（M1b）。
pub const CREATE_SUGGESTIONS: &str = "
CREATE TABLE IF NOT EXISTS suggestions (
    id          TEXT PRIMARY KEY,
    concept_id  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    payload     TEXT NOT NULL,
    status      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    applied_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_sugg_concept ON suggestions(concept_id);
CREATE INDEX IF NOT EXISTS idx_sugg_status ON suggestions(status);
";

/// chat_history 表：Chat with Vault 对话历史（M1c 增强）。
pub const CREATE_CHAT_HISTORY: &str = "
CREATE TABLE IF NOT EXISTS chat_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    citations   TEXT,
    created_at  INTEGER NOT NULL
);
";

#[derive(Debug, Clone)]
pub struct ConceptRow {
    pub id: String,
    pub path: String,
    pub type_: String,
    pub title: Option<String>,
    pub mtime: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct EdgeRow {
    pub src_id: String,
    pub dst_id: Option<String>,
    pub dst_path: String,
    pub link_text: Option<String>,
}
