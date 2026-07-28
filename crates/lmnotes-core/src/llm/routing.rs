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
    Transcribe,
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

#[derive(Default)]
pub struct Registry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    chats: HashMap<String, Arc<dyn ChatCap>>,
    embeds: HashMap<String, Arc<dyn EmbedCap>>,
    transcribes: HashMap<String, Arc<dyn TranscribeCap>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            chats: HashMap::new(),
            embeds: HashMap::new(),
            transcribes: HashMap::new(),
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
    pub fn embed_for(&self, routing: &Routing, task: Task) -> Result<(Arc<dyn EmbedCap>, String)> {
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

    /// 注册一个 transcribe provider。
    pub fn register_transcribe<P>(&mut self, p: P)
    where
        P: LlmProvider + TranscribeCap + 'static,
    {
        let arc = Arc::new(p);
        self.register_transcribe_arc(arc);
    }

    /// 注册一个已有 Arc 的 transcribe provider（用于同一实例同时注册多种能力）。
    pub fn register_transcribe_arc<P>(&mut self, arc: Arc<P>)
    where
        P: LlmProvider + TranscribeCap + 'static,
    {
        let id = arc.id().to_string();
        self.transcribes.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 按任务取 transcribe provider（首选 → 降级备选）。返回 (provider_arc, model)。
    pub fn transcribe_for(
        &self,
        routing: &Routing,
        task: Task,
    ) -> Result<(Arc<dyn TranscribeCap>, String)> {
        let (primary, fallbacks) = routing.map.get(&task).ok_or_else(|| {
            crate::CoreError::Conformance(format!("no routing for task {task:?}"))
        })?;
        for pref in std::iter::once(primary).chain(fallbacks.iter()) {
            if let Some(p) = self.transcribes.get(&pref.provider_id) {
                return Ok((p.clone(), pref.model.clone()));
            }
        }
        Err(crate::CoreError::Conformance(format!(
            "no transcribe provider for task {task:?} (tried {} + {} fallbacks)",
            primary.provider_id,
            fallbacks.len()
        )))
    }

    /// 取 transcribe 任务的全部候选 provider（首选 + 备选，仅返回已注册的）。
    /// 供运行时降级用：调用方按序试，云端网络错误时切下一个（ADR-0007）。
    /// 返回有序 Vec<(provider_arc, model)>，可能为空。
    pub fn transcribe_candidates(
        &self,
        routing: &Routing,
        task: Task,
    ) -> Vec<(Arc<dyn TranscribeCap>, String)> {
        let Some((primary, fallbacks)) = routing.map.get(&task) else {
            return Vec::new();
        };
        std::iter::once(primary)
            .chain(fallbacks.iter())
            .filter_map(|pref| {
                self.transcribes
                    .get(&pref.provider_id)
                    .map(|p| (p.clone(), pref.model.clone()))
            })
            .collect()
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

    // ── Transcribe 能力（T1）──────────────────────────────────────────────
    use super::super::provider::{AudioInput, Transcript};

    struct FakeTranscribe;
    #[async_trait]
    impl LlmProvider for FakeTranscribe {
        fn id(&self) -> &str {
            "fake-tr"
        }
        fn kind(&self) -> ProviderKind {
            ProviderKind::Cloud
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::TRANSCRIBE
        }
        async fn health(&self) -> Result<bool> {
            Ok(true)
        }
    }
    #[async_trait]
    impl TranscribeCap for FakeTranscribe {
        async fn transcribe(
            &self,
            _audio: AudioInput,
            _model: &str,
            _language: Option<&str>,
        ) -> Result<Transcript> {
            Ok(Transcript {
                text: "hello".into(),
            })
        }
    }

    #[tokio::test]
    async fn resolves_primary_transcribe() {
        let mut reg = Registry::new();
        reg.register_transcribe(FakeTranscribe);
        let r = routing(Task::Transcribe, "fake-tr", &[]);
        let (p, _) = reg.transcribe_for(&r, Task::Transcribe).unwrap();
        assert_eq!(p.id(), "fake-tr");
        // 验证能力可调用
        let t = p
            .transcribe(
                AudioInput {
                    bytes: vec![0],
                    mime: "audio/webm".into(),
                    filename: "x.webm".into(),
                },
                "m",
                None,
            )
            .await
            .unwrap();
        assert_eq!(t.text, "hello");
    }

    #[test]
    fn fallback_when_primary_missing_transcribe() {
        let mut reg = Registry::new();
        reg.register_transcribe(FakeTranscribe);
        let r = routing(Task::Transcribe, "absent", &["fake-tr"]);
        let (p, _) = reg.transcribe_for(&r, Task::Transcribe).unwrap();
        assert_eq!(p.id(), "fake-tr");
    }

    #[test]
    fn errors_when_all_missing_transcribe() {
        let reg = Registry::new();
        let r = routing(Task::Transcribe, "absent", &["also-absent"]);
        assert!(reg.transcribe_for(&r, Task::Transcribe).is_err());
    }

    // ── transcribe_candidates（运行时降级用，ADR-0007）──────────────────
    #[test]
    fn candidates_returns_ordered_primary_then_fallbacks() {
        // 注册两个 provider；candidates 应按 primary → fallback 顺序返回已注册的。
        let mut reg = Registry::new();
        reg.register_transcribe(FakeTranscribe); // id "fake-tr"
                                                 // 再注册一个 fake 作为 fallback
        struct FakeTranscribe2;
        #[async_trait]
        impl LlmProvider for FakeTranscribe2 {
            fn id(&self) -> &str {
                "fake-tr-2"
            }
            fn kind(&self) -> ProviderKind {
                ProviderKind::Local
            }
            fn capabilities(&self) -> Capabilities {
                Capabilities::TRANSCRIBE
            }
            async fn health(&self) -> Result<bool> {
                Ok(true)
            }
        }
        #[async_trait]
        impl TranscribeCap for FakeTranscribe2 {
            async fn transcribe(
                &self,
                _: AudioInput,
                _: &str,
                _: Option<&str>,
            ) -> Result<Transcript> {
                Ok(Transcript { text: "ok".into() })
            }
        }
        reg.register_transcribe(FakeTranscribe2);
        // primary = fake-tr-2, fallback = [fake-tr] → candidates 顺序应与此一致
        let r = routing(Task::Transcribe, "fake-tr-2", &["fake-tr"]);
        let cands = reg.transcribe_candidates(&r, Task::Transcribe);
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0].0.id(), "fake-tr-2");
        assert_eq!(cands[1].0.id(), "fake-tr");
    }

    #[test]
    fn candidates_skips_unregistered() {
        // primary 未注册、fallback 一个注册一个未注册 → 只返回注册的那个
        let mut reg = Registry::new();
        reg.register_transcribe(FakeTranscribe);
        let r = routing(
            Task::Transcribe,
            "absent-primary",
            &["absent-fb", "fake-tr"],
        );
        let cands = reg.transcribe_candidates(&r, Task::Transcribe);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].0.id(), "fake-tr");
    }

    #[test]
    fn candidates_empty_when_no_routing() {
        let reg = Registry::new();
        let r = Routing::default(); // 无任何路由
        let cands = reg.transcribe_candidates(&r, Task::Transcribe);
        assert!(cands.is_empty());
    }
}
