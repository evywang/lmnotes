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
pub const CREATE_VEC: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS vec_concepts USING vec0(
    id TEXT PRIMARY KEY,
    embedding float[768]
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
