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
    },
    #[serde(rename = "openai")]
    OpenAi {
        id: String,
        base_url: String,
        api_key: String,
        chat_model: String,
        embed_model: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub summarize: Option<ProviderRefSer>,
    pub link_suggest: Option<ProviderRefSer>,
    pub embed: Option<ProviderRefSer>,
    pub chat: Option<ProviderRefSer>,
    pub rewrite: Option<ProviderRefSer>,
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
            },
            guard: GuardConfigSer::default(),
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

    /// 映射到核心层的 Registry + Routing + GuardConfig。
    pub fn build(&self) -> (lmnotes_core::llm::routing::Registry, Routing, GuardConfig) {
        use lmnotes_core::llm::ollama::OllamaProvider;
        use lmnotes_core::llm::openai::OpenAiProvider;
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
                    ..
                } => {
                    let openai = std::sync::Arc::new(OpenAiProvider::new(id, base_url, api_key));
                    reg.register_chat_arc(openai.clone());
                    reg.register_embed_arc(openai);
                }
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
        Routing { map }
    }
}
