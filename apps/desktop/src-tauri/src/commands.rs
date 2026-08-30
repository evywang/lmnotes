//! Tauri 命令定义。M1a/M1b 逐步填充。
//!
//! 文件 IO 用 std::fs/tokio::fs（Tauri 壳层，非核心库业务模块）。
//! 豁免 clippy.toml 的 std::fs 约束。

#![allow(clippy::disallowed_methods)]

use lmnotes_core::backend::IndexBackend;
use lmnotes_core::graph::{self, EdgeKind, GraphData};
use lmnotes_core::index::schema::{filter_titles, NoteTitleHit};
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::Indexer;
use lmnotes_core::llm::guard::{check, GuardConfig, GuardDecision};
use lmnotes_core::llm::routing::{Registry, Routing, Task};
use lmnotes_core::llm::suggestion::{SuggestionRecord, SuggestionStatus};
use lmnotes_core::llm::{ChatMessage, ChatRequest, ChatRole};
use lmnotes_core::okf::concept::Concept;
use lmnotes_core::search::{SearchEngine, SearchHit};
use std::path::{Path, PathBuf};

/// Windows 反斜杠（避免源码里写字面量转义）。
const BS: char = '\u{005C}';
use std::sync::Arc;
use tauri::{Emitter, State};

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

/// 双链补全候选（FR-CAP-03）：title/alias/path 子串匹配，title 命中优先。
/// 数据源 all_concepts（内存过滤，ms 级），query 空返回前 limit 条。
#[tauri::command]
pub fn list_note_titles(
    query: Option<String>,
    limit: Option<usize>,
    meta: State<'_, Arc<dyn IndexBackend + '_>>,
) -> Result<Vec<NoteTitleHit>, String> {
    let rows = meta.all_concepts().map_err(|e| e.to_string())?;
    Ok(filter_titles(
        &rows,
        query.as_deref().unwrap_or(""),
        limit.unwrap_or(20),
    ))
}

/// 当前 vault 目录（v0.4 多库：config.last_vault → 回退 ~/.lmnotes/default，进程内缓存）。
pub(crate) fn vault_root() -> PathBuf {
    crate::llm_config::current_vault()
}

/// LMNotes 配置主目录 ~/.lmnotes（与 config.json / mcp.json 同级）。
/// whisper.cpp 模型存放于此下的 models/（ADR-0007）。
fn lmnotes_home() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".lmnotes")
}

/// whisper.cpp 模型存放目录 ~/.lmnotes/models/。
pub fn models_dir() -> PathBuf {
    lmnotes_home().join("models")
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
    sqlite: State<'_, Arc<SqliteIndex>>,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
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
    // 解析并增量索引
    match Concept::parse(&text) {
        Ok(c) => {
            indexer
                .index_concept(&path, &text, &c)
                .await
                .map_err(|e| e.to_string())?;
            // 索引完成后 spawn LLM 建议生成（不阻塞保存返回）
            let sqlite_c = sqlite.inner().clone();
            let reg_c = registry.inner().clone();
            let routing_c = routing.inner().clone();
            let guard_c = guard_cfg.inner().clone();
            let path_c = path.clone();
            let text_c = text.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = lmnotes_core::indexer::generate_suggestions(
                    &c, &path_c, &sqlite_c, &reg_c, &routing_c, &guard_c, &text_c,
                )
                .await
                {
                    eprintln!("generate_suggestions fail {path_c}: {e}");
                }
            });
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
    let (rel, _) = archive_binary(&data, &ext, "img").await?;
    Ok(rel)
}

