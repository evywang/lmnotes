//! LLM 用量记录（FR-MODEL-05，v0.8）：包装器 Provider，成功调用后发事件。
//!
//! 单一挂点：`llm_config::build` 注册时用 `Recording*` 包裹 provider，
//! indexer / chat / 转录 / 视觉全部调用自动覆盖，零调用点改造。
//! sink 由壳层注入（非阻塞——内部只做 channel send），事件落 `llm_usage` 表。
//!
//! token 为**估算值**（字符数/4，中英混合的粗略近似），仅用于用量感知，
//! 不用于计费。隐私：只记 provider/kind/本地或云端/估算 token，不记内容。

use std::sync::Arc;

use crate::Result;
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};

use super::provider::{
    AudioInput, Capabilities, ChatCap, ChatRequest, EmbedCap, ImageInput, LlmProvider,
    ProviderKind, Transcript, TranscribeCap, VisionCap,
};

/// 一次成功的 LLM 调用。
#[derive(Debug, Clone)]
pub struct UsageEvent {
    pub provider: String,
    /// "chat" | "embed" | "transcribe" | "vision"
    pub kind: &'static str,
    pub local: bool,
    pub tokens_est: u64,
}

/// 事件接收端（壳层实现：channel send，非阻塞）。
pub type UsageSink = Arc<dyn Fn(UsageEvent) + Send + Sync>;

/// 字符数/4 的粗略 token 估算。
pub fn est_tokens(texts: &[&str]) -> u64 {
    texts.iter().map(|t| t.len() as u64).sum::<u64>() / 4
}

macro_rules! delegate_base {
    ($t:ty, $bound:path) => {
        #[async_trait]
        impl<P: $bound> LlmProvider for $t {
            fn id(&self) -> &str {
                self.inner.id()
            }
            fn kind(&self) -> ProviderKind {
                self.inner.kind()
            }
            fn capabilities(&self) -> Capabilities {
                self.inner.capabilities()
            }
            async fn health(&self) -> Result<bool> {
                self.inner.health().await
            }
        }
    };
}

// ── chat（流式：计数经 CountingStream，结束时发事件）────────────────────

pub struct RecordingChat<P: ChatCap> {
    inner: Arc<P>,
    sink: UsageSink,
}

impl<P: ChatCap> RecordingChat<P> {
    pub fn arc(inner: Arc<P>, sink: UsageSink) -> Arc<Self> {
        Arc::new(Self { inner, sink })
    }
}

delegate_base!(RecordingChat<P>, ChatCap);

#[async_trait]
impl<P: ChatCap + 'static> ChatCap for RecordingChat<P> {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
        let req_chars: u64 = req.messages.iter().map(|m| m.content.len() as u64).sum();
        let stream = self.inner.chat_stream(req).await?;
        Ok(Box::new(CountingStream {
            inner: stream,
            seen: req_chars,
            sink: self.sink.clone(),
            provider: self.inner.id().to_string(),
            local: self.inner.kind() == ProviderKind::Local,
        }))
    }
}

/// 流消费结束时发 chat 用量事件（部分成功——已见字符>0——也记）。
struct CountingStream<S> {
    inner: S,
    seen: u64,
    sink: UsageSink,
    provider: String,
    local: bool,
}

impl<S: Stream<Item = Result<String>> + Unpin> Stream for CountingStream<S> {
    type Item = Result<String>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.inner.poll_next_unpin(cx) {
            std::task::Poll::Ready(Some(Ok(chunk))) => {
                self.seen += chunk.len() as u64;
                std::task::Poll::Ready(Some(Ok(chunk)))
            }
            std::task::Poll::Ready(Some(Err(e))) => {
                self.emit();
                std::task::Poll::Ready(Some(Err(e)))
            }
            std::task::Poll::Ready(None) => {
                self.emit();
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl<S> CountingStream<S> {
    fn emit(&mut self) {
        if self.seen > 0 {
            (self.sink)(UsageEvent {
                provider: std::mem::take(&mut self.provider),
                kind: "chat",
                local: self.local,
                tokens_est: self.seen / 4,
            });
        }
    }
}

// ── embed ────────────────────────────────────────────────────────────────

pub struct RecordingEmbed<P: EmbedCap> {
    inner: Arc<P>,
    sink: UsageSink,
}

impl<P: EmbedCap> RecordingEmbed<P> {
    pub fn arc(inner: Arc<P>, sink: UsageSink) -> Arc<Self> {
        Arc::new(Self { inner, sink })
    }
}

delegate_base!(RecordingEmbed<P>, EmbedCap);

#[async_trait]
impl<P: EmbedCap + 'static> EmbedCap for RecordingEmbed<P> {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let out = self.inner.embed(model, texts).await?;
        (self.sink)(UsageEvent {
            provider: self.inner.id().to_string(),
            kind: "embed",
            local: self.inner.kind() == ProviderKind::Local,
            tokens_est: est_tokens(
                &texts.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            ),
        });
        Ok(out)
    }
}

// ── transcribe ───────────────────────────────────────────────────────────

pub struct RecordingTranscribe<P: TranscribeCap> {
    inner: Arc<P>,
    sink: UsageSink,
}

impl<P: TranscribeCap> RecordingTranscribe<P> {
    pub fn arc(inner: Arc<P>, sink: UsageSink) -> Arc<Self> {
        Arc::new(Self { inner, sink })
    }
}

delegate_base!(RecordingTranscribe<P>, TranscribeCap);

#[async_trait]
impl<P: TranscribeCap + 'static> TranscribeCap for RecordingTranscribe<P> {
    async fn transcribe(
        &self,
        audio: AudioInput,
        model: &str,
        language: Option<&str>,
    ) -> Result<Transcript> {
        let out = self.inner.transcribe(audio, model, language).await?;
        (self.sink)(UsageEvent {
            provider: self.inner.id().to_string(),
            kind: "transcribe",
            local: self.inner.kind() == ProviderKind::Local,
            tokens_est: est_tokens(&[&out.text]),
        });
        Ok(out)
    }
}

