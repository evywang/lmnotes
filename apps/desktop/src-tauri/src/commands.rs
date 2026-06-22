//! Tauri 命令定义。M1a 逐步填充。

use lmnotes_core::search::{SearchEngine, SearchHit};
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
