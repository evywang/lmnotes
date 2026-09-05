//! LMNotes 桌面应用（Tauri 2）IPC 壳。

mod commands;
mod llm_config;
mod media_tasks;
pub mod whisper_cpp;

use lmnotes_core::backend::fs::FsBackend;
use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::indexer::{walk_and_index, Indexer};
use lmnotes_core::okf::concept::Concept;
use lmnotes_core::search::SearchEngine;
use notify::{RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;

/// 当前 vault 目录（v0.4 多库：与 commands::vault_root 同源，见 llm_config::current_vault）。
fn vault_dir() -> PathBuf {
    llm_config::current_vault()
}

/// 保活的 watcher（持有以避免被 drop）。
#[allow(dead_code)]
struct HoldWatcher(Option<notify::RecommendedWatcher>);

/// 保活标记：MCP server 在独立 spawn 中运行，此结构仅用于语义上标记其已启用。
#[allow(dead_code)]
struct HoldMcp;

/// 切换快速捕获浮窗（FR-CAP-01，v0.7）：存在则 show/hide 切换，
/// 不存在则创建（同一 dist 的 `#quick-capture` 路由，main.tsx 分流渲染精简 UI）。
fn toggle_quick_capture(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("quick-capture") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
        }
    } else {
        let built = tauri::WebviewWindowBuilder::new(
            app,
            "quick-capture",
            tauri::WebviewUrl::App("index.html#quick-capture".into()),
        )
        .title("LMNotes")
        .inner_size(460.0, 200.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .build();
        if let Err(e) = built {
            eprintln!("[hotkey] create quick-capture window failed: {e}");
        }
    }
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
    // LLM 用量记录（v0.8 FR-MODEL-05）：sink 只做 channel send（非阻塞），
    // 消费任务落 llm_usage 表；provider 注册时统一包裹（见 build_with_sink）。
    let (usage_tx, mut usage_rx) =
        tokio::sync::mpsc::unbounded_channel::<lmnotes_core::llm::usage::UsageEvent>();
    let usage_sink: lmnotes_core::llm::usage::UsageSink = Arc::new(move |e| {
        let _ = usage_tx.send(e);
    });
    let (registry, routing, guard_cfg) =
        llm_config::Config::load_or_default().build_with_sink(usage_sink);
    let registry = Arc::new(registry);
    let routing = Arc::new(routing);
    let guard_cfg = Arc::new(guard_cfg);
    let usage_db = meta.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(e) = usage_rx.recv().await {
            if let Err(err) = usage_db.record_usage(&e.provider, e.kind, e.local, e.tokens_est) {
                eprintln!("[usage] record failed: {err}");
            }
        }
    });

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
    let (tx, rx) = channel::<(PathBuf, bool)>(); // (path, is_remove)
    let watcher_result = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(e) = res {
            match e.kind {
                notify::EventKind::Create(_) | notify::EventKind::Modify(_) => {
                    for p in &e.paths {
                        if p.extension().map(|x| x == "md").unwrap_or(false) {
                            let _ = tx.send((p.clone(), false));
                        }
                    }
                }
                notify::EventKind::Remove(_) => {
                    for p in &e.paths {
                        if p.extension().map(|x| x == "md").unwrap_or(false) {
                            let _ = tx.send((p.clone(), true));
                        }
                    }
                }
                _ => {}
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
        let sqlite_watch = meta.clone();
        let reg_watch = registry.clone();
        let routing_watch = routing.clone();
        let guard_watch = guard_cfg.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok((p, is_remove)) = rx.recv() {
                if let Ok(rel) = p.strip_prefix(&dir_consumer) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    // 派生数据（索引/快照）的变更不参与重建索引（v0.3：消除快照写入噪音）
                    if rel.starts_with(".lmnotes/") {
                        continue;
                    }
                    if is_remove {
                        // 删除事件：尝试用路径作为 id 清除索引
                        if let Err(e) = indexer_consumer.unindex(&rel).await {
                            eprintln!("watch unindex fail {rel}: {e}");
                        }
                        continue;
                    }
                    // 变更事件：读 + 索引 + 生成建议
                    match tokio::fs::read_to_string(&p).await {
                        Ok(text) => match Concept::parse(&text) {
                            Ok(c) => {
                                if let Err(e) =
                                    indexer_consumer.index_concept(&rel, &text, &c).await
                                {
                                    eprintln!("watch index fail {rel}: {e}");
                                }
                                let text_c = text.clone();
                                let rel_c = rel.clone();
                                let sqlite_c = sqlite_watch.clone();
                                let reg_c = reg_watch.clone();
                                let routing_c = routing_watch.clone();
                                let guard_c = guard_watch.clone();
                                tauri::async_runtime::spawn(async move {
                                    if let Err(e) = lmnotes_core::indexer::generate_suggestions(
                                        &c, &rel_c, &sqlite_c, &reg_c, &routing_c, &guard_c,
                                        &text_c,
                                    )
                                    .await
                                    {
                                        eprintln!("watch suggestion fail {rel_c}: {e}");
                                    }
                                });
                            }
                            Err(e) => eprintln!("watch parse skip {rel}: {e}"),
                        },
                        Err(e) => eprintln!("watch read skip {rel}: {e}"),
                    }
                }
            }
        });
    }

    // MCP server：把 vault 只读暴露给 AI agent（streamable HTTP，仅 127.0.0.1）。
    // 复用桌面已构造的同一组 Arc 资源（零拷贝共享，无跨进程锁）。
    let mcp_cfg = llm_config::Config::load_or_default().mcp;
    let mcp_hold: Option<HoldMcp> = if mcp_cfg.enabled {
        // token：配置缺省则随机生成 32 字节 hex（仅本机 loopback，非空即可）
        let token = mcp_cfg.token.clone().unwrap_or_else(|| {
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::rng().fill_bytes(&mut bytes);
            hex::encode(bytes)
        });
        let mcp_server = lmnotes_mcp::LmnotesMcpServer::new(
            dir.clone(),
            engine.clone(),
            meta.clone() as Arc<dyn lmnotes_core::backend::IndexBackend>,
            meta.clone(),
            fulltext.clone(),
            registry.clone(),
            routing.clone(),
            guard_cfg.clone(),
        );
        let lmnotes_home = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".lmnotes");
        let port = mcp_cfg.port;
        let vault_for_disc = dir.clone();
        tauri::async_runtime::spawn(async move {
            // 端口冲突兜底：先尝试配置端口，bind 失败则退到 :0（OS 分配）。
            // serve() 内部 bind 成功后即写 mcp.json 发现文件并阻塞服务。
            let candidates: [std::net::SocketAddr; 2] = [
                format!("127.0.0.1:{port}")
                    .parse()
                    .unwrap_or(([127, 0, 0, 1], 0).into()),
                ([127, 0, 0, 1], 0).into(),
            ];
            for addr in candidates {
                match lmnotes_mcp::server::serve(
                    mcp_server.clone(),
                    addr,
                    token.clone(),
                    lmnotes_home.clone(),
                    vault_for_disc.clone(),
                )
                .await
                {
                    Ok(_) => break,
                    Err(e) => eprintln!("[mcp] bind {addr} failed: {e}; trying fallback :0"),
                }
            }
        });
        Some(HoldMcp)
    } else {
        eprintln!("[mcp] disabled by config (mcp.enabled = false)");
        None
    };

    // 媒体任务 worker 依赖（在 manage() 移动 Arc 之前克隆）
    let cancel_registry = media_tasks::CancelRegistry::default();
    let worker_deps = media_tasks::WorkerDeps {
        indexer: indexer.clone(),
        sqlite: meta.clone(),
        registry: registry.clone(),
        routing: routing.clone(),
        guard_cfg: guard_cfg.clone(),
        cancels: cancel_registry.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        // 全局快捷键（FR-CAP-01，v0.7）：CmdOrCtrl+Shift+L 切换快速捕获浮窗。
        // 注册在 setup 内（Rust 侧 GlobalShortcutExt，不经过 JS 权限）。
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        toggle_quick_capture(app);
                    }
                })
                .build(),
        )
        .manage(indexer)
        .manage(engine)
        .manage(meta.clone() as Arc<dyn lmnotes_core::backend::IndexBackend>)
        .manage(meta)
        .manage(fulltext)
        .manage(registry.clone())
        .manage(routing.clone())
        .manage(guard_cfg.clone())
        .manage(HoldWatcher(watcher))
        .manage(mcp_hold)
        .manage(cancel_registry)
        .setup(move |app| {
            let worker_deps = worker_deps;
            // 媒体任务后台 worker（v0.5 FR-MEDIA-04）：running→pending 兜底 + 常驻循环
            media_tasks::spawn_worker(app.handle().clone(), worker_deps);
            // 全局快捷键注册（v0.7 FR-CAP-01；v0.8 起可配置 config.capture.hotkey，
            // 保存后重启生效）。注册失败降级：打日志不阻塞，应用内捕获不受影响。
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let hotkey = llm_config::Config::load_or_default().capture.hotkey;
            if let Err(e) = app.global_shortcut().register(hotkey.as_str()) {
                eprintln!(
                    "[hotkey] register {hotkey} failed: {e} (被占用？可在设置中修改热键；应用内 Ctrl+N 捕获不受影响)"
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::search,
            commands::list_note_titles,
            commands::list_snapshots,
            commands::read_snapshot,
            commands::extract_action_items,
            commands::read_concept,
            commands::save_concept,
            commands::quick_capture,
            commands::insert_image,
            commands::insert_audio,
            commands::create_voice_note,
            commands::create_media_note,
            commands::describe_image,
            commands::list_templates,
            commands::create_note_from_template,
            commands::export_vault_zip,
            commands::init_git_repo,
            commands::cancel_media_task,
            commands::retry_media_task,
            commands::enqueue_media_task,
            commands::list_media_tasks,
            commands::list_suggestions,
            commands::accept_suggestion,
            commands::reject_suggestion,
            commands::rewrite_selection,
            commands::save_snapshot,
            commands::get_config,
            commands::set_config,
            commands::probe_providers,
            commands::chat_stream,
            commands::load_chat_history,
            commands::clear_chat_history,
            commands::create_note,
            commands::open_or_create_daily,
            commands::list_timeline,
            commands::list_tags,
            commands::list_notes_with_tag,
            commands::import_vault,
            commands::get_usage_summary,
            commands::generate_review,
            commands::import_note,
            commands::import_document,
            commands::list_tree,
            commands::delete_note,
            commands::create_folder,
            commands::reveal_in_explorer,
            commands::move_item,
            commands::graph_full,
            commands::graph_neighborhood,
            commands::list_vaults,
            commands::add_vault,
            commands::remove_vault,
            commands::switch_vault,
            commands::list_whisper_models,
            commands::download_whisper_model,
            commands::get_local_stt_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
