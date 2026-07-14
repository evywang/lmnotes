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
        let routing = self.build_routing();
        let guard = GuardConfig {
            cloud_allowed: self.guard.cloud_allowed,
            sensitive_patterns: self.guard.sensitive_patterns.clone(),
        };
        (reg, routing, guard)
    }

    fn build_routing(&self) -> Routing {
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
        if let Some(r) = &self.routing.transcribe {
            map.insert(Task::Transcribe, to_ref(r));
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
}