/// 二进制归档：按 SHA-256 哈希存 assets/<kind>/<前2位>/<hash>.<ext>（去重）。
/// kind ∈ {img, audio}。返回 (bundle-relative 路径带前导 /, hex 哈希)。
/// insert_image（图片）与 insert_audio / create_voice_note（音频）共用。
async fn archive_binary(data: &[u8], ext: &str, kind: &str) -> Result<(String, String), String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    let hash = hex::encode(h.finalize());
    let prefix = &hash[..2];
    let rel = format!("assets/{kind}/{prefix}/{hash}.{ext}");
    let full = vault_root().join(&rel);
    if !full.exists() {
        if let Some(p) = full.parent() {
            tokio::fs::create_dir_all(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&full, data)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok((format!("/{rel}"), hash))
}

/// 插入音频：存 assets/audio/<前2位>/<hash>.<ext>（去重），返回带前导 / 的相对路径。
/// 原子能力，FR-CAP-04 拖拽音频与 create_voice_note 均复用。
#[tauri::command]
pub async fn insert_audio(data: Vec<u8>, ext: String) -> Result<String, String> {
    let (rel, _) = archive_binary(&data, &ext, "audio").await?;
    Ok(rel)
}

// ============ 建议中心命令（T8）============

#[tauri::command]
pub fn list_suggestions(
    sqlite: State<'_, Arc<SqliteIndex>>,
) -> Result<Vec<SuggestionRecord>, String> {
    sqlite.list_pending_suggestions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn accept_suggestion(id: String, sqlite: State<'_, Arc<SqliteIndex>>) -> Result<(), String> {
    sqlite
        .set_suggestion_status(&id, SuggestionStatus::Accepted)
        .map_err(|e| e.to_string())
    // M1b 简化：仅标记状态。接受后写回 concept frontmatter/description 留 M1c。
}

#[tauri::command]
pub fn reject_suggestion(id: String, sqlite: State<'_, Arc<SqliteIndex>>) -> Result<(), String> {
    sqlite
        .set_suggestion_status(&id, SuggestionStatus::Rejected)
        .map_err(|e| e.to_string())
}

// ============ 就地改写 + 撤销（T9）============

/// 就地改写：对选中文本执行 action，返回新文本。改写前由前端先调 save_snapshot。
#[tauri::command]
pub async fn rewrite_selection(
    action: String, // polish | expand | translate | summarize
    selection: String,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<String, String> {
    let (chat, model) = registry
        .chat_for(&routing, Task::Rewrite)
        .map_err(|e| e.to_string())?;
    // 改写由用户主动触发，不读 concept 的 local_only 标记
    match check(&guard_cfg, chat.kind(), &selection, false) {
        GuardDecision::Allow => {}
        GuardDecision::Deny(reason) => return Err(reason),
    }
    let prompt = match action.as_str() {
        "polish" => "润色以下文本，保持原意，使其更流畅专业。只输出润色后的文本。",
        "expand" => "扩写以下文本，补充细节与例证，保持原意。只输出扩写后的文本。",
        "translate" => "将以下文本翻译为英文。只输出译文。",
        "summarize" => "用要点列表总结以下文本。只输出要点。",
        _ => return Err(format!("unknown action: {action}")),
    };
    let req = ChatRequest {
        model,
        messages: vec![
            ChatMessage {
                role: ChatRole::System,
                content: prompt.into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: selection,
            },
        ],
        temperature: Some(0.5),
    };
    chat.chat(req).await.map_err(|e| e.to_string())
}

/// 行动项抽取（FR-LLM-06）：从 transcript/meeting 笔记抽 markdown checklist。
///
/// 与 rewrite_selection 的区别：按 path 读整篇 concept，护栏读 frontmatter 的
/// `llm_local_only`（文本可扫描，三层护栏全生效——比语音路径严格，行为正确）。
/// 路由复用 Task::Summarize（同为"内容压缩生成"类任务，避免新增 Task 变体 +
/// 全量 config 迁移；量大后再立专用 Task）。
#[tauri::command]
pub async fn extract_action_items(
    path: String,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<String, String> {
    let full = vault_root().join(&path);
    let text = tokio::fs::read_to_string(&full)
        .await
        .map_err(|e| format!("read note failed: {e}"))?;
    let concept = Concept::parse(&text).map_err(|e| format!("parse concept failed: {e}"))?;
    let local_only = concept
        .frontmatter
        .extra
        .get("llm_local_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let body = concept.body.clone();

    let (chat, model) = registry
        .chat_for(&routing, Task::Summarize)
        .map_err(|e| e.to_string())?;
    match check(&guard_cfg, chat.kind(), &body, local_only) {
        GuardDecision::Allow => {}
        GuardDecision::Deny(reason) => return Err(reason),
    }
    let req = ChatRequest {
        model,
        messages: vec![
            ChatMessage {
                role: ChatRole::System,
                content: "从以下会议记录或语音转录中抽取行动项。输出 markdown 任务清单（- [ ] 格式，每条一行；若文中提及负责人或期限，用括号附在条目后）。只输出清单本身，不要任何前后解释。若确实没有行动项，输出：（无行动项）".into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: body,
            },
        ],
        temperature: Some(0.2), // 抽取任务求稳
    };
    chat.chat(req).await.map_err(|e| e.to_string())
}

/// 保存快照（撤销用）。存到 .lmnotes/llm/snapshots/<concept_path>-<ts>.md
#[tauri::command]
pub async fn save_snapshot(concept_path: String, text: String) -> Result<String, String> {
    let ts = chrono::Utc::now().timestamp();
    let safe = concept_path.replace(['/', '\\'], "_");
    let rel = format!(".lmnotes/llm/snapshots/{safe}-{ts}.md");
    let full = vault_root().join(&rel);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &text)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rel)
}

/// 快照元信息（历史版本面板列表项）。
#[derive(serde::Serialize)]
pub struct SnapshotInfo {
    /// 文件名尾段解析出的 unix 秒时间戳。
    pub ts: i64,
    /// vault 相对路径（read_snapshot 的入参）。
    pub rel_path: String,
    pub size_bytes: u64,
}

/// 从快照文件名解析时间戳：`{safe}-{ts}.md` 尾段数字；解析失败返回 None。
fn parse_snapshot_ts(filename: &str) -> Option<i64> {
    filename
        .strip_suffix(".md")?
        .rsplit('-')
        .next()?
        .parse::<i64>()
        .ok()
}

/// 列出某 concept 的历史快照（按时间降序）。
#[tauri::command]
pub fn list_snapshots(concept_path: String) -> Result<Vec<SnapshotInfo>, String> {
    let safe = concept_path.replace(['/', '\\'], "_");
    let dir = vault_root().join(".lmnotes/llm/snapshots");
    let mut out: Vec<SnapshotInfo> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(out), // 目录不存在 = 无快照
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&safe) || !name.ends_with(".md") {
            continue;
        }
        let Some(ts) = parse_snapshot_ts(&name) else {
            continue;
        };
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(SnapshotInfo {
            ts,
            rel_path: format!(".lmnotes/llm/snapshots/{name}"),
            size_bytes: size,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.ts));
    Ok(out)
}

/// 读取快照内容。rel_path 必须位于 .lmnotes/llm/snapshots/ 内（防目录穿越）。
#[tauri::command]
pub async fn read_snapshot(rel_path: String) -> Result<String, String> {
    const PREFIX: &str = ".lmnotes/llm/snapshots/";
    if !rel_path.starts_with(PREFIX) || rel_path.contains("..") {
        return Err(format!("invalid snapshot path: {rel_path}"));
    }
    tokio::fs::read_to_string(vault_root().join(&rel_path))
        .await
        .map_err(|e| format!("read snapshot failed: {e}"))
}

// ============ Provider 配置（T10）============

#[tauri::command]
pub fn get_config() -> Result<crate::llm_config::Config, String> {
    Ok(crate::llm_config::Config::load_or_default())
}

#[tauri::command]
pub fn set_config(config: crate::llm_config::Config) -> Result<(), String> {
    config.save()
}

/// 探测 Provider 健康状态（首启检测用）。
#[tauri::command]
pub async fn probe_providers(
    config: crate::llm_config::Config,
) -> Result<Vec<ProviderHealth>, String> {
    use lmnotes_core::llm::ollama::OllamaProvider;
    use lmnotes_core::llm::openai::OpenAiProvider;
    use lmnotes_core::llm::LlmProvider;
    let mut results = Vec::new();
    for p in &config.providers {
        match p {
            crate::llm_config::ProviderConfig::Ollama { base_url, .. } => {
                let ollama = OllamaProvider::new(base_url);
                let ok = ollama.health().await.unwrap_or(false);
                results.push(ProviderHealth {
                    provider_id: "ollama".into(),
                    healthy: ok,
                });
            }
            crate::llm_config::ProviderConfig::OpenAi {
                id,
                base_url,
                api_key,
                ..
            } => {
                let openai = OpenAiProvider::new(id, base_url, api_key);
                let ok = openai.health().await.unwrap_or(false);
                results.push(ProviderHealth {
                    provider_id: id.clone(),
                    healthy: ok,
                });
            }
            crate::llm_config::ProviderConfig::WhisperCpp {
                model,
                binary_path,
                ffmpeg_path,
                threads,
            } => {
                // 探测本地 whisper.cpp：binary + 模型文件都存在才健康。
                let model_name = model.as_deref().unwrap_or("base");
                let binary = binary_path
                    .as_ref()
                    .map(PathBuf::from)
                    .or_else(whisper_binary_path);
                let model_p = models_dir().join(format!("ggml-{model_name}.bin"));
                let _ = ffmpeg_path; // ffmpeg 探测单独显示在 LocalSttStatus
                let _ = threads;
                let ok = binary
                    .as_ref()
                    .map(|b| b.exists() && model_p.exists())
                    .unwrap_or(false);
                results.push(ProviderHealth {
                    provider_id: "whisper-cpp".into(),
                    healthy: ok,
                });
            }
        }
    }
    Ok(results)
}

#[derive(serde::Serialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub healthy: bool,
}

// ============ Chat with Vault（T4）============

#[derive(serde::Serialize, Clone)]
pub struct CitationRefDto {
    pub index: usize,
    pub concept_id: String,
    pub path: String,
}

/// Chat with Vault：向量+全文检索 → 拼上下文 → LLM 流式回答 → 引用。
/// 携带对话历史（多轮），历史持久化到 chat_history 表。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn chat_stream(
    query: String,
    history: Vec<HistoryMsg>,
    window: tauri::WebviewWindow,
    sqlite: State<'_, Arc<SqliteIndex>>,
    meta: State<'_, Arc<dyn IndexBackend + '_>>,
    fulltext: State<'_, Arc<TantivyIndex>>,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<Vec<CitationRefDto>, String> {
    use lmnotes_core::llm::guard::{check as guard_check, GuardDecision};
    use lmnotes_core::llm::routing::Task;
    use lmnotes_core::qa::context::build_context;
    use lmnotes_core::qa::prompt::SYSTEM;
    use lmnotes_core::qa::retriever::Retriever;

    // 存用户消息到历史
    let _ = sqlite.append_chat_history("user", &query, None);

    // 1. 取 embed provider
    let (embedder, embed_model) = registry
        .embed_for(&routing, Task::Embed)
        .map_err(|e| e.to_string())?;

    // 2. 检索
    let retriever = Retriever::new(
        meta.inner().clone(),
        fulltext.inner().clone(),
        sqlite.inner().clone(),
        embedder,
        embed_model,
    );
    let chunks = retriever
        .retrieve(&query, 5)
        .await
        .map_err(|e| e.to_string())?;
    let (ctx, citations) = build_context(&chunks, 6000);

    // 3. 取 chat provider
    let (chat, model) = registry
        .chat_for(&routing, Task::Chat)
        .map_err(|e| e.to_string())?;

    // 4. 护栏检查
    let full_input = format!("{SYSTEM}\n\n【上下文】\n{ctx}\n\n【问题】\n{query}");
    match guard_check(&guard_cfg, chat.kind(), &full_input, false) {
        GuardDecision::Allow => {}
        GuardDecision::Deny(reason) => return Err(reason),
    }

    // 5. 构建 messages：system（含 context）+ 历史（最近 20 条）+ 当前 query
    let mut messages = vec![lmnotes_core::llm::ChatMessage {
        role: lmnotes_core::llm::ChatRole::System,
        content: format!("{SYSTEM}\n\n【上下文】\n{ctx}"),
    }];
    for h in history.iter().rev().take(20).rev() {
        messages.push(lmnotes_core::llm::ChatMessage {
            role: match h.role.as_str() {
                "user" => lmnotes_core::llm::ChatRole::User,
                "assistant" => lmnotes_core::llm::ChatRole::Assistant,
                _ => lmnotes_core::llm::ChatRole::User,
            },
            content: h.content.clone(),
        });
    }

    let req = lmnotes_core::llm::ChatRequest {
        model,
        messages,
        temperature: Some(0.4),
    };

    // 6. 流式 chat（推送 chunk 到前端）
    use futures_util::StreamExt;
    let mut stream = chat.chat_stream(req).await.map_err(|e| e.to_string())?;
    let mut full_answer = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(text) => {
                full_answer.push_str(&text);
                let _ = window.emit("chat-chunk", &text);
            }
            Err(e) => {
                let _ = window.emit("chat-error", e.to_string());
            }
        }
    }

    // 7. 存回答到历史 + 返回引用
    let cite_json = if citations.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&citations).unwrap_or_default())
    };
    let _ = sqlite.append_chat_history("assistant", &full_answer, cite_json.as_deref());

    let cite_dtos = citations
        .into_iter()
        .map(|c| CitationRefDto {
            index: c.index,
            concept_id: c.concept_id,
            path: c.path,
        })
        .collect();
    Ok(cite_dtos)
}

#[derive(serde::Deserialize)]
pub struct HistoryMsg {
    pub role: String,
    pub content: String,
}

/// 加载历史对话记录。
#[tauri::command]
pub fn load_chat_history(
    sqlite: State<'_, Arc<SqliteIndex>>,
) -> Result<Vec<lmnotes_core::index::sqlite::ChatHistoryRow>, String> {
    sqlite.load_chat_history().map_err(|e| e.to_string())
}

/// 清空历史对话记录。
#[tauri::command]
pub fn clear_chat_history(sqlite: State<'_, Arc<SqliteIndex>>) -> Result<(), String> {
    sqlite.clear_chat_history().map_err(|e| e.to_string())
}

// ============ 新建 + 导入笔记 ============

/// 新建笔记：创建带 frontmatter 的空 concept，返回相对路径。
#[tauri::command]
pub async fn create_note(title: String, parent_dir: Option<String>) -> Result<String, String> {
    use chrono::Utc;
    let dir = parent_dir.unwrap_or_else(|| "notes".into());
    let id = lmnotes_core::id::new_note_id(Utc::now().naive_utc());
    // 文件名：标题转 safe slug + id 后缀避免重名
    let slug: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let slug = if slug.is_empty() {
        id.rsplit_once('_')
            .map(|(_, s)| s)
            .unwrap_or("untitled")
            .to_string()
    } else {
        slug
    };
    let date = Utc::now().format("%Y%m%d").to_string();
    let path = format!("{dir}/{slug}-{date}.md");
    let content = format!(
        "---\ntype: note\nid: {id}\ntitle: {title}\ncreated: {ts}\n---\n\n# {title}\n\n",
        id = id,
        title = title,
        ts = Utc::now().format("%Y-%m-%dT%H:%M:%S+08:00")
    );
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &content)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// 运行时转录降级（ADR-0007）：薄封装核心层 `try_transcribe_with_fallback`。
///
/// 核心降级逻辑（候选顺序、护栏、网络错误识别、降级判定）在
/// `lmnotes_core::llm::transcribe_fallback`，有完整单测覆盖。
/// 壳层仅负责 String 错误转换（Tauri 命令返回 `Result<_, String>`）与降级日志。
pub(crate) async fn transcribe_with_fallback(
    registry: &Registry,
    routing: &Routing,
    guard_cfg: &GuardConfig,
    audio: lmnotes_core::llm::provider::AudioInput,
    language: Option<&str>,
    budget: std::time::Duration,
) -> Result<(lmnotes_core::llm::provider::Transcript, String), String> {
    // 候选日志默认关闭（每次转录都打太吵）；LMNOTES_DEBUG=1 打开用于排查。
    if std::env::var("LMNOTES_DEBUG").is_ok() {
        eprintln!(
            "transcribe: candidates = {:?}",
            registry
                .transcribe_candidates(routing, Task::Transcribe)
                .iter()
                .map(|(p, _)| p.id())
                .collect::<Vec<_>>()
        );
    }
    // 预算归调用点所有（v0.5.1）：内联 60s / 队列 15min；覆盖整条 fallback 链
    //（云端尝试 + 本地降级共享预算）。超时丢弃 future → kill_on_drop 终止子进程。
    tokio::time::timeout(
        budget,
        lmnotes_core::llm::transcribe_fallback::try_transcribe_with_fallback(
            registry, routing, guard_cfg, audio, language,
        ),
    )
    .await
    .map_err(|_| {
        let msg = format!("transcription timed out ({}s)", budget.as_secs());
        eprintln!("transcribe budget exceeded: {msg}");
        msg
    })?
    .map_err(|e| {
        eprintln!("transcribe failed: {e}");
        e.to_string()
    })
}

