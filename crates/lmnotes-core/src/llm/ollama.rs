//! Ollama 本地 Provider（默认 http://localhost:11434）。
//!
//! 流式 chat：POST /api/chat（stream=true，NDJSON，每行 {"message":{"content":"..."}}，
//! 遇 {"done":true} 结束）。embed：POST /api/embeddings。

use super::provider::*;
use crate::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
        }
    }
    pub fn default_local() -> Self {
        Self::new("http://localhost:11434")
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::CHAT | Capabilities::EMBED
    }
    async fn health(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        Ok(self
            .client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false))
    }
}

#[derive(Serialize)]
struct ChatBody {
    model: String,
    messages: Vec<MsgSer>,
    stream: bool,
}
#[derive(Serialize)]
struct MsgSer {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct ChatChunk {
    message: Option<MsgDe>,
    done: Option<bool>,
}
#[derive(Deserialize)]
struct MsgDe {
    content: String,
}

#[async_trait]
impl ChatCap for OllamaProvider {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn futures_util::Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatBody {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| MsgSer {
                    role: match m.role {
                        ChatRole::System => "system".into(),
                        ChatRole::User => "user".into(),
                        ChatRole::Assistant => "assistant".into(),
                    },
                    content: m.content,
                })
                .collect(),
            stream: true,
        };
        let resp = self.client.post(&url).json(&body).send().await?;
        let byte_stream = resp.bytes_stream();
        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut bytes, mut buf)| async move {
                loop {
                    if let Some(idx) = buf.find('\n') {
                        let line: String = buf.drain(..=idx).collect();
                        if let Ok(c) = serde_json::from_str::<ChatChunk>(line.trim()) {
                            if let Some(m) = c.message {
                                return Some((Ok(m.content), (bytes, buf)));
                            }
                            if c.done.unwrap_or(false) {
                                return None;
                            }
                        }
                        continue;
                    }
                    match bytes.next().await {
                        Some(Ok(chunk)) => buf.push_str(&String::from_utf8_lossy(&chunk)),
                        Some(Err(e)) => {
                            return Some((Err(crate::CoreError::Http(e)), (bytes, buf)))
                        }
                        None => {
                            if buf.trim().is_empty() {
                                return None;
                            }
                            let line = std::mem::take(&mut buf);
                            if let Ok(c) = serde_json::from_str::<ChatChunk>(line.trim()) {
                                if let Some(m) = c.message {
                                    return Some((Ok(m.content), (bytes, buf)));
                                }
                            }
                            return None;
                        }
                    }
                }
            },
        )
        .boxed();
        Ok(Box::new(stream))
    }
}

#[derive(Serialize)]
struct EmbedBody {
    model: String,
    prompt: String,
}
#[derive(Deserialize)]
struct EmbedResp {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbedCap for OllamaProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embeddings", self.base_url);
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            let r: EmbedResp = self
                .client
                .post(&url)
                .json(&EmbedBody {
                    model: model.into(),
                    prompt: t.clone(),
                })
                .send()
                .await?
                .json()
                .await?;
            out.push(r.embedding);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_local_with_both_caps() {
        let p = OllamaProvider::default_local();
        assert_eq!(p.kind(), ProviderKind::Local);
        assert!(p
            .capabilities()
            .contains(Capabilities::CHAT | Capabilities::EMBED));
        assert_eq!(p.id(), "ollama");
    }

    #[tokio::test]
    async fn chat_stream_parses_ndjson() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "{\"message\":{\"content\":\"Hello\"},\"done\":false}\n\
                     {\"message\":{\"content\":\" world\"},\"done\":false}\n\
                     {\"message\":{},\"done\":true}\n",
            ))
            .mount(&server)
            .await;
        let p = OllamaProvider::new(server.uri());
        let req = ChatRequest {
            model: "x".into(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "hi".into(),
            }],
            temperature: None,
        };
        let mut s = p.chat_stream(req).await.unwrap();
        let mut out = String::new();
        while let Some(chunk) = s.next().await {
            out.push_str(&chunk.unwrap());
        }
        assert_eq!(out, "Hello world");
    }
}
