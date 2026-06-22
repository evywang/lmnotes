//! Tauri 命令定义。M1a 逐步填充。

use lmnotes_core::indexer::Indexer;
use lmnotes_core::okf::concept::Concept;
use lmnotes_core::search::{SearchEngine, SearchHit};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
pub fn search(
    query: String,
    limit: Option<usize>,
    engine: State<'_, Arc<SearchEngine>>,
) -> Result<Vec<SearchHit>, String> {
    engine
        .search(&query, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

/// 默认 vault 目录（M1a 固定 ~/.lmnotes/default）。
fn vault_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".lmnotes").join("default")
}

#[derive(serde::Serialize)]
pub struct ConceptDto {
    pub text: String,
}

#[tauri::command]
pub async fn read_concept(path: String) -> Result<ConceptDto, String> {
    let full = vault_root().join(&path);
    let text = tokio::fs::read_to_string(&full)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ConceptDto { text })
}

#[tauri::command]
pub async fn save_concept(
    path: String,
    text: String,
    indexer: State<'_, Arc<Indexer>>,
) -> Result<(), String> {
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &text)
        .await
        .map_err(|e| e.to_string())?;
    // 解析并增量索引（T10 把这里改为文件事件触发）
    match Concept::parse(&text) {
        Ok(c) => {
            indexer
                .index_concept(&path, &text, &c)
                .await
                .map_err(|e| e.to_string())?;
        }
        Err(e) => {
            // frontmatter 损坏：不阻塞保存，索引跳过（Vault::validate 会报告）
            eprintln!("index skip (parse fail): {e}");
        }
    }
    Ok(())
}

/// 快速捕获：写入当日 daily note（不存在则创建）。
/// 返回 daily note 的相对路径，便于前端打开。
#[tauri::command]
pub async fn quick_capture(text: String) -> Result<String, String> {
    use chrono::Utc;
    let root = vault_root();
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let daily_path = format!("notes/daily/{date}.md");
    let full = root.join(&daily_path);

    // 若不存在，创建带 frontmatter 的 daily note
    if !full.exists() {
        let id = lmnotes_core::id::new_note_id(Utc::now().naive_utc());
        let header = format!(
            "---\ntype: daily\nid: {id}\ntitle: {date}\n---\n\n# {date}\n\n",
            date = date
        );
        if let Some(p) = full.parent() {
            tokio::fs::create_dir_all(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&full, header)
            .await
            .map_err(|e| e.to_string())?;
    }

    // 追加捕获条目（带时间戳）
    let time = Utc::now().format("%H:%M").to_string();
    let entry = format!("\n## {time}\n\n{text}\n");
    let mut existing = tokio::fs::read_to_string(&full)
        .await
        .map_err(|e| e.to_string())?;
    existing.push_str(&entry);
    tokio::fs::write(&full, existing)
        .await
        .map_err(|e| e.to_string())?;

    Ok(daily_path)
}

/// 插入图片：按 SHA-256 哈希存 assets/img/<前2位>/<hash>.<ext>（去重）。
/// 返回 bundle-relative 路径（带前导 /），供前端插入 markdown 图片链接。
#[tauri::command]
pub async fn insert_image(data: Vec<u8>, ext: String) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&data);
    let hash = hex::encode(h.finalize());
    let prefix = &hash[..2];
    let rel = format!("assets/img/{prefix}/{hash}.{ext}");
    let full = vault_root().join(&rel);
    if !full.exists() {
        if let Some(p) = full.parent() {
            tokio::fs::create_dir_all(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&full, &data)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(format!("/{rel}"))
}
