//! Provider 配置读写（M1b-T10）。存 ~/.lmnotes/config.json。
//!
//! Tauri 壳 crate 的配置读写是同步阻塞的（启动期一次性），不走 StorageBackend
//!（后者用于 vault 内 concept 文件）。ADR-0002 的 std::fs 约束针对核心库业务模块，
//! 此处豁免。

#![allow(clippy::disallowed_methods)]

use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::routing::{ProviderRef, Routing, Task};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    pub routing: RoutingConfig,
    pub guard: GuardConfigSer,
    /// MCP server 配置（暴露笔记给 AI agent）。向后兼容：旧 config.json 无此段时取默认。
    #[serde(default)]
    pub mcp: McpConfig,
    /// 媒体处理策略（v0.5 分流：短同步 / 长入队）。旧 config 无此段取默认。
    #[serde(default)]
    pub media: MediaConfig,
    /// 已登记的 vault 目录（绝对路径）。v0.4 多库（FR-STORE-01）；旧 config 无此段取默认单库。
    #[serde(default = "default_vaults")]
    pub vaults: Vec<String>,
    /// 启动时打开的 vault。None / 失效路径 → 回退默认库 ~/.lmnotes/default。
    #[serde(default)]
    pub last_vault: Option<String>,
    /// 快速捕获浮窗（v0.8 热键可配置）。旧 config 无此段取默认热键。
    #[serde(default)]
    pub capture: CaptureConfig,
}

/// 全局快捷键配置（Tauri accelerator 语法，如 `CmdOrCtrl+Shift+L`）。
/// 改动保存后需重启应用生效（注册发生在启动期）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
        }
    }
}

fn default_hotkey() -> String {
    "CmdOrCtrl+Shift+L".to_string()
}

/// 默认 vault 路径（M1a 以来的固定值）。
pub fn default_vault_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".lmnotes").join("default")
}

fn default_vaults() -> Vec<String> {
    vec![default_vault_path().to_string_lossy().into_owned()]
}

/// 纯函数：解析当前 vault（可测）。last_vault 存在且是目录 → 用之；否则默认库。
pub fn resolve_vault(last_vault: Option<&str>, default: &std::path::Path) -> std::path::PathBuf {
    match last_vault {
        Some(p) if !p.trim().is_empty() && std::path::Path::new(p).is_dir() => {
            std::path::PathBuf::from(p)
        }
        _ => default.to_path_buf(),
    }
}

/// 进程内当前 vault（重启式切换 → 进程内不变，OnceLock 缓存）。
/// 供 commands::vault_root / lib::vault_dir 收口委托。
pub fn current_vault() -> std::path::PathBuf {
    static VAULT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    VAULT
        .get_or_init(|| {
            let cfg = Config::load_or_default();
            resolve_vault(cfg.last_vault.as_deref(), &default_vault_path())
        })
        .clone()
}

/// 媒体处理策略（v0.5 设计 T4：短同步 / 长入队分流）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConfig {
    /// 超过该时长(ms)的音视频转录入队后台处理；以内同步执行。默认 60_000。
    #[serde(default = "default_background_threshold")]
    pub background_threshold_ms: u64,
}

fn default_background_threshold() -> u64 {
    60_000
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            background_threshold_ms: default_background_threshold(),
        }
    }
}

/// MCP server（暴露 vault 给 AI agent）的配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// 是否在启动时拉起 MCP HTTP server。默认开启。
    #[serde(default = "default_mcp_enabled")]
    pub enabled: bool,
    /// 绑定端口（仅 127.0.0.1）。0 = 由 OS 分配空闲端口；默认 21920。
    #[serde(default = "default_mcp_port")]
    pub port: u16,
    /// Bearer token；None 则启动时随机生成并写入 ~/.lmnotes/mcp.json 发现文件。
    #[serde(default)]
    pub token: Option<String>,
}