/// 共享：转录产物 → transcript concept 落盘 → 索引 + 后台建议。
/// voice（create_voice_note）与 media（create_media_note）同构复用（FR-CAP-04）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_and_write_transcript(
    asset_rel: String,
    transcript_text: &str,
    provider_id: &str,
    mime: &str,
    duration_ms: Option<u64>,
    language: Option<String>,
    title: Option<String>,
    id_slug: &str,
    indexer: &Arc<Indexer>,
    sqlite: &Arc<SqliteIndex>,
    registry: &Arc<Registry>,
    routing: &Arc<Routing>,
    guard_cfg: &Arc<GuardConfig>,
) -> Result<String, lmnotes_core::CoreError> {
    use chrono::Utc;
    use lmnotes_core::id::new_resource_id;
    use lmnotes_core::okf::concept::Concept;
    use lmnotes_core::okf::frontmatter::Frontmatter;
    use std::collections::BTreeMap;

    let now = Utc::now();
    let ts_display = now.format("%Y-%m-%d %H:%M").to_string();
    let title = title.unwrap_or_else(|| format!("Media transcript {ts_display}"));
    let mut extra = BTreeMap::new();
    if let Some(ms) = duration_ms {
        extra.insert("duration_ms".into(), serde_yaml::Value::Number(ms.into()));
    }
    extra.insert("mime".into(), serde_yaml::Value::String(mime.to_string()));
    extra.insert(
        "transcribed_by".into(),
        serde_yaml::Value::String(provider_id.to_string()),
    );
    let fm = Frontmatter {
        type_: "transcript".into(),
        title: Some(title.clone()),
        description: None,
        resource: Some(asset_rel),
        tags: vec![id_slug.to_string()],
        timestamp: Some(now),
        id: Some(new_resource_id(id_slug)),
        aliases: vec![],
        status: None,
        language,
        created: Some(now),
        extra,
    };
    let body = format!(
        "# {title}

{}
",
        transcript_text.trim()
    );
    let concept = Concept {
        frontmatter: fm,
        body,
    };
    let text = concept.to_string();

    let slug: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let slug = if slug.is_empty() {
        id_slug.to_string()
    } else {
        slug
    };
    let date = now.format("%Y%m%d").to_string();
    let path = format!("transcripts/{slug}-{date}.md");
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(lmnotes_core::CoreError::Io)?;
    }
    tokio::fs::write(&full, &text)
        .await
        .map_err(lmnotes_core::CoreError::Io)?;

    if let Err(e) = indexer.index_concept(&path, &text, &concept).await {
        eprintln!("transcript note index fail {path}: {e}");
    }
    let sqlite_c = sqlite.clone();
    let reg_c = registry.clone();
    let routing_c = routing.clone();
    let guard_c = guard_cfg.clone();
    let path_c = path.clone();
    let text_c = text.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = lmnotes_core::indexer::generate_suggestions(
            &concept, &path_c, &sqlite_c, &reg_c, &routing_c, &guard_c, &text_c,
        )
        .await
        {
            eprintln!("transcript suggestion fail {path_c}: {e}");
        }
    });
    Ok(path)
}

/// 1) 音频 SHA-256 去重归档到 assets/audio/
/// 2) 经路由取 transcribe provider（云端 Whisper 兼容）
/// 3) 过三层护栏（音频不可字符串扫描，仅 cloud_allowed + local_only 闸）
/// 4) 调转录 → 写 type: transcript concept 到 transcripts/（含 resource/duration_ms/mime/transcribed_by）
/// 5) 增量索引 + 后台生成 LLM 建议
/// 返回 transcript concept 的 vault-relative 路径（前端打开）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_voice_note(
    audio: Vec<u8>,
    ext: String,
    mime: String,
    duration_ms: u64,
    language: Option<String>,
    title: Option<String>,
    indexer: State<'_, Arc<Indexer>>,
    sqlite: State<'_, Arc<SqliteIndex>>,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<String, String> {
    use lmnotes_core::llm::provider::AudioInput;
    let id_slug = "voice"; // tags/id 前缀（media 流程用 "media"）

    // 1) 归档音频
    let (asset_rel, hash) = archive_binary(&audio, &ext, "audio").await?;

    // 2-4) 转录（含云端失败→本地 whisper.cpp 自动降级，ADR-0007）。
    // 返回 (Transcript, provider_id)——provider_id 写进 frontmatter 的 transcribed_by，
    // 用户可看出本次是云端还是本地转录的。
    let filename = format!("{hash}.{ext}");
    let mime_for_meta = mime.clone();
    let (tr, provider_id) = transcribe_with_fallback(
        &registry,
        &routing,
        &guard_cfg,
        AudioInput {
            bytes: audio,
            mime,
            filename,
        },
        language.as_deref(),
        // 内联速记预算：60s（长媒体走队列，v0.5.1）
        std::time::Duration::from_secs(60),
    )
    .await?;

    // 5-7) 共享：构建/落盘/索引 transcript concept（voice 与 media 同构）
    build_and_write_transcript(
        asset_rel,
        &tr.text,
        &provider_id,
        &mime_for_meta,
        Some(duration_ms),
        language,
        title,
        id_slug,
        indexer.inner(),
        sqlite.inner(),
        registry.inner(),
        routing.inner(),
        guard_cfg.inner(),
    )
    .await

    .map_err(|e| e.to_string())
}

/// 图片描述（FR-MEDIA-02）：已归档图片 → 视觉 LLM 描述/OCR → image-desc concept。
///
/// 护栏：图片 bytes 不可字符串扫描，同语音先例——仅 cloud_allowed 门控。
/// 产物 `type: image-desc`，resource 指原图，存 descriptions/（PRD §3.5）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn describe_image(
    asset_rel: String,
    indexer: State<'_, Arc<Indexer>>,
    sqlite: State<'_, Arc<SqliteIndex>>,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<String, String> {
    describe_image_core(
        &asset_rel,
        indexer.inner(),
        sqlite.inner(),
        registry.inner(),
        routing.inner(),
        guard_cfg.inner(),
    )
    .await
}

/// describe_image 的核心（worker 复用；&Arc 参数，无 Tauri State）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn describe_image_core(
    asset_rel: &str,
    indexer: &Arc<Indexer>,
    sqlite: &Arc<SqliteIndex>,
    registry: &Arc<Registry>,
    routing: &Arc<Routing>,
    guard_cfg: &Arc<GuardConfig>,
) -> Result<String, String> {
    use lmnotes_core::id::new_resource_id;
    use lmnotes_core::llm::provider::{ImageInput, DEFAULT_VISION_PROMPT};
    use lmnotes_core::okf::concept::Concept;
    use lmnotes_core::okf::frontmatter::Frontmatter;

    // 1) 读原图（限 assets/ 内，防穿越）
    let rel = asset_rel.trim_start_matches('/');
    if !rel.starts_with("assets/") || rel.contains("..") {
        return Err(format!("invalid asset path: {asset_rel}"));
    }
    let full = vault_root().join(rel);
    let bytes = tokio::fs::read(&full)
        .await
        .map_err(|e| format!("read image failed: {e}"))?;
    let mime = mime_from_ext(rel.rsplit('.').next().unwrap_or(""));

    // 2) 取 vision provider + 护栏
    let (vision, model) = registry
        .vision_for(routing, Task::Vision)
        .map_err(|e| e.to_string())?;
    match check(guard_cfg, vision.kind(), "", false) {
        GuardDecision::Allow => {}
        GuardDecision::Deny(reason) => return Err(reason),
    }
    let provider_id = vision.id().to_string();

    // 3) 描述（默认提示词含图中文字转写）
    let desc = vision
        .describe(
            ImageInput {
                bytes,
                mime: mime.clone(),
            },
            &model,
            Some(DEFAULT_VISION_PROMPT),
        )
        .await
        .map_err(|e| e.to_string())?;

    // 4) image-desc concept → descriptions/
    let now = chrono::Utc::now();
    let hash8 = rel
        .rsplit('/')
        .next()
        .and_then(|f| f.split('.').next())
        .unwrap_or("img")
        .chars()
        .take(8)
        .collect::<String>();
    let title = format!("Image {hash8}");
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("mime".into(), serde_yaml::Value::String(mime));
    extra.insert(
        "described_by".into(),
        serde_yaml::Value::String(format!("{model}@{provider_id}")),
    );
    let fm = Frontmatter {
        type_: "image-desc".into(),
        title: Some(title.clone()),
        description: None,
        resource: Some(format!("/{rel}")),
        tags: vec!["image-desc".into()],
        timestamp: Some(now),
        id: Some(new_resource_id("imgdesc")),
        aliases: vec![],
        status: None,
        language: None,
        created: Some(now),
        extra,
    };
    let body = format!(
        "# {title}

{desc}

![{title}](/{rel})
"
    );
    let concept = Concept {
        frontmatter: fm,
        body,
    };
    let text = concept.to_string();
    let path = format!("descriptions/img-{hash8}-{}.md", now.format("%Y%m%d"));
    let out = vault_root().join(&path);
    if let Some(pp) = out.parent() {
        tokio::fs::create_dir_all(pp)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&out, &text)
        .await
        .map_err(|e| e.to_string())?;

    // 5) 索引 + 建议（与 transcript 一致）
    if let Err(e) = indexer.index_concept(&path, &text, &concept).await {
        eprintln!("image-desc index fail {path}: {e}");
    }
    let sqlite_c = sqlite.clone();
    let reg_c = registry.clone();
    let routing_c = routing.clone();
    let guard_c = guard_cfg.clone();
    let path_c = path.clone();
    let text_c = text.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = lmnotes_core::indexer::generate_suggestions(
            &concept, &path_c, &sqlite_c, &reg_c, &routing_c, &guard_c, &text_c,
        )
        .await
        {
            eprintln!("image-desc suggestion fail {path_c}: {e}");
        }
    });
    Ok(path)
}

