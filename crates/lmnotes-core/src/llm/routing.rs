//! 任务→Provider 路由（ADR-0005 §3）。双 map 方案（F7）。

use super::provider::*;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Task {
    Summarize,
    LinkSuggest,
    Embed,
    Chat,
    Rewrite,
}

#[derive(Debug, Clone)]
pub struct ProviderRef {
    pub provider_id: String,
    pub model: String,
}

/// 路由：每个任务一个首选 + 备选（降级用）。
#[derive(Debug, Clone, Default)]
pub struct Routing {
    /// 任务 → (首选, [备选...])
    pub map: HashMap<Task, (ProviderRef, Vec<ProviderRef>)>,
}

pub struct Registry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    chats: HashMap<String, Arc<dyn ChatCap>>,
    embeds: HashMap<String, Arc<dyn EmbedCap>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            chats: HashMap::new(),
            embeds: HashMap::new(),
        }
    }

    /// 注册一个 chat provider。
    pub fn register_chat<P>(&mut self, p: P)
    where
        P: LlmProvider + ChatCap + 'static,
    {
        let arc = Arc::new(p);
        self.register_chat_arc(arc);
    }

    /// 注册一个已有 Arc 的 chat provider（用于同一实例同时注册 chat+embed，评审 R8）。
    pub fn register_chat_arc<P>(&mut self, arc: Arc<P>)
    where
        P: LlmProvider + ChatCap + 'static,
    {
        let id = arc.id().to_string();
        self.chats.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 注册一个 embed provider。
    pub fn register_embed<P>(&mut self, p: P)
    where
        P: LlmProvider + EmbedCap + 'static,
    {
        let arc = Arc::new(p);
        self.register_embed_arc(arc);
    }

    /// 注册一个已有 Arc 的 embed provider（同 register_chat_arc 用途）。
    pub fn register_embed_arc<P>(&mut self, arc: Arc<P>)
    where
        P: LlmProvider + EmbedCap + 'static,
    {
        let id = arc.id().to_string();
        self.embeds.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 按任务取 chat provider（首选 → 降级备选）。返回 (provider_arc, model)。
    pub fn chat_for(&self, routing: &Routing, task: Task) -> Result<(Arc<dyn ChatCap>, String)> {
        let (primary, fallbacks) = routing.map.get(&task).ok_or_else(|| {
            crate::CoreError::Conformance(format!("no routing for task {task:?}"))
        })?;
        for pref in std::iter::once(primary).chain(fallbacks.iter()) {
            if let Some(p) = self.chats.get(&pref.provider_id) {
                return Ok((p.clone(), pref.model.clone()));
            }
        }
        Err(crate::CoreError::Conformance(format!(
            "no registered chat provider for task {task:?} (tried {} + {} fallbacks)",
            primary.provider_id,
            fallbacks.len()
        )))
    }

    /// 按任务取 embed provider。
    pub fn embed_for(
        &self,
        routing: &Routing,
        task: Task,
    ) -> Result<(Arc<dyn EmbedCap>, String)> {
        let (primary, fallbacks) = routing.map.get(&task).ok_or_else(|| {
            crate::CoreError::Conformance(format!("no routing for task {task:?}"))
        })?;
        for pref in std::iter::once(primary).chain(fallbacks.iter()) {
            if let Some(p) = self.embeds.get(&pref.provider_id) {
                return Ok((p.clone(), pref.model.clone()));
            }
        }
        Err(crate::CoreError::Conformance(format!(
            "no embed provider for task {task:?}"
        )))
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn list(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::Stream;

    struct FakeChat;
    #[async_trait]
    impl LlmProvider for FakeChat {
        fn id(&self) -> &str {
            "fake"
        }
        fn kind(&self) -> ProviderKind {
            ProviderKind::Local
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::CHAT
        }
        async fn health(&self) -> Result<bool> {
            Ok(true)
        }
    }
    #[async_trait]
    impl ChatCap for FakeChat {
        async fn chat_stream(
            &self,
            _: ChatRequest,
        ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
            Ok(Box::new(futures_util::stream::iter(vec![Ok("hi".into())])))
        }
    }

    fn routing(task: Task, primary: &str, fb: &[&str]) -> Routing {
        let mut map = HashMap::new();
        let primary_ref = ProviderRef {
            provider_id: primary.into(),
            model: "m".into(),
        };
        let fbs: Vec<ProviderRef> = fb
            .iter()
            .map(|f| ProviderRef {
                provider_id: f.to_string(),
                model: "m".into(),
            })
            .collect();
        map.insert(task, (primary_ref, fbs));
        Routing { map }
    }

    #[test]
    fn resolves_primary_chat() {
        let mut reg = Registry::new();
        reg.register_chat(FakeChat);
        let r = routing(Task::Summarize, "fake", &[]);
        let (p, _) = reg.chat_for(&r, Task::Summarize).unwrap();
        assert_eq!(p.id(), "fake");
    }

    #[test]
    fn fallback_when_primary_missing() {
        let mut reg = Registry::new();
        reg.register_chat(FakeChat);
        let r = routing(Task::Summarize, "absent", &["fake"]);
        let (p, _) = reg.chat_for(&r, Task::Summarize).unwrap();
        assert_eq!(p.id(), "fake");
    }

    #[test]
    fn errors_when_all_missing() {
        let reg = Registry::new();
        let r = routing(Task::Chat, "absent", &["also-absent"]);
        assert!(reg.chat_for(&r, Task::Chat).is_err());
    }
}