// ── vision ───────────────────────────────────────────────────────────────

pub struct RecordingVision<P: VisionCap> {
    inner: Arc<P>,
    sink: UsageSink,
}

impl<P: VisionCap> RecordingVision<P> {
    pub fn arc(inner: Arc<P>, sink: UsageSink) -> Arc<Self> {
        Arc::new(Self { inner, sink })
    }
}

delegate_base!(RecordingVision<P>, VisionCap);

#[async_trait]
impl<P: VisionCap + 'static> VisionCap for RecordingVision<P> {
    async fn describe(&self, image: ImageInput, model: &str, prompt: Option<&str>) -> Result<String> {
        let out = self.inner.describe(image, model, prompt).await?;
        (self.sink)(UsageEvent {
            provider: self.inner.id().to_string(),
            kind: "vision",
            local: self.inner.kind() == ProviderKind::Local,
            tokens_est: est_tokens(&[&out]),
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::provider::{ChatMessage, ChatRole};
    use futures_util::StreamExt;
    use std::sync::Mutex;

    struct FakeChat {
        fail: bool,
    }

    #[async_trait]
    impl LlmProvider for FakeChat {
        fn id(&self) -> &str {
            "fake"
        }
        fn kind(&self) -> ProviderKind {
            ProviderKind::Cloud
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
            _req: ChatRequest,
        ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
            if self.fail {
                return Err(crate::CoreError::Other("boom".into()));
            }
            let chunks: Vec<Result<String>> = vec![Ok("abcd".into()), Ok("efgh".into())];
            Ok(Box::new(futures_util::stream::iter(chunks)))
        }
    }

    fn collector() -> (UsageSink, Arc<Mutex<Vec<UsageEvent>>>) {
        let events: Arc<Mutex<Vec<UsageEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: UsageSink = {
            let events = events.clone();
            Arc::new(move |e| events.lock().unwrap().push(e))
        };
        (sink, events)
    }

    fn req() -> ChatRequest {
        ChatRequest {
            model: "m".into(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "1234".into(),
            }],
            temperature: None,
        }
    }

    #[tokio::test]
    async fn chat_records_tokens_on_success() {
        let (sink, events) = collector();
        let p = RecordingChat::arc(Arc::new(FakeChat { fail: false }), sink);
        let out = p.chat(req()).await.unwrap();
        assert_eq!(out, "abcdefgh");
        let evs = events.lock().unwrap();
        assert_eq!(evs.len(), 1, "{evs:?}");
        assert_eq!(evs[0].kind, "chat");
        assert_eq!(evs[0].provider, "fake");
        assert!(!evs[0].local);
        // req 4 + resp 8 = 12 chars → 3 tokens
        assert_eq!(evs[0].tokens_est, 3);
    }

    #[tokio::test]
    async fn chat_failure_records_nothing() {
        let (sink, events) = collector();
        let p = RecordingChat::arc(Arc::new(FakeChat { fail: true }), sink);
        assert!(p.chat(req()).await.is_err());
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stream_partial_success_still_counts() {
        let (sink, events) = collector();
        // 流中途出错：已消费 chunk 计入
        struct HalfBroken;
        #[async_trait]
        impl LlmProvider for HalfBroken {
            fn id(&self) -> &str {
                "hb"
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
        impl ChatCap for HalfBroken {
            async fn chat_stream(
                &self,
                _req: ChatRequest,
            ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
                Ok(Box::new(futures_util::stream::iter(vec![
                    Ok("12345678".into()),
                    Err(crate::CoreError::Other("mid".into())),
                ])))
            }
        }
        let p = RecordingChat::arc(Arc::new(HalfBroken), sink);
        let mut stream = p.chat_stream(req()).await.unwrap();
        assert!(stream.next().await.unwrap().is_ok());
        assert!(stream.next().await.unwrap().is_err());
        let evs = events.lock().unwrap();
        assert_eq!(evs.len(), 1, "{evs:?}");
        assert!(evs[0].local);
        assert_eq!(evs[0].tokens_est, 3); // (4 req + 8 seen) / 4
    }
}