/// 扩展名 → mime（图片归档的有限集合）。
fn mime_from_ext(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ============ 数据导出（FR-STORE-05，v0.5）============

/// 递归收集 vault 内需导出的相对路径（排除 .lmnotes/ 派生数据）。
fn collect_export_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                // 跳过派生数据目录
                if p.file_name().map(|n| n == ".lmnotes").unwrap_or(false) {
                    continue;
                }
                collect_export_files(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

/// 导出 vault 为 zip（流式，排除 .lmnotes/）。dest 为绝对路径。
/// 返回写入的文件数。
#[tauri::command]
pub async fn export_vault_zip(dest: String, app: tauri::AppHandle) -> Result<u64, String> {
    use tauri::Emitter;
    let root = vault_root();
    let mut files = Vec::new();
    collect_export_files(&root, &mut files);
    files.sort();
    let out_path = PathBuf::from(&dest);
    if let Some(pp) = out_path.parent() {
        tokio::fs::create_dir_all(pp)
            .await
            .map_err(|e| e.to_string())?;
    }
    // zip 写放阻塞线程
    let root_c = root.clone();
    let files_c = files.clone();
    let dest_c = dest.clone();
    let count = tauri::async_runtime::spawn_blocking(move || -> Result<u64, String> {
        let file = std::fs::File::create(&dest_c).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let total = files_c.len() as u64;
        for (i, abs) in files_c.iter().enumerate() {
            let rel = abs
                .strip_prefix(&root_c)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace(BS, "/");
            zip.start_file(&rel, opts).map_err(|e| e.to_string())?;
            let mut f = std::fs::File::open(abs).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
            if i % 25 == 0 {
                let _ = app.emit(
                    "export-progress",
                    serde_json::json!({ "done": i as u64, "total": total }),
                );
            }
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(total)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(count)
}

/// 在当前 vault 初始化 git 仓库（探测 git CLI；无则返回指引性错误）。
/// init + .gitignore(.lmnotes/) + 首次提交（缺 user.name 时降级为仅 init）。
#[tauri::command]
pub async fn init_git_repo() -> Result<String, String> {
    // 1) 探测 git
    let probe = tokio::process::Command::new("git")
        .arg("--version")
        .output()
        .await;
    let probe = match probe {
        Ok(o) if o.status.success() => o,
        _ => {
            return Err("git not found in PATH. Install git first (git-scm.com).".into());
        }
    };
    let _ = String::from_utf8_lossy(&probe.stdout);

    let root = vault_root();
    if root.join(".git").exists() {
        return Err("this vault is already a git repository".into());
    }
    let gitignore = ".lmnotes/
";
    tokio::fs::write(root.join(".gitignore"), gitignore)
        .await
        .map_err(|e| e.to_string())?;

    let run = |args: &[&str]| {
        tokio::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
    };
    run(&["init"]).await.map_err(|e| e.to_string())?;
    let _ = run(&["add", "-A"]).await;
    match run(&["commit", "-m", "init: import existing notes"]).await {
        Ok(o) if o.status.success() => {
            Ok("git repository initialized with an initial commit".into())
        }
        _ => Ok(
            "git repository initialized. Configure user.name/user.email to make the first commit."
                .into(),
        ),
    }
}

// ============ 模板系统（FR-CAP-08，v0.5）============

#[derive(serde::Serialize)]
pub struct TemplateInfo {
    /// 模板文件名（含 .md）。
    pub name: String,
    /// vault 相对路径（templates/<name>）。
    pub path: String,
}

/// 列 vault 的 templates/ 目录（不存在返回空）。
#[tauri::command]
pub fn list_templates() -> Result<Vec<TemplateInfo>, String> {
    let dir = vault_root().join("templates");
    let mut out: Vec<TemplateInfo> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".md") {
                out.push(TemplateInfo {
                    name: name.trim_end_matches(".md").to_string(),
                    path: format!("templates/{name}"),
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 渲染模板占位符（纯函数，便于单测）。未识别的 {{…}} 保留原样。
fn render_template_placeholders(
    tpl: &str,
    title: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let local = now.with_timezone(&chrono::Local);
    s_matcher(
        tpl,
        &[
            ("{{title}}", title),
            ("{{date}}", &local.format("%Y-%m-%d").to_string()),
            ("{{time}}", &local.format("%H:%M").to_string()),
            ("{{datetime}}", &local.format("%Y-%m-%dT%H:%M").to_string()),
        ],
    )
}

/// 简单的按序字符串替换（每对全部替换）。
fn s_matcher(s: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = s.to_string();
    for (k, v) in pairs {
        out = out.replace(k, v);
    }
    out
}

/// 从模板新建笔记：读模板 → 占位符替换 → 落 notes/<parent_dir>。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_note_from_template(
    template_path: String,
    title: String,
    parent_dir: Option<String>,
    indexer: State<'_, Arc<Indexer>>,
    sqlite: State<'_, Arc<SqliteIndex>>,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<String, String> {
    use chrono::Utc;
    // 模板路径防护：必须 templates/ 下
    let tpl_rel = template_path.trim_start_matches('/');
    if !tpl_rel.starts_with("templates/") || tpl_rel.contains("..") {
        return Err(format!("invalid template path: {template_path}"));
    }
    let raw = tokio::fs::read_to_string(vault_root().join(tpl_rel))
        .await
        .map_err(|e| format!("read template failed: {e}"))?;
    let content = render_template_placeholders(&raw, &title, Utc::now());
    let dir = parent_dir.unwrap_or_else(|| "notes".into());

    let id = lmnotes_core::id::new_note_id(Utc::now().naive_utc());
    let slug: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let slug = if slug.is_empty() {
        id.rsplit_once('_')
            .map(|(_, s)| s)
            .unwrap_or("untitled")
            .to_string()
    } else {
        slug
    };
    let date = Utc::now().format("%Y%m%d").to_string();
    let path = format!("{dir}/{slug}-{date}.md");
    // frontmatter 若模板没有 → 补最小 frontmatter；有则把 title/id 替换为本次
    let final_text = if content.starts_with(
        "---
",
    ) {
        content.replacen(
            "---
",
            &format!(
                "---
title: {title}
id: {id}
"
            ),
            1,
        )
    } else {
        format!(
            "---
type: note
id: {id}
title: {title}
created: {}
---

{content}",
            Utc::now().format("%Y-%m-%dT%H:%M:%S+08:00")
        )
    };
    let full = vault_root().join(&path);
    if let Some(pp) = full.parent() {
        tokio::fs::create_dir_all(pp)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &final_text)
        .await
        .map_err(|e| e.to_string())?;
    // 索引（模板笔记同样应可检索）
    if let Ok(c) = Concept::parse(&final_text) {
        let _ = indexer.index_concept(&path, &final_text, &c).await;
        let sqlite_c = sqlite.inner().clone();
        let reg_c = registry.inner().clone();
        let routing_c = routing.inner().clone();
        let guard_c = guard_cfg.inner().clone();
        let path_c = path.clone();
        let text_c = final_text.clone();
        tauri::async_runtime::spawn(async move {
            let _ = lmnotes_core::indexer::generate_suggestions(
                &c, &path_c, &sqlite_c, &reg_c, &routing_c, &guard_c, &text_c,
            )
            .await;
        });
    }
    Ok(path)
}

// ============ 媒体任务队列（FR-MEDIA-04，v0.5）============

/// 媒体任务 DTO（前端任务中心行）。
#[derive(serde::Serialize, Clone)]
pub struct MediaTaskDto {
    pub id: String,
    pub kind: String,
    pub asset_rel: String,
    pub mime: String,
    pub duration_ms: Option<i64>,
    pub language: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub result_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&lmnotes_core::index::schema::MediaTask> for MediaTaskDto {
    fn from(t: &lmnotes_core::index::schema::MediaTask) -> Self {
        Self {
            id: t.id.clone(),
            kind: t.kind.clone(),
            asset_rel: t.asset_rel.clone(),
            mime: t.mime.clone(),
            duration_ms: t.duration_ms,
            language: t.language.clone(),
            status: t.status.clone(),
            error: t.error.clone(),
            result_path: t.result_path.clone(),
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

/// 由 mime + 任务 kind 决定归档桶（纯函数，便于单测）。
/// video/* → video；audio/* → audio；describe 任务（图片）→ img。
/// worker 以 `assets/video/` 前缀判定需抽音轨——分桶错误会让队列视频任务必败。
fn media_kind_dir(mime: Option<&str>, kind: &str) -> Result<&'static str, String> {
    let mime = mime.unwrap_or("");
    if kind == "describe" {
        return if mime.starts_with("image/") {
            Ok("img")
        } else {
            Err(format!("describe tasks require image/*, got {mime:?}"))
        };
    }
    if mime.starts_with("video/") {
        Ok("video")
    } else if mime.starts_with("audio/") {
        Ok("audio")
    } else {
        Err(format!(
            "transcribe tasks require audio/* or video/*, got {mime:?}"
        ))
    }
}

/// 任务入队：媒体已归档（或先归档 bytes）→ pending → worker 处理。
/// 接受两种调用：已归档（传 asset_rel）/ 未归档（传 data+ext+kind，先归档）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_media_task(
    kind: String, // transcribe | describe
    asset_rel: Option<String>,
    data: Option<Vec<u8>>,
    ext: Option<String>,
    mime: Option<String>,
    duration_ms: Option<u64>,
    language: Option<String>,
    sqlite: State<'_, Arc<SqliteIndex>>,
) -> Result<MediaTaskDto, String> {
    if kind != "transcribe" && kind != "describe" {
        return Err(format!("unknown task kind: {kind}"));
    }
    let asset_rel = match (asset_rel, data) {
        (Some(r), _) => r,
        (None, Some(bytes)) => {
            let ext = ext.ok_or("missing ext")?;
            // 按 mime 前缀分桶（GAP-B 修复：video 必须落 assets/video/——
            // worker 靠该前缀决定是否抽音轨，归错桶会导致队列视频任务必败）
            let kind_dir = media_kind_dir(mime.as_deref(), &kind)?;
            let (rel, _) = archive_binary(&bytes, &ext, kind_dir).await?;
            rel
        }
        (None, None) => return Err("either asset_rel or data is required".into()),
    };
    let mime = mime.unwrap_or_else(|| "application/octet-stream".into());
    let now = chrono::Utc::now().timestamp();
    let t = lmnotes_core::index::schema::MediaTask {
        id: format!(
            "mt_{}_{}",
            now,
            lmnotes_core::id::new_resource_id("t")
                .rsplit('_')
                .next()
                .unwrap_or("x")
        ),
        kind,
        asset_rel,
        mime,
        duration_ms: duration_ms.map(|d| d as i64),
        language,
        status: "pending".into(),
        error: None,
        result_path: None,
        created_at: now,
        updated_at: now,
    };
    sqlite.insert_media_task(&t).map_err(|e| e.to_string())?;
    Ok(MediaTaskDto::from(&t))
}

/// 列媒体任务（前端任务中心）。
#[tauri::command]
pub fn list_media_tasks(
    status: Option<String>,
    sqlite: State<'_, Arc<SqliteIndex>>,
) -> Result<Vec<MediaTaskDto>, String> {
    Ok(sqlite
        .list_media_tasks(status.as_deref())
        .map_err(|e| e.to_string())?
        .iter()
        .map(MediaTaskDto::from)
        .collect())
}

/// 重试失败任务（failed → pending）。
#[tauri::command]
pub fn retry_media_task(id: String, sqlite: State<'_, Arc<SqliteIndex>>) -> Result<(), String> {
    sqlite
        .update_media_task_status(&id, "pending", None, None)
        .map_err(|e| e.to_string())
}

/// 取消任务（仅 pending；running 不强杀——worker 单并发很快轮到）。
#[tauri::command]
pub fn cancel_media_task(
    id: String,
    sqlite: State<'_, Arc<SqliteIndex>>,
    cancels: State<'_, crate::media_tasks::CancelRegistry>,
) -> Result<(), String> {
    // 两步法（v0.5.1，严格防竞态）：
    // ① pending → 条件直翻（rows=0 说明 worker 已拉起为 running）
    // ② running → 经注册表 abort（worker 侧以条件 UPDATE 收尾，完成/取消不互相覆盖）
    if sqlite
        .cancel_pending_media_task(&id)
        .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    cancels.abort(&id);
    Ok(())
}

/// 媒体文件转转录笔记（FR-CAP-04）：拖拽/粘贴的音频或视频文件。
/// 音频：归档后直接转录（与 voice 同路径）。
/// 视频：归档到 assets/video/ → ffmpeg sidecar 抽 16kHz mono 音轨（-vn）→ 转录。
/// resource 指向原始媒体；tags/id 用 "media"（与 voice 流程区分）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_media_note(
    data: Vec<u8>,
    ext: String,
    mime: String,
    kind: String, // "audio" | "video"
    duration_ms: Option<u64>,
    language: Option<String>,
    title: Option<String>,
    indexer: State<'_, Arc<Indexer>>,
    sqlite: State<'_, Arc<SqliteIndex>>,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, Arc<GuardConfig>>,
) -> Result<String, String> {
    use lmnotes_core::llm::provider::AudioInput;
    let id_slug = "media";

    // 1) 归档原始媒体（audio → assets/audio/，video → assets/video/）
    let kind_dir = match kind.as_str() {
        "audio" => "audio",
        "video" => "video",
        other => return Err(format!("unsupported media kind: {other}")),
    };
    let (asset_rel, hash) = archive_binary(&data, &ext, kind_dir).await?;

    // 2) 视频先抽音轨：ffmpeg -i in -vn -ar 16000 -ac 1 pcm wav
    let audio_bytes: Vec<u8>;
    let audio_mime: String;
    let audio_ext: String;
    if kind == "video" {
        let ffmpeg = ffmpeg_binary_path().ok_or_else(|| {
            "video transcription requires the ffmpeg sidecar (not found)".to_string()
        })?;
        let src = vault_root().join(asset_rel.trim_start_matches('/'));
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let wav = tmp.path().join("audio.wav");
        extract_audio_track(&ffmpeg, &src, &wav).await?;
        audio_bytes = tokio::fs::read(&wav)
            .await
            .map_err(|e| format!("read extracted audio failed: {e}"))?;
        audio_mime = "audio/wav".into();
        audio_ext = "wav".into();
    } else {
        audio_bytes = data;
        audio_mime = mime.clone();
        audio_ext = ext.clone();
    }
    if audio_bytes.is_empty() {
        return Err("no audio track found in media file".into());
    }

    // 3) 转录（云优先/本地兜底/护栏，与 voice 一致）
    let filename = format!("{hash}.{audio_ext}");
    let mime_for_meta = mime.clone(); // frontmatter 记原始 mime
    let (tr, provider_id) = transcribe_with_fallback(
        &registry,
        &routing,
        &guard_cfg,
        AudioInput {
            bytes: audio_bytes,
            mime: audio_mime,
            filename,
        },
        language.as_deref(),
        // 内联速记预算：60s（长媒体走队列，v0.5.1）
        std::time::Duration::from_secs(60),
    )
    .await?;

    // 4-6) 共享 builder（resource 指原始媒体、duration 可空、tags=media）
    build_and_write_transcript(
        asset_rel,
        &tr.text,
        &provider_id,
        &mime_for_meta,
        duration_ms,
        language,
        title,
        id_slug,
        indexer.inner(),
        sqlite.inner(),
        registry.inner(),
        routing.inner(),
        guard_cfg.inner(),
    )
    .await

    .map_err(|e| e.to_string())
}

/// 构造 ffmpeg 抽音轨命令（纯函数便于单测参数拼装）。
/// `ffmpeg -y -i <in> -vn -ar 16000 -ac 1 -c:a pcm_s16le <out>`
pub(crate) fn build_extract_audio_cmd(
    ffmpeg: &Path,
    input: &Path,
    out: &Path,
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(ffmpeg);
    // v0.5.1 GAP-1 审计修复：超时丢弃 future 时必须连带杀 ffmpeg（孤儿进程防护）
    cmd.kill_on_drop(true);
    cmd.arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-vn") // 丢视频轨——视频转录的关键差异
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(out);
    cmd
}

/// ffmpeg 抽音轨（参数见 build_extract_audio_cmd）。
async fn extract_audio_track(ffmpeg: &Path, input: &Path, out: &Path) -> Result<(), String> {
    // v0.5.1 GAP-2 审计修复：内联路径 60s 预算（同内联转录；超时丢弃 future → kill_on_drop 杀 ffmpeg）。
    // 大视频请走队列（worker 抽音轨预算 15min）。
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        build_extract_audio_cmd(ffmpeg, input, out).output(),
    )
    .await
    .map_err(|_| "ffmpeg audio extraction timed out (60s)".to_string())?
    .map_err(|e| format!("ffmpeg spawn failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ffmpeg audio extraction failed: {}",
            stderr.chars().take(400).collect::<String>()
        ));
    }
    Ok(())
}

/// 导入 .md 文件：把外部文件复制到 vault，自动生成 frontmatter（若无）。
/// file_path 是用户系统的绝对路径。
#[tauri::command]
pub async fn import_note(file_path: String) -> Result<String, String> {
    use chrono::Utc;
    let src = std::path::PathBuf::from(&file_path);
    let name = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "imported".into());
    let id = lmnotes_core::id::new_note_id(Utc::now().naive_utc());
    let date = Utc::now().format("%Y%m%d").to_string();

    // 读源文件
    let raw = tokio::fs::read_to_string(&src)
        .await
        .map_err(|e| e.to_string())?;

    // 检查是否有 frontmatter（以 --- 开头）
    let content = if raw.trim_start().starts_with("---") {
        // 已有 frontmatter，直接用（但补 id 若无）
        if raw.contains("id:") {
            raw
        } else {
            // 在第一个 --- 后插入 id
            raw.replacen("---\n", &format!("---\nid: {id}\n"), 1)
        }
    } else {
        // 无 frontmatter，生成
        format!(
            "---\ntype: note\nid: {id}\ntitle: {name}\ncreated: {ts}\n---\n\n{raw}",
            id = id,
            name = name,
            ts = Utc::now().format("%Y-%m-%dT%H:%M:%S+08:00"),
            raw = raw
        )
    };

    // 目标路径
    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let slug = if slug.is_empty() {
        "imported".into()
    } else {
        slug
    };
    let path = format!("notes/{slug}-{date}.md");
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &content)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// 新建子文件夹。
#[tauri::command]
pub async fn create_folder(parent_dir: String, name: String) -> Result<String, String> {
    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '/')
        .take(50)
        .collect();
    let path = format!("{parent_dir}/{slug}");
    let full = vault_root().join(&path);
    tokio::fs::create_dir_all(&full)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// 在系统文件管理器中打开指定路径所在的文件夹，并选中该文件。
#[tauri::command]
pub async fn reveal_in_explorer(rel_path: String) -> Result<(), String> {
    let full = vault_root().join(&rel_path);

    // 规范化路径：rel_path 来自前端（用 / 分隔），join 后产生混合分隔符路径
    // （如 C:\...\.lmnotes\default\notes/ai）。Windows 能解析但 explorer.exe 不行。
    // 用 canonicalize 获取纯 Windows 路径。
    let canonical = full.canonicalize().unwrap_or_else(|_| full.clone());
    let path_str = canonical.to_string_lossy().to_string();

    // 辅助日志
    eprintln!("[reveal] rel_path={rel_path}");
    eprintln!("[reveal] vault_root={}", vault_root().display());
    eprintln!("[reveal] full_path={path_str}");
    eprintln!("[reveal] exists={}", full.exists());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // 用 /select,"路径" 直接调 explorer.exe（不加额外引号层）
        let select_arg = if full.is_dir() {
            path_str.clone()
        } else {
            format!(r#"/select,"{path_str}""#)
        };
        eprintln!("[reveal] explorer.exe raw_arg={select_arg}");

        let result = std::process::Command::new("explorer.exe")
            .raw_arg(&select_arg)
            .creation_flags(0x08000000)
            .spawn();
        eprintln!("[reveal] spawn result: {:?}", result.as_ref().err());
        result.map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path_str])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let dir = if full.is_dir() {
            full.clone()
        } else {
            full.parent().unwrap_or(&full).to_path_buf()
        };
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    let _ = &path_str;
    Ok(())
}
#[tauri::command]
pub async fn import_document(file_path: String) -> Result<String, String> {
    use chrono::Utc;
    let src = std::path::PathBuf::from(&file_path);
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let name = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "imported".into());
    let id = lmnotes_core::id::new_note_id(Utc::now().naive_utc());
    let date = Utc::now().format("%Y%m%d").to_string();

    let body = match ext.as_str() {
        "md" | "markdown" | "txt" => tokio::fs::read_to_string(&src)
            .await
            .map_err(|e| e.to_string())?,
        "pdf" => convert_pdf(&src)?,
        "docx" => convert_docx(&src)?,
        "xlsx" | "xls" => convert_xlsx(&src)?,
        _ => {
            return Err(format!(
                "不支持的格式: .{ext}（支持: pdf, docx, xlsx, txt, md）"
            ))
        }
    };

    let content = if body.trim_start().starts_with("---") {
        if body.contains("id:") {
            body
        } else {
            body.replacen("---\n", &format!("---\nid: {id}\n"), 1)
        }
    } else {
        format!(
            "---\ntype: note\nid: {id}\ntitle: {name}\ncreated: {ts}\n---\n\n{body}",
            ts = Utc::now().format("%Y-%m-%dT%H:%M:%S+08:00")
        )
    };

    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let slug = if slug.is_empty() {
        "imported".into()
    } else {
        slug
    };
    let path = format!("notes/{slug}-{date}.md");
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &content)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// PDF → text（best-effort）
fn convert_pdf(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| e.to_string())?;
    Ok(text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// DOCX → Markdown
fn convert_docx(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let docx = docx_rs::read_docx(&bytes).map_err(|e| e.to_string())?;
    let mut md = String::new();
    for child in &docx.document.children {
        match child {
            docx_rs::DocumentChild::Paragraph(para) => {
                let text = collect_para_text(para);
                if !text.is_empty() {
                    md.push_str(&text);
                    md.push_str("\n\n");
                }
            }
            docx_rs::DocumentChild::Table(table) => {
                md.push_str(&convert_docx_table(table));
                md.push_str("\n\n");
            }
            _ => {}
        }
    }
    Ok(md.trim().to_string())
}

fn collect_para_text(para: &docx_rs::Paragraph) -> String {
    let mut text = String::new();
    for child in &para.children {
        if let docx_rs::ParagraphChild::Run(run) = child {
            for rc in &run.children {
                if let docx_rs::RunChild::Text(t) = rc {
                    text.push_str(&t.text);
                }
            }
        }
    }
    text
}

fn convert_docx_table(table: &docx_rs::Table) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    let mut md = String::new();
    for (i, row_child) in table.rows.iter().enumerate() {
        let docx_rs::TableChild::TableRow(row) = row_child;
        let cells: Vec<String> = row
            .cells
            .iter()
            .map(|c| {
                let docx_rs::TableRowChild::TableCell(cell) = c;
                let mut t = String::new();
                for content in &cell.children {
                    if let docx_rs::TableCellContent::Paragraph(p) = content {
                        t.push_str(&collect_para_text(p));
                    }
                }
                t
            })
            .collect();
        md.push_str("| ");
        md.push_str(&cells.join(" | "));
        md.push_str(" |\n");
        if i == 0 {
            md.push_str("| ");
            md.push_str(&cells.iter().map(|_| "---").collect::<Vec<_>>().join(" | "));
            md.push_str(" |\n");
        }
    }
    md
}

/// XLSX/XLS → Markdown 表格（第一个 sheet）
fn convert_xlsx(path: &std::path::Path) -> Result<String, String> {
    use calamine::{open_workbook, Reader, Xls, Xlsx};
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut md = String::new();
    if ext == "xlsx" {
        let mut wb: Xlsx<_> =
            open_workbook(path).map_err(|e: calamine::XlsxError| e.to_string())?;
        if let Some(Ok(range)) = wb.worksheet_range_at(0) {
            md.push_str(&range_to_md(&range));
        }
    } else {
        let mut wb: Xls<_> = open_workbook(path).map_err(|e: calamine::XlsError| e.to_string())?;
        if let Some(first) = wb.sheet_names().first().cloned() {
            if let Ok(range) = wb.worksheet_range(&first) {
                md.push_str(&range_to_md(&range));
            }
        }
    }
    Ok(md.trim().to_string())
}

fn range_to_md(range: &calamine::Range<calamine::Data>) -> String {
    let mut md = String::new();
    for (i, row) in range.rows().enumerate() {
        md.push_str("| ");
        md.push_str(&row.iter().map(cell_to_str).collect::<Vec<_>>().join(" | "));
        md.push_str(" |\n");
        if i == 0 {
            md.push_str("| ");
            md.push_str(&row.iter().map(|_| "---").collect::<Vec<_>>().join(" | "));
            md.push_str(" |\n");
        }
    }
    md
}

fn cell_to_str(cell: &calamine::Data) -> String {
    use calamine::Data;
    match cell {
        Data::Int(i) => i.to_string(),
        Data::Float(f) => f.to_string(),
        Data::String(s) => s.clone(),
        Data::DateTime(d) => d.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Error(e) => format!("{e:?}"),
        Data::Empty => String::new(),
        Data::DurationIso(s) => s.clone(),
        Data::DateTimeIso(s) => s.clone(),
    }
}

// ============ 文件树 + 删除 ============

#[derive(serde::Serialize)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
}

/// 递归列出 vault 目录树（跳过 .lmnotes/）。
#[tauri::command]
pub fn list_tree(rel_path: Option<String>) -> Result<Vec<FileTreeNode>, String> {
    let root = vault_root();
    let base = match &rel_path {
        Some(p) => root.join(p),
        None => root.clone(),
    };
    Ok(list_dir_recursive(&root, &base))
}

fn list_dir_recursive(root: &std::path::Path, dir: &std::path::Path) -> Vec<FileTreeNode> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut nodes: Vec<FileTreeNode> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // 跳过隐藏目录 + .lmnotes
            if name.starts_with('.') {
                return None;
            }
            let full = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let rel = full
                .strip_prefix(root)
                .unwrap_or(&full)
                .to_string_lossy()
                .replace('\\', "/");
            if is_dir {
                let children = list_dir_recursive(root, &full);
                Some(FileTreeNode {
                    name,
                    path: rel,
                    is_dir: true,
                    children,
                })
            } else if name.ends_with(".md") {
                Some(FileTreeNode {
                    name,
                    path: rel,
                    is_dir: false,
                    children: vec![],
                })
            } else {
                None
            }
        })
        .collect();
    // 目录在前，文件在后
    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    nodes
}

