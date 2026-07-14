//! Tauri 命令定义。M1a/M1b 逐步填充。
//!
//! 文件 IO 用 std::fs/tokio::fs（Tauri 壳层，非核心库业务模块）。
//! 豁免 clippy.toml 的 std::fs 约束。

#![allow(clippy::disallowed_methods)]

use lmnotes_core::backend::IndexBackend;
use lmnotes_core::graph::{self, EdgeKind, GraphData};
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

/// 语音转录笔记（FR-CAP-05 + FR-MEDIA-01）：
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
    use chrono::Utc;
    use lmnotes_core::id::new_resource_id;
    use lmnotes_core::llm::provider::AudioInput;
    use lmnotes_core::okf::concept::Concept;
    use lmnotes_core::okf::frontmatter::Frontmatter;
    use std::collections::BTreeMap;

    // 1) 归档音频
    let (asset_rel, hash) = archive_binary(&audio, &ext, "audio").await?;
    let now = Utc::now();

    // 2) 取 transcribe provider（首选→降级）
    let (transcriber, model) = registry
        .transcribe_for(&routing, Task::Transcribe)
        .map_err(|e| e.to_string())?;
    let provider_id = transcriber.id().to_string();

    // 3) 护栏：音频不可字符串扫描，传空串；依赖 cloud_allowed + local_only 两道闸。
    //    此路径无源 concept，local_only 恒 false。云端默认被 cloud_allowed=false 拒。
    match check(&guard_cfg, transcriber.kind(), "", false) {
        GuardDecision::Allow => {}
        GuardDecision::Deny(reason) => return Err(reason),
    }

    // 4) 转录（mime 克隆一份，转录消费一份，frontmatter 存一份）
    let filename = format!("{hash}.{ext}");
    let mime_for_meta = mime.clone();
    let tr = transcriber
        .transcribe(
            AudioInput {
                bytes: audio,
                mime,
                filename,
            },
            &model,
            language.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;

    // 5) 构建 transcript concept（PRD §3.5 完整形态）
    let ts_display = now.format("%Y-%m-%d %H:%M").to_string();
    let title = title.unwrap_or_else(|| format!("Voice note {ts_display}"));
    let mut extra = BTreeMap::new();
    extra.insert(
        "duration_ms".into(),
        serde_yaml::Value::Number(duration_ms.into()),
    );
    extra.insert("mime".into(), serde_yaml::Value::String(mime_for_meta));
    extra.insert(
        "transcribed_by".into(),
        serde_yaml::Value::String(format!("{model}@{provider_id}")),
    );
    let fm = Frontmatter {
        type_: "transcript".into(),
        title: Some(title.clone()),
        description: None,
        resource: Some(asset_rel),
        tags: vec!["voice".into()],
        timestamp: Some(now),
        id: Some(new_resource_id("voice")),
        aliases: vec![],
        status: None,
        language,
        created: Some(now),
        extra,
    };
    let body = format!("# {title}\n\n{}\n", tr.text.trim());
    let concept = Concept {
        frontmatter: fm,
        body,
    };
    let text = concept.to_string();

    // 6) 写 transcripts/<slug>-<YYYYMMDD>.md（slug 逻辑同 create_note）
    let slug: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let slug = if slug.is_empty() {
        "voice".to_string()
    } else {
        slug
    };
    let date = now.format("%Y%m%d").to_string();
    let path = format!("transcripts/{slug}-{date}.md");
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &text)
        .await
        .map_err(|e| e.to_string())?;

    // 7) 增量索引 + 后台 LLM 建议（复刻 lib.rs watcher 分支）
    if let Err(e) = indexer.index_concept(&path, &text, &concept).await {
        eprintln!("voice note index fail {path}: {e}");
    }
    let sqlite_c = sqlite.inner().clone();
    let reg_c = registry.inner().clone();
    let routing_c = routing.inner().clone();
    let guard_c = guard_cfg.inner().clone();
    let path_c = path.clone();
    let text_c = text.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = lmnotes_core::indexer::generate_suggestions(
            &concept, &path_c, &sqlite_c, &reg_c, &routing_c, &guard_c, &text_c,
        )
        .await
        {
            eprintln!("voice note suggestion fail {path_c}: {e}");
        }
    });

    Ok(path)
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
