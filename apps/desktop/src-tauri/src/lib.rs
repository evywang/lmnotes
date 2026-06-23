//! LMNotes 桌面应用（Tauri 2）IPC 壳。

mod commands;
mod llm_config;

use lmnotes_core::backend::fs::FsBackend;
use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::indexer::{walk_and_index, Indexer};
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::routing::{Registry, Routing};
use lmnotes_core::okf::concept::Concept;
use lmnotes_core::search::SearchEngine;
use notify::{RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;

/// 默认 vault 目录（M1a 固定 ~/.lmnotes/default；UI 选择器 M1b+）。
fn vault_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".lmnotes").join("default")
}

/// 保活的 watcher（持有以避免被 drop）。
#[allow(dead_code)]
struct HoldWatcher(Option<notify::RecommendedWatcher>);

/// 构建默认 Registry + Routing（M1b-T10：从 config.json 加载）。
fn build_registry_from_config() -> (Registry, Routing, GuardConfig) {
    let cfg = llm_config::Config::load_or_default();
    cfg.build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let dir = vault_dir();
    let lmnotes_dir = dir.join(".lmnotes");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::create_dir_all(&lmnotes_dir);

    let meta = Arc::new(SqliteIndex::open(lmnotes_dir.join("index.sqlite")).expect("open sqlite"));
    let fulltext = Arc::new(TantivyIndex::open(lmnotes_dir.join("tantivy")).expect("open tantivy"));
    let indexer = Arc::new(Indexer::new(meta.clone(), fulltext.clone()));
    let engine = Arc::new(SearchEngine::new(meta.clone(), fulltext.clone()));
    let (registry, routing, guard_cfg) = build_registry_from_config();

    // 首启探测：检测 Provider 健康，不可用时日志提示（O6c）
    tauri::async_runtime::spawn(async {
        let cfg = llm_config::Config::load_or_default();
        let healths = commands::probe_providers(cfg).await.unwrap_or_default();
        for h in &healths {
            eprintln!(
                "provider {} health: {}",
                h.provider_id,
                if h.healthy { "OK" } else { "UNREACHABLE" }
            );
        }
        if healths.iter().all(|h| !h.healthy) {
            eprintln!("⚠ No healthy LLM provider. LLM features (suggestions/rewrite) will be disabled. Configure ~/.lmnotes/config.json or start Ollama.");
        }
    });

    // 启动时全量重建（增量，walk_and_index 跳过未变）
    let indexer_boot = indexer.clone();
    let dir_boot = dir.clone();
    let meta_boot = meta.clone();
    let embed_dim = llm_config::Config::load_or_default().embed_dim();
    tauri::async_runtime::spawn(async move {
        // 用 config 的 embed_dim 初始化 schema（维度变化时自动重建 vec 表）
        let _ = meta_boot.init_schema_with_vec_dim(embed_dim).await;
        let backend = FsBackend::new(&dir_boot);
        let (checked, indexed) = walk_and_index(&indexer_boot, &backend, &dir_boot).await;
        eprintln!("startup index: {checked} checked, {indexed} (re)indexed");
    });

    // 文件监听：外部编辑 .md 触发重索引
    let indexer_watch = indexer.clone();
    let dir_watch = dir.clone();
    let (tx, rx) = channel::<PathBuf>();
    let watcher_result = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(e) = res {
            if matches!(
                e.kind,
                notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            ) {
                for p in &e.paths {
                    if p.extension().map(|x| x == "md").unwrap_or(false) {
                        let _ = tx.send(p.clone());
                    }
                }
            }
        }
    });
    let watcher = match watcher_result {
        Ok(mut w) => {
            let _ = w.watch(&dir_watch, RecursiveMode::Recursive);
            Some(w)
        }
        Err(e) => {
            eprintln!("watcher init failed: {e}");
            None
        }
    };
    if watcher.is_some() {
        let indexer_consumer = indexer_watch.clone();
        let dir_consumer = dir_watch.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok(p) = rx.recv() {
                if let Ok(rel) = p.strip_prefix(&dir_consumer) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    match tokio::fs::read_to_string(&p).await {
                        Ok(text) => match Concept::parse(&text) {
                            Ok(c) => {
                                if let Err(e) =
                                    indexer_consumer.index_concept(&rel, &text, &c).await
                                {
                                    eprintln!("watch index fail {rel}: {e}");
                                }
                            }
                            Err(e) => eprintln!("watch parse skip {rel}: {e}"),
                        },
                        Err(e) => eprintln!("watch read skip {rel}: {e}"),
                    }
                }
            }
        });
    }

    tauri::Builder::default()
        .manage(indexer)
        .manage(engine)
        .manage(meta)
        .manage(Arc::new(registry))
        .manage(Arc::new(routing))
        .manage(guard_cfg)
        .manage(HoldWatcher(watcher))
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::search,
            commands::read_concept,
            commands::save_concept,
            commands::quick_capture,
            commands::insert_image,
            commands::list_suggestions,
            commands::accept_suggestion,
            commands::reject_suggestion,
            commands::rewrite_selection,
            commands::save_snapshot,
            commands::get_config,
            commands::set_config,
            commands::probe_providers
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