/// 移动文件或文件夹到新目录（更新索引）。
/// src_path: vault 相对路径（如 notes/ai/attention.md）
/// dest_dir: 目标目录（如 notes/projects）
#[tauri::command]
pub async fn move_item(src_path: String, dest_dir: String) -> Result<String, String> {
    let root = vault_root();
    let src_full = root.join(&src_path);
    let file_name = src_full
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or("无法获取文件名")?;
    let dest_full = root.join(&dest_dir).join(&file_name);

    eprintln!("[move_item] {src_path} → {dest_dir}/{file_name}");

    // 确保目标目录存在
    tokio::fs::create_dir_all(root.join(&dest_dir))
        .await
        .map_err(|e| e.to_string())?;

    // 移动
    tokio::fs::rename(&src_full, &dest_full)
        .await
        .map_err(|e| e.to_string())?;

    // 返回新路径（vault 相对）
    let new_rel = format!("{dest_dir}/{file_name}");
    Ok(new_rel)
}

/// 删除笔记文件 + 从索引清除。
#[tauri::command]
pub async fn delete_note(path: String, indexer: State<'_, Arc<Indexer>>) -> Result<(), String> {
    let full = vault_root().join(&path);
    // 先从索引清除（读文件获取 concept id）
    if let Ok(text) = tokio::fs::read_to_string(&full).await {
        if let Ok(c) = lmnotes_core::okf::concept::Concept::parse(&text) {
            let id = c.frontmatter.id.unwrap_or_else(|| path.clone());
            indexer.unindex(&id).await.map_err(|e| e.to_string())?;
        }
    }
    // 删文件
    tokio::fs::remove_file(&full)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ============ 知识图谱命令（FR-SEARCH-03）============

/// 图谱节点 DTO（扁平，snake_case 供前端 invoke 直接消费）。
#[derive(serde::Serialize)]
pub struct GraphNodeDto {
    pub id: String,
    pub title: String,
    pub path: String,
}

/// 图谱边 DTO。
#[derive(serde::Serialize)]
pub struct GraphEdgeDto {
    pub src: String,
    pub dst: String,
    /// "explicit"（用户手写链接）| "semantic"（向量近邻）。
    pub kind: &'static str,
    /// 权重（显式=1.0；语义=相似度）。
    pub weight: f32,
}

/// 完整图谱 DTO。
#[derive(serde::Serialize)]
pub struct GraphDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

impl From<GraphData> for GraphDto {
    fn from(data: GraphData) -> Self {
        let nodes = data
            .nodes
            .into_iter()
            .map(|n| GraphNodeDto {
                id: n.id,
                title: n.title,
                path: n.path,
            })
            .collect();
        let edges = data
            .edges
            .into_iter()
            .map(|e| GraphEdgeDto {
                src: e.src,
                dst: e.dst,
                kind: match e.kind {
                    EdgeKind::Explicit => "explicit",
                    EdgeKind::Semantic => "semantic",
                },
                weight: e.weight,
            })
            .collect();
        GraphDto { nodes, edges }
    }
}

/// 全库图谱：全部节点 + 全部显式链接边。
/// 不含语义近邻边（语义边由 graph_neighborhood 按需展开）。
#[tauri::command]
pub fn graph_full(sqlite: State<'_, Arc<SqliteIndex>>) -> Result<GraphDto, String> {
    let idx: &SqliteIndex = sqlite.inner();
    let data = graph::build_full_graph(idx).map_err(|e| e.to_string())?;
    Ok(GraphDto::from(data))
}

/// 单点子图：focus 笔记的出链 + 入链 + 语义近邻。
/// `concept_id` 接受 concept id 或 path（前端只持有 path，这里优先按 path 反查）。
/// `k`/`threshold` 为 None 时用核心库默认值。
#[tauri::command]
pub fn graph_neighborhood(
    concept_id: String,
    k: Option<usize>,
    threshold: Option<f32>,
    sqlite: State<'_, Arc<SqliteIndex>>,
) -> Result<GraphDto, String> {
    let idx: &SqliteIndex = sqlite.inner();
    // 前端传入的通常是 path；优先按 path 解析成 concept id（与 MCP get_note_links 一致）。
    let id = match idx.get_concept(&concept_id) {
        Ok(Some(c)) => c.id, // 已是合法 id
        _ => match idx.get_concept_by_path(&concept_id) {
            Ok(Some(c)) => c.id,     // 是 path，反查到 id
            _ => concept_id.clone(), // 都查不到，原样传下去（返回空子图）
        },
    };
    let data = graph::build_neighborhood(idx, idx, &id, k, threshold).map_err(|e| e.to_string())?;
    Ok(GraphDto::from(data))
}

// ============ 本地 STT（whisper.cpp）模型管理（FR-MEDIA-05 / ADR-0007）============

/// 可下载的 whisper.cpp 模型元数据（来源：HuggingFace ggml-org/whisper.cpp）。
#[derive(serde::Serialize, Clone)]
pub struct WhisperModel {
    /// 模型短名（ggml-<name>.bin 的 <name>），如 "base"。
    pub name: String,
    /// 展示名（含参数量）。
    pub label: String,
    /// 大致体积（MB）。
    pub size_mb: u64,
    /// 是否已下载到 ~/.lmnotes/models/（结构化标记；推荐语文案由前端 i18n 渲染）。
    pub downloaded: bool,
    /// HF 下载直链。
    pub url: String,
}

/// 本地 STT 当前就绪状态。
#[derive(serde::Serialize)]
pub struct LocalSttStatus {
    /// whisper.cpp binary 是否就绪（sidecar 已打包/路径有效）。
    pub binary_available: bool,
    /// ffmpeg binary 是否就绪（转码用）。
    pub ffmpeg_available: bool,
    /// 已下载的模型名列表（ggml-<name>.bin 的 <name>）。
    pub models: Vec<String>,
}

/// 内置模型清单（MVP 只列 base/small/medium；large 留后续）。
/// 推荐语文案在前端 i18n（localStt.modelNote.<name>），后端只出结构化字段。
fn builtin_whisper_models() -> Vec<WhisperModel> {
    let base = "https://huggingface.co/ggml-org/whisper.cpp/resolve/main";
    vec![
        WhisperModel {
            name: "base".into(),
            label: "Base (74M)".into(),
            size_mb: 142,
            downloaded: false,
            url: format!("{base}/ggml-base.bin"),
        },
        WhisperModel {
            name: "small".into(),
            label: "Small (244M)".into(),
            size_mb: 466,
            downloaded: false,
            url: format!("{base}/ggml-small.bin"),
        },
        WhisperModel {
            name: "medium".into(),
            label: "Medium (769M)".into(),
            size_mb: 1480,
            downloaded: false,
            url: format!("{base}/ggml-medium.bin"),
        },
    ]
}

/// 列出可选 whisper.cpp 模型 + 标记已下载（结构化 downloaded 字段）。
#[tauri::command]
pub fn list_whisper_models() -> Vec<WhisperModel> {
    let downloaded: std::collections::HashSet<String> = list_downloaded_model_names();
    builtin_whisper_models()
        .into_iter()
        .map(|mut m| {
            m.downloaded = downloaded.contains(&m.name);
            m
        })
        .collect()
}

/// 扫描 models/ 目录，返回已下载的模型名（去 ggml- 前缀与 .bin 后缀）。
fn list_downloaded_model_names() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let dir = models_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if let Some(stripped) = name
                    .strip_prefix("ggml-")
                    .and_then(|s| s.strip_suffix(".bin"))
                {
                    set.insert(stripped.to_string());
                }
            }
        }
    }
    set
}

