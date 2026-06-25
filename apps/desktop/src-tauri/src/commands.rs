//! Tauri 命令定义。M1a/M1b 逐步填充。

use lmnotes_core::backend::IndexBackend;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::Indexer;
use lmnotes_core::llm::guard::{check, GuardConfig, GuardDecision};
use lmnotes_core::llm::routing::{Registry, Routing, Task};
use lmnotes_core::llm::suggestion::{SuggestionRecord, SuggestionStatus};
use lmnotes_core::llm::{ChatMessage, ChatRequest, ChatRole};
use lmnotes_core::okf::concept::Concept;
use lmnotes_core::search::{SearchEngine, SearchHit};
use std::path::PathBuf;
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
    meta: State<'_, Arc<dyn IndexBackend>>,
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