fn default_mcp_enabled() -> bool {
    true
}
fn default_mcp_port() -> u16 {
    21920
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: default_mcp_enabled(),
            port: default_mcp_port(),
            token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderConfig {
    /// Ollama 本地（id 固定 "ollama"，单实例；评审 R8）。
    #[serde(rename = "ollama")]
    Ollama {
        base_url: String,
        chat_model: String,
        embed_model: String,
        #[serde(default = "default_ollama_dim")]
        embed_dim: usize,
        /// 视觉模型（llava / llama3.2-vision 等）。None = 不启用视觉描述。
        #[serde(default)]
        vision_model: Option<String>,
    },
    #[serde(rename = "openai")]
    OpenAi {
        id: String,
        base_url: String,
        api_key: String,
        chat_model: String,
        embed_model: String,
        #[serde(default = "default_openai_dim")]
        embed_dim: usize,
        /// 转录模型（如 "whisper-1"、"whisper-large-v3"）。None 表示该 provider 不提供转录。
        /// 仅 OpenAI 兼容端点支持；Ollama 不支持转录。
        #[serde(default)]
        transcribe_model: Option<String>,
        /// 视觉模型（gpt-4o-mini / GLM-4V 等）。None = 不启用。
        #[serde(default)]
        vision_model: Option<String>,
    },
    /// 本地 whisper.cpp（ADR-0007）。云端不可用时的降级 provider。
    /// id 固定 "whisper-cpp"。binary/ffmpeg 路径缺省走 sidecar 解析（commands::resolve_sidecar）。
    #[serde(rename = "whisper_cpp")]
    WhisperCpp {
        /// 模型短名（ggml-<name>.bin 的 <name>，如 "base"/"small"）。None 用 "base"。
        #[serde(default)]
        model: Option<String>,
        /// 覆盖 whisper.cpp 二进制路径（高级用户；默认自动探测 sidecar）。
        #[serde(default)]
        binary_path: Option<String>,
        /// 覆盖 ffmpeg 二进制路径（默认自动探测；None 则跳过转码）。
        #[serde(default)]
        ffmpeg_path: Option<String>,
        /// CPU 线程数（默认 4）。
        #[serde(default = "default_whisper_threads")]
        threads: usize,
    },
}

fn default_whisper_threads() -> usize {
    4
}

fn default_ollama_dim() -> usize {
    768
}
fn default_openai_dim() -> usize {
    1024
}

impl ProviderConfig {
    /// 取该 Provider 的 embedding 维度。
    pub fn embed_dim(&self) -> usize {
        match self {
            ProviderConfig::Ollama { embed_dim, .. } => *embed_dim,
            ProviderConfig::OpenAi { embed_dim, .. } => *embed_dim,
            // whisper.cpp 不做 embedding，不参与 dim 协商
            ProviderConfig::WhisperCpp { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub summarize: Option<ProviderRefSer>,
    pub link_suggest: Option<ProviderRefSer>,
    pub embed: Option<ProviderRefSer>,
    pub chat: Option<ProviderRefSer>,
    pub rewrite: Option<ProviderRefSer>,
    /// 语音转录任务路由（FR-CAP-05）。None 表示不启用转录。
    #[serde(default)]
    pub transcribe: Option<ProviderRefSer>,
    /// 视觉描述任务路由（FR-MEDIA-02，v0.4）。None 时从 vision_model 自动派生。
    #[serde(default)]
    pub vision: Option<ProviderRefSer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRefSer {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuardConfigSer {
    #[serde(default)]
    pub cloud_allowed: bool,
    #[serde(default)]
    pub sensitive_patterns: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            providers: vec![ProviderConfig::Ollama {
                base_url: "http://localhost:11434".into(),
                chat_model: "qwen2.5:7b".into(),
                embed_model: "nomic-embed-text".into(),
                embed_dim: 768,
                vision_model: None,
            }],
            routing: RoutingConfig {
                summarize: Some(ProviderRefSer {
                    provider: "ollama".into(),
                    model: "qwen2.5:7b".into(),
                }),
                link_suggest: Some(ProviderRefSer {
                    provider: "ollama".into(),
                    model: "qwen2.5:7b".into(),
                }),
                chat: Some(ProviderRefSer {
                    provider: "ollama".into(),
                    model: "qwen2.5:7b".into(),
                }),
                rewrite: Some(ProviderRefSer {
                    provider: "ollama".into(),
                    model: "qwen2.5:7b".into(),
                }),
                embed: Some(ProviderRefSer {
                    provider: "ollama".into(),
                    model: "nomic-embed-text".into(),
                }),
                // 默认全本地：无转录 provider（需用户显式配云端 + transcribe_model）。
                transcribe: None,
                vision: None,
            },
            guard: GuardConfigSer::default(),
            mcp: McpConfig::default(),
            media: MediaConfig::default(),
            vaults: default_vaults(),
            last_vault: None,
            capture: CaptureConfig::default(),
        }
    }
}

fn config_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".lmnotes/config.json")
}

impl Config {
    pub fn load_or_default() -> Self {
        match std::fs::read_to_string(config_path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 取当前配置的 embedding 维度（从 Embed 任务路由的 Provider 取）。
    /// 若路由未配，取第一个 Provider 的维度；都没有则默认 768。
    pub fn embed_dim(&self) -> usize {
        if let Some(embed_ref) = &self.routing.embed {
            // 找匹配 id 的 provider
            for p in &self.providers {
                let pid = match p {
                    ProviderConfig::Ollama { .. } => "ollama",
                    ProviderConfig::OpenAi { id, .. } => id.as_str(),
                    ProviderConfig::WhisperCpp { .. } => "whisper-cpp",
                };
                if pid == embed_ref.provider {
                    return p.embed_dim();
                }
            }
        }
        // fallback: 第一个 provider 的 dim，或 768
        self.providers.first().map(|p| p.embed_dim()).unwrap_or(768)
    }

    /// 映射到核心层的 Registry + Routing + GuardConfig。
    pub fn build(&self) -> (lmnotes_core::llm::routing::Registry, Routing, GuardConfig) {
        self.build_with_probe(&SidecarProbe::real())
    }

    /// 同 build()，但给全部 provider 挂用量记录（v0.8 FR-MODEL-05，运行时路径）。
    pub fn build_with_sink(
        &self,
        sink: lmnotes_core::llm::usage::UsageSink,
    ) -> (lmnotes_core::llm::routing::Registry, Routing, GuardConfig) {
        self.build_with_probe_sink(&SidecarProbe::real(), Some(sink))
    }

    /// 同 build()，但 sidecar/模型探测可注入（单测伪造，不依赖真实文件系统）。
    pub(crate) fn build_with_probe(
        &self,
        probe: &SidecarProbe,
    ) -> (lmnotes_core::llm::routing::Registry, Routing, GuardConfig) {
        self.build_with_probe_sink(probe, None)
    }

    /// build 全路径：探测注入 + 可选用量 sink（None = 不记录，单测/探测用）。
    pub(crate) fn build_with_probe_sink(
        &self,
        probe: &SidecarProbe,
        sink: Option<lmnotes_core::llm::usage::UsageSink>,
    ) -> (lmnotes_core::llm::routing::Registry, Routing, GuardConfig) {
        use lmnotes_core::llm::usage::{RecordingChat, RecordingEmbed, RecordingTranscribe, RecordingVision};
        use lmnotes_core::llm::{ChatCap, EmbedCap, VisionCap};
        use std::sync::Arc;

        fn wrap_chat<P: ChatCap + 'static>(
            reg: &mut lmnotes_core::llm::routing::Registry,
            arc: Arc<P>,
            sink: &Option<lmnotes_core::llm::usage::UsageSink>,
        ) {
            match sink {
                Some(s) => reg.register_chat_arc(RecordingChat::arc(arc, s.clone())),
                None => reg.register_chat_arc(arc),
            }
        }
        fn wrap_embed<P: EmbedCap + 'static>(
            reg: &mut lmnotes_core::llm::routing::Registry,
            arc: Arc<P>,
            sink: &Option<lmnotes_core::llm::usage::UsageSink>,
        ) {
            match sink {
                Some(s) => reg.register_embed_arc(RecordingEmbed::arc(arc, s.clone())),
                None => reg.register_embed_arc(arc),
            }
        }
        fn wrap_vision<P: VisionCap + 'static>(
            reg: &mut lmnotes_core::llm::routing::Registry,
            arc: Arc<P>,
            sink: &Option<lmnotes_core::llm::usage::UsageSink>,
        ) {
            match sink {
                Some(s) => reg.register_vision_arc(RecordingVision::arc(arc, s.clone())),
                None => reg.register_vision_arc(arc),
            }
        }
        use lmnotes_core::llm::ollama::OllamaProvider;
        use lmnotes_core::llm::openai::OpenAiProvider;
        use lmnotes_core::llm::whisper::WhisperProvider;
        let mut reg = lmnotes_core::llm::routing::Registry::new();
        let mut user_configured_whisper_cpp = false;
        // 注册成功时本地 whisper.cpp 实际使用的模型短名（路由表与之保持一致）。
        let mut effective_local_model: Option<String> = None;
        for p in &self.providers {
            match p {
                ProviderConfig::Ollama {
                    base_url,
                    vision_model,
                    ..
                } => {
                    let ollama = std::sync::Arc::new(OllamaProvider::new(base_url));
                    wrap_chat(&mut reg, ollama.clone(), &sink);
                    wrap_embed(&mut reg, ollama.clone(), &sink);
                    if vision_model.is_some() {
                        wrap_vision(&mut reg, ollama, &sink);
                    }
                }
                ProviderConfig::OpenAi {
                    id,
                    base_url,
                    api_key,
                    vision_model,
                    ..
                } => {
                    let openai = std::sync::Arc::new(OpenAiProvider::new(id, base_url, api_key));
                    wrap_chat(&mut reg, openai.clone(), &sink);
                    wrap_embed(&mut reg, openai.clone(), &sink);
                    if vision_model.is_some() {
                        wrap_vision(&mut reg, openai, &sink);
                    }
                }
                ProviderConfig::WhisperCpp {
                    model,
                    binary_path,
                    ffmpeg_path,
                    threads,
                } => {
                    if register_whisper_cpp(
                        &mut reg,
                        model.as_deref(),
                        binary_path.as_deref(),
                        ffmpeg_path.as_deref(),
                        *threads,
                        probe,
                    ) {
                        user_configured_whisper_cpp = true;
                        effective_local_model = Some(effective_model(model.as_deref(), probe));
                    }
                }
            }
            // 转录能力：仅 OpenAI 兼容 provider 配了 transcribe_model 时注册
            //（独立 WhisperProvider 实例，与 chat/embed 同 id 但不同能力 map）。
            if let ProviderConfig::OpenAi {
                id,
                base_url,
                api_key,
                transcribe_model: Some(_),
                ..
            } = p
            {
                let whisper = std::sync::Arc::new(WhisperProvider::new(id, base_url, api_key));
                match &sink {
                    Some(s) => reg.register_transcribe_arc(RecordingTranscribe::arc(whisper, s.clone())),
                    None => reg.register_transcribe_arc(whisper),
                }
            }
        }
        // 自动注册 whisper.cpp 作为本地降级 provider（ADR-0007 §决策"开箱可用"）：
        // 即使用户 config.json 未声明 WhisperCpp，只要 sidecar 可探测到就注册，
        // 使其天然成为云端 transcribe 的 fallback。
        // 模型取实际已下载者（优先 base）——用户经 UI 下载 small/medium 后，
        // 不能仍指向默认 base，否则 model_path 落在未下载的 ggml-base.bin 上。
        let auto_whisper_cpp = if !user_configured_whisper_cpp {
            let ok =
                register_whisper_cpp(&mut reg, None, None, None, default_whisper_threads(), probe);
            if ok {
                effective_local_model = Some(effective_model(None, probe));
            }
            ok
        } else {
            false
        };
        let whisper_cpp_registered = user_configured_whisper_cpp || auto_whisper_cpp;
        let routing = self.build_routing(whisper_cpp_registered, &effective_local_model);
        let guard = GuardConfig {
            cloud_allowed: self.guard.cloud_allowed,
            sensitive_patterns: self.guard.sensitive_patterns.clone(),
        };
        (reg, routing, guard)
    }

    fn build_routing(&self, whisper_cpp_registered: bool, local_model: &Option<String>) -> Routing {
        let mut map = std::collections::HashMap::new();
        let to_ref = |r: &ProviderRefSer| {
            (
                ProviderRef {
                    provider_id: r.provider.clone(),
                    model: r.model.clone(),
                },
                vec![],
            )
        };
        if let Some(r) = &self.routing.summarize {
            map.insert(Task::Summarize, to_ref(r));
        }
        if let Some(r) = &self.routing.link_suggest {
            map.insert(Task::LinkSuggest, to_ref(r));
        }
        if let Some(r) = &self.routing.embed {
            map.insert(Task::Embed, to_ref(r));
        }
        if let Some(r) = &self.routing.chat {
            map.insert(Task::Chat, to_ref(r));
        }
        if let Some(r) = &self.routing.rewrite {
            map.insert(Task::Rewrite, to_ref(r));
        }
        // Transcribe 路由：primary + whisper-cpp 作为本地降级 fallback（ADR-0007）。
        // primary 来自显式 routing.transcribe、或自动派生的云端 provider、或仅本地。
        // local 的 model 与注册时的模型选择保持一致（已下载者，缺省 base）。
        let local_model = local_model.clone().unwrap_or_else(|| "base".to_string());
        let local_fb = if whisper_cpp_registered {
            vec![ProviderRef {
                provider_id: "whisper-cpp".into(),
                model: local_model.clone(),
            }]
        } else {
            vec![]
        };
        if let Some(r) = &self.routing.transcribe {
            map.insert(
                Task::Transcribe,
                (
                    ProviderRef {
                        provider_id: r.provider.clone(),
                        model: r.model.clone(),
                    },
                    local_fb.clone(),
                ),
            );
        } else if let Some((pid, model)) = self.derive_transcribe_ref() {
            // 用户在 provider 上填了 transcribe_model 但未显式配 routing.transcribe：
            // 自动派生，避免"配了模型却路由不到"的常见陷阱。
            map.insert(
                Task::Transcribe,
                (
                    ProviderRef {
                        provider_id: pid,
                        model,
                    },
                    local_fb.clone(),
                ),
            );
        } else if whisper_cpp_registered {
            // 无云端 provider：本地 whisper.cpp 当 primary（仅本地模式）。
            map.insert(
                Task::Transcribe,
                (
                    ProviderRef {
                        provider_id: "whisper-cpp".into(),
                        model: local_model,
                    },
                    vec![],
                ),
            );
        }
        // Vision 路由（FR-MEDIA-02）：显式 routing.vision > 从 vision_model 自动派生
        if let Some(r) = &self.routing.vision {
            map.insert(Task::Vision, to_ref(r));
        } else if let Some((pid, model)) = self.derive_vision_ref() {
            map.insert(
                Task::Vision,
                (
                    ProviderRef {
                        provider_id: pid,
                        model,
                    },
                    vec![],
                ),
            );
        }
        Routing { map }
    }

    /// 找第一个配了 vision_model 的 provider（Ollama/OpenAi 均可）。
    fn derive_vision_ref(&self) -> Option<(String, String)> {
        for p in &self.providers {
            let (pid, vm) = match p {
                ProviderConfig::Ollama {
                    vision_model: Some(m),
                    ..
                } => ("ollama", m.clone()),
                ProviderConfig::OpenAi {
                    id,
                    vision_model: Some(m),
                    ..
                } => (id.as_str(), m.clone()),
                _ => continue,
            };
            return Some((pid.to_string(), vm));
        }
        None
    }

    /// 找第一个配了 transcribe_model 的 OpenAi 兼容 provider，返回 (id, model)。
    /// 用于 routing.transcribe 缺省时自动派生。
    fn derive_transcribe_ref(&self) -> Option<(String, String)> {
        for p in &self.providers {
            if let ProviderConfig::OpenAi {
                id,
                transcribe_model: Some(m),
                ..
            } = p
            {
                return Some((id.clone(), m.clone()));
            }
        }
        None
    }
}

/// sidecar / 已下载模型探测结果。可注入：单测用伪造值，运行时用 `real()`。
pub(crate) struct SidecarProbe {
    pub whisper: Option<std::path::PathBuf>,
    pub ffmpeg: Option<std::path::PathBuf>,
    /// 自动注册时选中的已下载模型（优先 base）；None = 无已下载模型。
    pub preferred_model: Option<String>,
}

impl SidecarProbe {
    fn real() -> Self {
        Self {
            whisper: crate::commands::whisper_binary_path(),
            ffmpeg: crate::commands::ffmpeg_binary_path(),
            preferred_model: crate::commands::preferred_downloaded_model(),
        }
    }
}

/// 模型短名决策链：用户显式 > 已下载探测 > "base" 占位。
fn effective_model(user: Option<&str>, probe: &SidecarProbe) -> String {
    user.or(probe.preferred_model.as_deref())
        .unwrap_or("base")
        .to_string()
}

/// 注册 whisper.cpp 本地 provider 到 Registry。返回是否成功注册（sidecar 可达）。
/// 失败（binary 不可达）静默跳过——本地 STT 在运行时降级时由前端引导下载。
/// 路径优先级：用户显式 binary_path > probe 探测（exe 同目录/env/~/.lmnotes/bin）。
fn register_whisper_cpp(
    reg: &mut lmnotes_core::llm::routing::Registry,
    model: Option<&str>,
    binary_path: Option<&str>,
    ffmpeg_path: Option<&str>,
    threads: usize,
    probe: &SidecarProbe,
) -> bool {
    let Some(provider) = resolve_whisper_cpp(model, binary_path, ffmpeg_path, threads, probe)
    else {
        return false;
    };
    reg.register_transcribe(provider);
    true
}

/// 组装 WhisperCppProvider（纯装配，不碰文件系统，便于单测注入 probe）。
fn resolve_whisper_cpp(
    model: Option<&str>,
    binary_path: Option<&str>,
    ffmpeg_path: Option<&str>,
    threads: usize,
    probe: &SidecarProbe,
) -> Option<crate::whisper_cpp::WhisperCppProvider> {
    use crate::commands::models_dir;
    use crate::whisper_cpp::WhisperCppProvider;
    // binary：显式 > 自动探测
    let binary = binary_path
        .map(std::path::PathBuf::from)
        .or_else(|| probe.whisper.clone())?;
    // ffmpeg：显式 > 自动探测（None 允许——仅 WAV 直通场景）
    let ffmpeg = ffmpeg_path
        .map(std::path::PathBuf::from)
        .or_else(|| probe.ffmpeg.clone());
    // 模型：~/.lmnotes/models/ggml-<name>.bin
    let model_name = effective_model(model, probe);
    let model_p = models_dir().join(format!("ggml-{model_name}.bin"));
    Some(WhisperCppProvider::new(
        "whisper-cpp",
        binary,
        ffmpeg,
        model_p,
        threads,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmnotes_core::llm::routing::Task;

    /// 辅助：构造一个带云端 OpenAi provider（含 transcribe_model）的 Config。
    fn config_with_transcribe(
        transcribe_model: Option<&str>,
        explicit_routing_transcribe: Option<ProviderRefSer>,
    ) -> Config {
        Config {
            providers: vec![ProviderConfig::OpenAi {
                id: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                api_key: "sk-test".into(),
                chat_model: "gpt-4o-mini".into(),
                embed_model: "text-embedding-3-small".into(),
                embed_dim: 1536,
                transcribe_model: transcribe_model.map(String::from),
                vision_model: None,
            }],
            routing: RoutingConfig {
                transcribe: explicit_routing_transcribe,
                ..Default::default()
            },
            guard: GuardConfigSer {
                cloud_allowed: true,
                ..Default::default()
            },
            mcp: McpConfig::default(),
            media: MediaConfig::default(),
            vaults: default_vaults(),
            last_vault: None,
            capture: CaptureConfig::default(),
        }
    }

    #[test]
    fn build_auto_derives_transcribe_routing_from_provider_model() {
        // 用户只在 provider 上填了 transcribe_model，未配 routing.transcribe。
        // build() 应自动派生路由，使 transcribe_for 能解析到 provider。
        let cfg = config_with_transcribe(Some("whisper-1"), None);
        let (reg, routing, _guard) = cfg.build();
        let resolved = reg.transcribe_for(&routing, Task::Transcribe);
        assert!(resolved.is_ok(), "expected transcribe routing to resolve");
        let (_provider, model) = resolved.unwrap();
        assert_eq!(model, "whisper-1");
    }

    #[test]
    fn explicit_transcribe_routing_takes_precedence() {
        // 显式配 routing.transcribe 时，不被自动派生覆盖。
        let cfg = config_with_transcribe(
            Some("whisper-1"),
            Some(ProviderRefSer {
                provider: "openai".into(),
                model: "whisper-large-v3".into(),
            }),
        );
        let (_reg, routing, _guard) = cfg.build();
        // 路由 map 里应有 transcribe 项（解析依赖注册的 provider，这里只验 routing 内容）
        let (entry, _fbs) = routing.map.get(&Task::Transcribe).unwrap();
        assert_eq!(entry.model, "whisper-large-v3");
    }

    #[test]
    fn build_auto_derives_vision_routing_from_vision_model() {
        // provider 配了 vision_model 即启用视觉路由（无需显式 routing.vision）。
        let mut cfg = config_with_transcribe(None, None);
        if let ProviderConfig::OpenAi { vision_model, .. } = &mut cfg.providers[0] {
            *vision_model = Some("gpt-4o-mini".into());
        }
        let (reg, routing, _guard) = cfg.build();
        let resolved = reg.vision_for(&routing, Task::Vision);
        assert!(resolved.is_ok(), "vision routing should resolve");
        assert_eq!(resolved.unwrap().1, "gpt-4o-mini");
    }

    #[test]
    fn build_without_vision_model_has_no_vision_routing() {
        let cfg = config_with_transcribe(None, None);
        let (reg, routing, _guard) = cfg.build();
        assert!(!routing.map.contains_key(&Task::Vision));
        assert!(reg.vision_for(&routing, Task::Vision).is_err());
    }

    #[test]
    fn build_without_transcribe_model_has_no_transcribe_routing() {
        // provider 未配 transcribe_model 且无显式路由 → 不应插入 Transcribe 路由。
        let cfg = config_with_transcribe(None, None);
        let (reg, routing, _guard) = cfg.build();
        assert!(
            !routing.map.contains_key(&Task::Transcribe),
            "no transcribe routing expected"
        );
        assert!(reg.transcribe_for(&routing, Task::Transcribe).is_err());
    }

    // ── WhisperCpp 配置变体（ADR-0007）──────────────────────────────────

    #[test]
    fn whisper_cpp_config_round_trips_through_json() {
        // 用户在 config.json 写 whisper_cpp provider，应正确反序列化 + 回写。
        let json = r#"{
            "providers": [
                {"type":"whisper_cpp","model":"small","threads":8}
            ],
            "routing": {},
            "guard": {"cloud_allowed": false, "sensitive_patterns": []}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.providers.len(), 1);
        match &cfg.providers[0] {
            ProviderConfig::WhisperCpp {
                model,
                threads,
                binary_path,
                ffmpeg_path,
            } => {
                assert_eq!(model.as_deref(), Some("small"));
                assert_eq!(*threads, 8);
                assert!(binary_path.is_none());
                assert!(ffmpeg_path.is_none());
            }
            other => panic!("expected WhisperCpp, got {other:?}"),
        }
        // 回写 JSON 应能再次解析（round-trip）
        let reserialized = serde_json::to_string(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(cfg.providers.len(), cfg2.providers.len());
    }

    #[test]
    fn whisper_cpp_defaults_threads_and_model_when_omitted() {
        // 缺省 model=None（→ base）、threads=4（default_whisper_threads）。
        let json = r#"{
            "providers": [{"type":"whisper_cpp"}],
            "routing": {},
            "guard": {"cloud_allowed": false, "sensitive_patterns": []}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        match &cfg.providers[0] {
            ProviderConfig::WhisperCpp { model, threads, .. } => {
                assert!(model.is_none(), "model should default to None (→ base)");
                assert_eq!(*threads, 4, "threads should default to 4");
            }
            other => panic!("expected WhisperCpp, got {other:?}"),
        }
    }

    // ── 自动注册（注入 SidecarProbe，评审 I7）────────────────────────────

    fn probe_with_sidecar(model: Option<&str>) -> SidecarProbe {
        SidecarProbe {
            whisper: Some(std::path::PathBuf::from("/fake/bin/whisper.exe")),
            ffmpeg: Some(std::path::PathBuf::from("/fake/bin/ffmpeg.exe")),
            preferred_model: model.map(String::from),
        }
    }

    #[test]
    fn auto_registers_whisper_cpp_as_fallback_when_sidecar_available() {
        // 云端 provider 在、sidecar 可探测 → 候选 = [cloud, whisper-cpp]。
        let cfg = config_with_transcribe(Some("whisper-1"), None);
        let (reg, routing, _) = cfg.build_with_probe(&probe_with_sidecar(Some("small")));
        let cands = reg.transcribe_candidates(&routing, Task::Transcribe);
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0].0.id(), "openai");
        assert_eq!(cands[1].0.id(), "whisper-cpp");
        // 路由 fallback 槽位的 model 与注册模型一致（已下载 small）。
        assert_eq!(cands[1].1, "small");
    }

    #[test]
    fn no_auto_registration_without_sidecar() {
        // sidecar 探测不到 → 不注册 whisper-cpp，候选只有云端。
        let cfg = config_with_transcribe(Some("whisper-1"), None);
        let probe = SidecarProbe {
            whisper: None,
            ffmpeg: None,
            preferred_model: Some("small".into()),
        };
        let (reg, routing, _) = cfg.build_with_probe(&probe);
        let cands = reg.transcribe_candidates(&routing, Task::Transcribe);
        assert_eq!(cands.len(), 1);
        assert!(!reg.list().contains(&"whisper-cpp"));
    }

    #[test]
    fn local_only_mode_when_no_cloud_provider() {
        // 无云端 provider 但 sidecar 可用 → whisper-cpp 当 primary。
        let cfg = Config::default();
        let (reg, routing, _) = cfg.build_with_probe(&probe_with_sidecar(None));
        let cands = reg.transcribe_candidates(&routing, Task::Transcribe);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].0.id(), "whisper-cpp");
        // 无已下载模型 → 路由 model 占位 base。
        assert_eq!(cands[0].1, "base");
    }

    #[test]
    fn user_configured_whisper_cpp_wins_over_auto() {
        // 用户显式声明 whisper_cpp（model=medium）→ 只注册一个实例，
        // 且不触发自动注册的模型选择（保持用户的 medium）。
        let cfg: Config = serde_json::from_str(
            r#"{
            "providers": [
                {"type":"openai","id":"openai","base_url":"https://api.openai.com/v1",
                 "api_key":"sk-test","chat_model":"gpt-4o-mini","embed_model":"e",
                 "transcribe_model":"whisper-1"},
                {"type":"whisper_cpp","model":"medium"}
            ],
            "routing": {},
            "guard": {"cloud_allowed": true, "sensitive_patterns": []}
        }"#,
        )
        .unwrap();
        // probe 声称已下载 base：自动注册会选 base，但用户显式 medium 必须胜出。
        let (reg, routing, _) = cfg.build_with_probe(&probe_with_sidecar(Some("base")));
        let cands = reg.transcribe_candidates(&routing, Task::Transcribe);
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[1].1, "medium", "explicit user model must win");
    }

    #[test]
    fn resolve_uses_preferred_downloaded_model_when_user_silent() {
        // C2 回归：自动注册选中已下载的 small，而不是硬编码 base。
        let p = resolve_whisper_cpp(None, None, None, 4, &probe_with_sidecar(Some("small")));
        let provider = p.expect("sidecar available should resolve");
        assert!(provider
            .model_path()
            .to_string_lossy()
            .ends_with("ggml-small.bin"));
    }

    #[test]
    fn resolve_without_sidecar_is_none() {
        let probe = SidecarProbe {
            whisper: None,
            ffmpeg: None,
            preferred_model: Some("small".into()),
        };
        assert!(resolve_whisper_cpp(None, None, None, 4, &probe).is_none());
    }

    #[test]
    fn resolve_explicit_binary_path_bypasses_probe() {
        // 用户显式 binary_path 时，即使 probe 探测不到也能注册。
        let probe = SidecarProbe {
            whisper: None,
            ffmpeg: None,
            preferred_model: None,
        };
        let p = resolve_whisper_cpp(Some("medium"), Some("/custom/whisper"), None, 2, &probe);
        let provider = p.expect("explicit binary path should resolve");
        assert!(provider
            .model_path()
            .to_string_lossy()
            .ends_with("ggml-medium.bin"));
    }
    // ── 多 Vault（FR-STORE-01，v0.4）────────────────────────────────────

    #[test]
    fn legacy_config_without_vault_fields_parses() {
        // 旧 config.json（v0.3 前无 vaults/last_vault）应能解析且 vaults 回退默认单库。
        let json = r#"{
            "providers": [{"type":"ollama","base_url":"http://localhost:11434","chat_model":"m","embed_model":"e"}],
            "routing": {},
            "guard": {"cloud_allowed": false, "sensitive_patterns": []}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.vaults.len(), 1);
        assert!(cfg.vaults[0]
            .replace('\\', "/")
            .ends_with(".lmnotes/default"));
        assert!(cfg.last_vault.is_none());
    }

    #[test]
    fn resolve_vault_falls_back_when_missing_or_invalid() {
        let default = std::path::Path::new("/home/u/.lmnotes/default");
        // None → 默认
        assert_eq!(resolve_vault(None, default), default);
        // 指向不存在目录 → 默认
        assert_eq!(resolve_vault(Some("/nonexistent/vault"), default), default);
        // 空串 → 默认
        assert_eq!(resolve_vault(Some("  "), default), default);
    }

    #[test]
    fn resolve_vault_uses_valid_dir() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().to_str().unwrap().to_string();
        assert_eq!(
            resolve_vault(Some(&p), std::path::Path::new("/default")),
            std::path::PathBuf::from(&p)
        );
    }

    #[test]
    fn capture_hotkey_default_and_legacy_parse() {
        // v0.8：默认 CmdOrCtrl+Shift+L；旧 config（无 capture 段）解析取默认；
        // 自定义值 round-trip 保留。
        let legacy = serde_yaml::from_str::<Config>(
            "providers: []\nrouting: {}\nguard:\n  cloud_allowed: false\n  sensitive_patterns: []\n",
        )
        .unwrap();
        assert_eq!(legacy.capture.hotkey, "CmdOrCtrl+Shift+L");
        let custom = serde_yaml::from_str::<Config>(
            "providers: []\nrouting: {}\nguard:\n  cloud_allowed: false\n  sensitive_patterns: []\ncapture:\n  hotkey: Ctrl+Shift+K\n",
        )
        .unwrap();
        assert_eq!(custom.capture.hotkey, "Ctrl+Shift+K");
    }
}