/// 自动注册 whisper.cpp 时选模型：优先 base（默认推荐档），
/// 否则确定性取一个已下载模型；无任何已下载模型时 None（退回 base 占位）。
pub fn preferred_downloaded_model() -> Option<String> {
    pick_preferred_model(&list_downloaded_model_names())
}

fn pick_preferred_model(names: &std::collections::HashSet<String>) -> Option<String> {
    if names.contains("base") {
        return Some("base".to_string());
    }
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted.into_iter().next().cloned()
}

// ============ 多 Vault 管理（FR-STORE-01，v0.4）============

/// vault 清单条目。
#[derive(serde::Serialize)]
pub struct VaultInfo {
    /// 绝对路径。
    pub path: String,
    /// 展示名（目录名）。
    pub name: String,
    /// 是否当前库。
    pub current: bool,
}

/// 列出已登记的 vault（标记当前库）。
#[tauri::command]
pub fn list_vaults() -> Vec<VaultInfo> {
    let cfg = crate::llm_config::Config::load_or_default();
    let cur = vault_root();
    let mut seen = false;
    let out: Vec<VaultInfo> = cfg
        .vaults
        .iter()
        .map(|p| {
            let pb = PathBuf::from(p);
            let is_cur = pb == cur;
            if is_cur {
                seen = true;
            }
            VaultInfo {
                path: p.clone(),
                name: pb
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.clone()),
                current: is_cur,
            }
        })
        .collect();
    // 当前库不在清单（如默认库未登记）→ 兜底补一条，UI 永远能看到当前库
    if !seen {
        let mut out = out;
        let cur_s = cur.to_string_lossy().into_owned();
        out.push(VaultInfo {
            name: cur
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| cur_s.clone()),
            path: cur_s,
            current: true,
        });
        out
    } else {
        out
    }
}

