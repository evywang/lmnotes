//! LLM Provider 抽象（ADR-0005）。按能力拆 trait（F7）。

use crate::Result;
use async_trait::async_trait;
use futures_util::Stream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Local,
    Cloud,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Capabilities: u8 {
        const CHAT = 1 << 0;
        const EMBED = 1 << 1;
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
}

/// 所有 Provider 必须实现。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> Capabilities;
    async fn health(&self) -> Result<bool>;
}

/// chat 能力 trait（按需实现）。
#[async_trait]
pub trait ChatCap: LlmProvider {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>>;

    /// 非流式（聚合 stream）。
    async fn chat(&self, req: ChatRequest) -> Result<String> {
        let mut stream = self.chat_stream(req).await?;
        let mut out = String::new();
        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            out.push_str(&chunk?);
        }
        Ok(out)
    }
}

/// embed 能力 trait。
#[async_trait]
pub trait EmbedCap: LlmProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}
