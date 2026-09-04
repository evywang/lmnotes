//! 三层索引共享的 schema 常量与数据结构。

/// SQLite concepts 表：concept 元数据。
pub const CREATE_CONCEPTS: &str = "
CREATE TABLE IF NOT EXISTS concepts (
    id          TEXT PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    type_       TEXT NOT NULL,
    title       TEXT,
    mtime       INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    aliases     TEXT NOT NULL DEFAULT '[]',
    tags        TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX IF NOT EXISTS idx_concepts_path ON concepts(path);
";

/// 老库迁移（v0.3 FR-CAP-03）：aliases 供双链补全按别名命中。
/// 对已有列的库执行会报 duplicate column——调用方容忍并跳过。
pub const ALTER_CONCEPTS_ALIASES: &str =
    "ALTER TABLE concepts ADD COLUMN aliases TEXT NOT NULL DEFAULT '[]'";

/// 老库迁移（v0.7 FR-SEARCH-05）：tags 供标签云/过滤聚合。
/// 同 aliases：JSON 数组存列，duplicate column 容错跳过。
pub const ALTER_CONCEPTS_TAGS: &str =
    "ALTER TABLE concepts ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'";

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

/// media_tasks 表：媒体处理后台任务（v0.5 FR-MEDIA-04，收编 FR-CAP-09）。
pub const CREATE_MEDIA_TASKS: &str = "
CREATE TABLE IF NOT EXISTS media_tasks (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    asset_rel   TEXT NOT NULL,
    mime        TEXT NOT NULL,
    duration_ms INTEGER,
    language    TEXT,
    status      TEXT NOT NULL,
    error       TEXT,
    result_path TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_tasks_status ON media_tasks(status);
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
    /// 别名（frontmatter aliases，JSON 序列化存列；v0.3 补全用）。
    pub aliases: Vec<String>,
    /// 标签（frontmatter tags，JSON 序列化存列；v0.7 标签云/过滤用）。
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EdgeRow {
    pub src_id: String,
    pub dst_id: Option<String>,
    pub dst_path: String,
    pub link_text: Option<String>,
}

// ── 双链补全候选（FR-CAP-03，v0.3）────────────────────────────────────────

/// 补全候选命中项。`matched_alias` 非空表示按别名命中（前端 label 用别名）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteTitleHit {
    pub title: String,
    pub path: String,
    pub id: String,
    pub matched_alias: Option<String>,
}

/// title 回退：无 title 时用文件名（去 .md）。与 graph::title_of 语义一致。
pub fn title_of(row: &ConceptRow) -> String {
    row.title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| {
            row.path
                .rsplit('/')
                .next()
                .unwrap_or(&row.path)
                .trim_end_matches(".md")
                .to_string()
        })
}

/// 补全候选过滤：title / alias / path(含文件名) 大小写不敏感子串匹配。
///
/// `query` 为空 → 返回前 `limit` 条（rows 已按 path 排序，保持顺序）。
/// 匹配优先级：title > alias > path/文件名（同一行只产出一条，取最高优先级来源）。
pub fn filter_titles(rows: &[ConceptRow], query: &str, limit: usize) -> Vec<NoteTitleHit> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    for row in rows {
        if out.len() >= limit {
            break;
        }
        let title = title_of(row);
        let matched_alias = row
            .aliases
            .iter()
            .find(|a| q.is_empty() || a.to_lowercase().contains(&q));
        let hit = q.is_empty()
            || title.to_lowercase().contains(&q)
            || matched_alias.is_some()
            || row.path.to_lowercase().contains(&q);
        if !hit {
            continue;
        }
        // label：title 命中（或空 query）用 title；否则别名命中用别名；path 命中回退 title
        let (label, matched_alias) = if !q.is_empty() && !title.to_lowercase().contains(&q) {
            match matched_alias {
                Some(a) => (a.clone(), Some(a.clone())),
                None => (title, None),
            }
        } else {
            (title, None)
        };
        out.push(NoteTitleHit {
            title: label,
            path: row.path.clone(),
            id: row.id.clone(),
            matched_alias,
        });
    }
    out
}

/// 媒体后台任务（FR-MEDIA-04）。生命周期 pending → running → done/failed；
/// cancelled 由用户从 pending 取消。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MediaTask {
    pub id: String,
    /// transcribe | describe
    pub kind: String,
    /// 输入资产（/assets/{audio|video|img}/…）。
    pub asset_rel: String,
    pub mime: String,
    pub duration_ms: Option<i64>,
    pub language: Option<String>,
    pub status: String,
    pub error: Option<String>,
    /// 产出笔记路径（transcripts/… 或 descriptions/…）。
    pub result_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