/// 登记一个 vault 目录（须已存在）。
#[tauri::command]
pub fn add_vault(path: String) -> Result<(), String> {
    let pb = PathBuf::from(&path);
    if !pb.is_dir() {
        return Err(format!("not a directory: {path}"));
    }
    let mut cfg = crate::llm_config::Config::load_or_default();
    if cfg.vaults.iter().any(|v| v == &path) {
        return Ok(()); // 幂等
    }
    cfg.vaults.push(path);
    cfg.save()
}

/// 移出一个 vault（仅出清单，不删数据；不可移出当前库）。
#[tauri::command]
pub fn remove_vault(path: String) -> Result<(), String> {
    if Path::new(&path) == vault_root() {
        return Err("cannot remove the current vault".into());
    }
    let mut cfg = crate::llm_config::Config::load_or_default();
    cfg.vaults.retain(|v| v != &path);
    if cfg.last_vault.as_deref() == Some(path.as_str()) {
        cfg.last_vault = None;
    }
    cfg.save()
}

/// 切换 vault：写 last_vault 后重启应用（重启式切换，ADR-0008——
/// 热切换需重建 indexer/engine/watcher/MCP 五组状态，侵入面大）。
#[tauri::command]
pub async fn switch_vault(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = crate::llm_config::Config::load_or_default();
    if !cfg.vaults.iter().any(|v| v == &path) {
        return Err(format!("vault not registered: {path}"));
    }
    if !PathBuf::from(&path).is_dir() {
        return Err(format!("vault directory missing: {path}"));
    }
    cfg.last_vault = Some(path);
    cfg.save()?;
    app.restart(); // -> !，不会返回
}

/// 探测本地 STT 就绪状态（前端开语音浮窗前调用，决定是否需下载模型）。
#[tauri::command]
pub fn get_local_stt_status() -> LocalSttStatus {
    LocalSttStatus {
        binary_available: whisper_binary_path().is_some(),
        ffmpeg_available: ffmpeg_binary_path().is_some(),
        models: list_downloaded_model_names().into_iter().collect(),
    }
}

/// 推测 whisper.cpp sidecar 路径。
/// 发布模式：Tauri externalBin 把 sidecar 安装到主程序同目录（resolve_sidecar 探测 exe 同级）。
/// 开发模式（无 sidecar 打包）：探测 ~/.lmnotes/bin/whisper[.exe]（用户手动放）或 PATH。
pub fn whisper_binary_path() -> Option<PathBuf> {
    resolve_sidecar("whisper")
}

pub(crate) fn ffmpeg_binary_path() -> Option<PathBuf> {
    resolve_sidecar("ffmpeg")
}

