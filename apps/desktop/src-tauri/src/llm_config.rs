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
            },
            guard: GuardConfigSer::default(),
            mcp: McpConfig::default(),
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
        use lmnotes_core::llm::ollama::OllamaProvider;
        use lmnotes_core::llm::openai::OpenAiProvider;
        use lmnotes_core::llm::whisper::WhisperProvider;
        let mut reg = lmnotes_core::llm::routing::Registry::new();
        let mut user_configured_whisper_cpp = false;
        for p in &self.providers {
            match p {
                ProviderConfig::Ollama { base_url, .. } => {
                    let ollama = std::sync::Arc::new(OllamaProvider::new(base_url));
                    reg.register_chat_arc(ollama.clone());
                    reg.register_embed_arc(ollama);
                }
                ProviderConfig::OpenAi {
                    id,
                    base_url,
                    api_key,
                    transcribe_model: _,
                    ..
                } => {
                    let openai = std::sync::Arc::new(OpenAiProvider::new(id, base_url, api_key));
                    reg.register_chat_arc(openai.clone());
                    reg.register_embed_arc(openai);
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
                    ) {
                        user_configured_whisper_cpp = true;
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
                let whisper = WhisperProvider::new(id, base_url, api_key);
                reg.register_transcribe(whisper);
            }
        }
        // 自动注册 whisper.cpp 作为本地降级 provider（ADR-0007 §决策"开箱可用"）：
        // 即使用户 config.json 未声明 WhisperCpp，只要 sidecar 可探测到就注册，
        // 使其天然成为云端 transcribe 的 fallback。
        let auto_whisper_cpp = if !user_configured_whisper_cpp {
            register_whisper_cpp(
                &mut reg,
                Some("base"),
                None,
                None,
                default_whisper_threads(),
            )
        } else {
            false
        };
        let whisper_cpp_registered = user_configured_whisper_cpp || auto_whisper_cpp;
        let routing = self.build_routing(whisper_cpp_registered);
        let guard = GuardConfig {
            cloud_allowed: self.guard.cloud_allowed,
            sensitive_patterns: self.guard.sensitive_patterns.clone(),
        };
        (reg, routing, guard)
    }

    fn build_routing(&self, whisper_cpp_registered: bool) -> Routing {
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
        let local_fb = if whisper_cpp_registered {
            vec![ProviderRef {
                provider_id: "whisper-cpp".into(),
                model: "base".into(),
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
                        model: "base".into(),
                    },
                    vec![],
                ),
            );
        }
        Routing { map }
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

/// 注册 whisper.cpp 本地 provider 到 Registry。返回是否成功注册（sidecar+模型可达）。
/// 失败（binary/模型不可达）静默跳过——本地 STT 在运行时降级时由前端引导下载。
/// 路径优先级：用户显式 binary_path > commands::resolve_sidecar > 放弃。
fn register_whisper_cpp(
    reg: &mut lmnotes_core::llm::routing::Registry,
    model: Option<&str>,
    binary_path: Option<&str>,
    ffmpeg_path: Option<&str>,
    threads: usize,
) -> bool {
    use crate::commands::{ffmpeg_binary_path, models_dir, whisper_binary_path};
    use crate::whisper_cpp::WhisperCppProvider;
    let model_name = model.unwrap_or("base");
    // binary：显式 > 自动探测
    let binary = if let Some(p) = binary_path {
        Some(std::path::PathBuf::from(p))
    } else {
        whisper_binary_path()
    };
    let Some(binary) = binary else {
        return false; // sidecar 不可达
    };
    // ffmpeg：显式 > 自动探测（None 允许——仅 WAV 直通场景）
    let ffmpeg = ffmpeg_path
        .map(std::path::PathBuf::from)
        .or_else(ffmpeg_binary_path);
    // 模型：~/.lmnotes/models/ggml-<name>.bin
    let model_p = models_dir().join(format!("ggml-{model_name}.bin"));
    let provider = WhisperCppProvider::new("whisper-cpp", binary, ffmpeg, model_p, threads);
    reg.register_transcribe(provider);
    true
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
}
