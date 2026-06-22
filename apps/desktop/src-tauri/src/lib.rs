//! LMNotes 桌面应用（Tauri 2）IPC 壳。命令在 commands.rs，M1a 逐步填充。

mod commands;

use lmnotes_core::backend::IndexBackend;
use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::indexer::Indexer;
use lmnotes_core::search::SearchEngine;
use std::path::PathBuf;
use std::sync::Arc;

/// 默认 vault 目录（M1a 固定 ~/.lmnotes/default；UI 选择器 M1b）。
fn vault_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".lmnotes").join("default")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let dir = vault_dir();
    let lmnotes_dir = dir.join(".lmnotes");
    let meta = Arc::new(SqliteIndex::open(lmnotes_dir.join("index.sqlite")).expect("open sqlite"));
    let fulltext = Arc::new(TantivyIndex::open(lmnotes_dir.join("tantivy")).expect("open tantivy"));
    let indexer = Arc::new(Indexer::new(meta.clone(), fulltext.clone()));
    let engine = Arc::new(SearchEngine::new(meta.clone(), fulltext.clone()));

    // 异步初始化 schema
    let meta_init = meta.clone();
    tauri::async_runtime::spawn(async move {
        let _ = meta_init.init_schema().await;
    });

    // 确保默认 vault 目录存在（首次启动）
    let _ = std::fs::create_dir_all(vault_dir());
    let _ = std::fs::create_dir_all(vault_dir().join(".lmnotes"));

    tauri::Builder::default()
        .manage(meta)
        .manage(fulltext)
        .manage(indexer)
        .manage(engine)
        .invoke_handler(tauri::generate_handler![commands::ping, commands::search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
