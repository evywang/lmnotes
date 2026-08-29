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
        const TRANSCRIBE = 1 << 2;
        const VISION = 1 << 3;
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

/// 音频转录输入：bytes + mime + 文件名（multipart 上传用）。
#[derive(Debug, Clone)]
pub struct AudioInput {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub filename: String,
}

/// 转录结果。
#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
}

/// 转录能力 trait（ADR-0005 §1）。Whisper 兼容端点按需实现。
#[async_trait]
pub trait TranscribeCap: LlmProvider {
    /// 转录音频。`language` 为 ISO-639-1（如 "zh"/"en"），None 表示自动检测。
    async fn transcribe(
        &self,
        audio: AudioInput,
        model: &str,
        language: Option<&str>,
    ) -> Result<Transcript>;
}

/// 视觉描述输入（单图，OCR 由通用视觉模型顺带覆盖——不做专用管线）。
#[derive(Debug, Clone)]
pub struct ImageInput {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// 视觉描述能力 trait（ADR-0005 §1 预留位，v0.4 FR-MEDIA-02 落地）。
#[async_trait]
pub trait VisionCap: LlmProvider {
    /// 描述图片。`prompt` 为 None 时用实现方默认提示词。
    async fn describe(
        &self,
        image: ImageInput,
        model: &str,
        prompt: Option<&str>,
    ) -> Result<String>;
}

/// 视觉描述默认提示词（含图中文字转写，覆盖 OCR 场景）。
pub const DEFAULT_VISION_PROMPT: &str = "用中文详细描述这张图片的内容；若图中含有文字，先逐字转写（保留原文语言），再描述版面与图表结构。";