/// 解析 sidecar：依次查 env 覆盖、主程序同目录（externalBin 安装位）、
/// ~/.lmnotes/bin/、PATH（which）。
fn resolve_sidecar(name: &str) -> Option<PathBuf> {
    let exe_ext = if cfg!(windows) { ".exe" } else { "" };
    // 1) 环境变量显式覆盖（高级用户/CI）
    if let Ok(p) = std::env::var(format!("LMNOTES_{}_PATH", name.to_uppercase())) {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    // 2) 主程序同目录：Tauri externalBin 在打包安装时的落点
    //   （triple 后缀被剥回原始名，与主可执行文件并排）。
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(format!("{name}{exe_ext}"));
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }
    // 3) ~/.lmnotes/bin/<name>[.exe]（开发模式：用户手动放置）
    let local = lmnotes_home().join("bin").join(format!("{name}{exe_ext}"));
    if local.exists() {
        return Some(local);
    }
    // 4) PATH（仅 Unix which；Windows 走上面的 exe 同目录探测）
    #[cfg(unix)]
    {
        if let Ok(out) = std::process::Command::new("which").arg(name).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    None
}

/// 下载 whisper.cpp 模型到 ~/.lmnotes/models/。
/// 断点续传（HTTP Range）+ 最多 3 次重试 + 30s 停滞超时（ADR-0007 §缓解）。
/// 流式下载，定期 emit 进度事件 `whisper-model-progress`。返回本地模型文件路径。
#[tauri::command]
pub async fn download_whisper_model(name: String, app: tauri::AppHandle) -> Result<String, String> {
    let model = builtin_whisper_models()
        .into_iter()
        .find(|m| m.name == name)
        .ok_or_else(|| format!("unknown model: {name}"))?;
    let dir = models_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let dest = dir.join(format!("ggml-{}.bin", name));
    let tmp = dir.join(format!("ggml-{name}.bin.part"));

    // 幂等：已下载直接返回（顺手清理残留 .part）。
    if dest.exists() {
        let _ = tokio::fs::remove_file(&tmp).await;
        let _ = app.emit(
            "whisper-model-progress",
            serde_json::json!({ "name": name, "done": true }),
        );
        return Ok(dest.to_string_lossy().into_owned());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    const MAX_ATTEMPTS: u32 = 3;
    let stall = std::time::Duration::from_secs(30);
    let mut last_err = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        match download_attempt(&client, &model.url, &tmp, &dest, &name, &app, stall).await {
            Ok(path) => return Ok(path),
            Err(e) => {
                eprintln!("whisper model download attempt {attempt}/{MAX_ATTEMPTS} failed: {e}");
                last_err = e;
                // 失败保留 .part：下次尝试（或下次下载请求）从断点续传。
                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
    Err(last_err)
}

/// 单次下载尝试：按已有 .part 大小发 Range 续传；连续 30s 无数据视为停滞失败。
async fn download_attempt(
    client: &reqwest::Client,
    url: &str,
    tmp: &std::path::Path,
    dest: &std::path::Path,
    name: &str,
    app: &tauri::AppHandle,
    stall: std::time::Duration,
) -> Result<String, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    // 断点：已有 .part 的字节数。
    let mut start: u64 = tokio::fs::metadata(tmp).await.map(|m| m.len()).unwrap_or(0);
    let mut resp = if start > 0 {
        client
            .get(url)
            .header(reqwest::header::RANGE, format!("bytes={start}-"))
            .send()
            .await
            .map_err(|e| format!("download request: {e}"))?
    } else {
        client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("download request: {e}"))?
    };
    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // .part 比远端文件大（损坏/上游变更）：丢弃从头下。
        tokio::fs::remove_file(tmp)
            .await
            .map_err(|e| e.to_string())?;
        start = 0;
        resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("download request: {e}"))?;
    }
    if !resp.status().is_success() {
        return Err(format!("download HTTP {}", resp.status()));
    }
    // 206 = 续传命中（content_length 为剩余字节）；200 = 服务器未支持 Range，全量重下。
    let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let total = resp
        .content_length()
        .map(|r| if resumed { start + r } else { r });
    let mut file = if resumed {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(tmp)
            .await
            .map_err(|e| e.to_string())?
    } else {
        start = 0;
        tokio::fs::File::create(tmp)
            .await
            .map_err(|e| e.to_string())?
    };

    let mut stream = resp.bytes_stream();
    let mut downloaded = start;
    let mut last_emit = std::time::Instant::now();
    loop {
        let next = tokio::time::timeout(stall, stream.next())
            .await
            .map_err(|_| "download stalled: no data for 30s".to_string())?;
        let bytes = match next {
            None => break, // 流结束
            Some(c) => c.map_err(|e| format!("download stream: {e}"))?,
        };
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
        downloaded += bytes.len() as u64;
        // 节流：每 250ms emit 一次，避免事件风暴。
        if last_emit.elapsed() > std::time::Duration::from_millis(250) {
            let _ = app.emit(
                "whisper-model-progress",
                serde_json::json!({
                    "name": name,
                    "downloaded": downloaded,
                    "total": total,
                }),
            );
            last_emit = std::time::Instant::now();
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::rename(tmp, dest)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "whisper-model-progress",
        serde_json::json!({ "name": name, "done": true }),
    );
    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        build_extract_audio_cmd, media_kind_dir, parse_snapshot_ts, pick_preferred_model,
        render_template_placeholders,
    };
    use chrono::TimeZone;

    #[test]
    fn media_kind_dir_buckets_by_mime() {
        // GAP-B 回归：video mime 必须落 video 桶（旧实现归 audio → 队列视频任务必败）
        assert!(matches!(media_kind_dir(Some("video/mp4"), "transcribe"), Ok("video")));
        assert!(matches!(media_kind_dir(Some("video/webm"), "transcribe"), Ok("video")));
        assert!(matches!(media_kind_dir(Some("audio/webm"), "transcribe"), Ok("audio")));
        assert!(matches!(media_kind_dir(Some("audio/mpeg"), "transcribe"), Ok("audio")));
        assert!(matches!(media_kind_dir(Some("image/png"), "describe"), Ok("img")));
        // 不匹配的 mime 拒绝（防止静默归错桶）
        assert!(media_kind_dir(Some("video/mp4"), "describe").is_err());
        assert!(media_kind_dir(Some("image/png"), "transcribe").is_err());
        assert!(media_kind_dir(None, "transcribe").is_err());
    }

    #[test]
    fn template_placeholders_replace_all() {
        use chrono::{Datelike, Timelike};
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 8, 29, 14, 30, 0)
            .unwrap();
        // {{date}}/{{time}} 按本地时区渲染 → 期望值从同一时刻推导（时区无关）
        let local = now.with_timezone(&chrono::Local);
        let exp_date = format!("{}-{:02}-{:02}", local.year(), local.month(), local.day());
        let exp_time = format!("{}:{:02}", local.hour(), local.minute());
        let out = render_template_placeholders(
            "# {{title}}

日期 {{date}} 时间 {{time}} 完整 {{datetime}}
{{title}} 复用",
            "周会",
            now,
        );
        assert!(out.contains("# 周会"));
        assert!(out.contains(&format!("日期 {exp_date}")));
        assert!(out.contains(&format!("时间 {exp_time}")));
        assert!(out.contains(&format!("完整 {exp_date}T{exp_time}")));
        assert_eq!(out.matches("周会").count(), 2);
    }

    #[test]
    fn template_unknown_placeholder_preserved() {
        let out =
            render_template_placeholders("keep {{unknown}} and {{title}}", "T", chrono::Utc::now());
        assert!(
            out.contains("{{unknown}}"),
            "unknown placeholder must survive"
        );
        assert!(out.contains("T"));
    }
    #[test]
    fn extract_audio_cmd_drops_video_track() {
        // FR-CAP-04：视频转录必须 -vn 丢视频轨 + 16kHz 单声道 PCM
        let cmd = build_extract_audio_cmd(
            std::path::Path::new("ffmpeg"),
            std::path::Path::new("in.mp4"),
            std::path::Path::new("out.wav"),
        );
        let dbg = format!("{cmd:?}");
        // v0.5.1 GAP-1：与 build_whisper_cmd 同要求——kill_on_drop 必须开启
        assert!(
            dbg.contains("kill_on_drop: true"),
            "missing kill_on_drop: {dbg}"
        );
        for needle in ["-vn", "16000", "pcm_s16le", "in.mp4", "out.wav"] {
            assert!(dbg.contains(needle), "cmd missing {needle:?}: {dbg}");
        }
    }

    #[test]
    fn snapshot_ts_parses_valid_suffix() {
        assert_eq!(
            parse_snapshot_ts("notes_a_md-1770000000.md"),
            Some(1770000000)
        );
        assert_eq!(parse_snapshot_ts("x-1.md"), Some(1));
    }

    #[test]
    fn snapshot_ts_rejects_non_numeric_or_missing_suffix() {
        assert_eq!(parse_snapshot_ts("notes_a_md.md"), None);
        assert_eq!(parse_snapshot_ts("notes_a_md-abc.md"), None);
        assert_eq!(parse_snapshot_ts("notes_a_md.txt"), None);
    }

    fn names(list: &[&str]) -> std::collections::HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn pick_prefers_base_when_downloaded() {
        assert_eq!(
            pick_preferred_model(&names(&["small", "base"])),
            Some("base".into())
        );
    }

    #[test]
    fn pick_falls_back_to_any_downloaded_model() {
        // 只下载了 small/medium：必须选中已下载者（确定性取字典序最小），
        // 否则自动注册会指向不存在的 ggml-base.bin。
        assert_eq!(
            pick_preferred_model(&names(&["small"])),
            Some("small".into())
        );
        assert_eq!(
            pick_preferred_model(&names(&["medium", "small"])),
            Some("medium".into())
        );
    }

    #[test]
    fn pick_none_when_nothing_downloaded() {
        assert_eq!(pick_preferred_model(&names(&[])), None);
    }
}
