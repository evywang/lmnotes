//! OpenAI 兼容 Provider（GLM/OpenAI/自建）。
//!
//! 流式 chat：POST /v1/chat/completions（stream=true，SSE `data: {...}` 行，
//! 遇 `data: [DONE]` 结束）。embed：POST /v1/embeddings。Authorization Bearer。

use super::provider::*;
use crate::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAiProvider {
    id: String,
    base_url: String,
    api_key: String,
    client: Client,
}

impl OpenAiProvider {
    /// id 用于 Registry 区分多个 OpenAI 兼容端点（如 "glm"、"openai"）。
    pub fn new(id: impl Into<String>, base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Cloud
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::CHAT | Capabilities::EMBED
    }
    async fn health(&self) -> Result<bool> {
        let url = format!("{}/v1/models", self.base_url);
        let r = self.client.get(&url).bearer_auth(&self.api_key).send().await;
        Ok(r.map(|x| x.status().is_success()).unwrap_or(false))
    }
}

#[derive(Serialize)]
struct ChatBody {
    model: String,
    messages: Vec<MsgSer>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}
#[derive(Serialize)]
struct MsgSer {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct ChatChunk {
    choices: Vec<ChoiceDe>,
}
#[derive(Deserialize)]
struct ChoiceDe {
    delta: DeltaDe,
}
#[derive(Deserialize)]
struct DeltaDe {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl ChatCap for OpenAiProvider {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn futures_util::Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/v1/chat/completions", self.base_url);
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
            temperature: req.temperature,
        };
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        let byte_stream = resp.bytes_stream();
        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut bytes, mut buf)| async move {
                loop {
                    if let Some(idx) = buf.find('\n') {
                        let line: String = buf.drain(..=idx).collect();
                        let line = line.trim();
                        if line == "data: [DONE]" {
                            return None;
                        }
                        if let Some(json) = line.strip_prefix("data: ") {
                            if let Ok(c) = serde_json::from_str::<ChatChunk>(json) {
                                if let Some(content) = c
                                    .choices
                                    .into_iter()
                                    .next()
                                    .and_then(|ch| ch.delta.content)
                                {
                                    return Some((Ok(content), (bytes, buf)));
                                }
                            }
                        }
                        continue;
                    }
                    match bytes.next().await {
                        Some(Ok(chunk)) => buf.push_str(&String::from_utf8_lossy(&chunk)),
                        Some(Err(e)) => return Some((Err(crate::CoreError::Http(e)), (bytes, buf))),
                        None => return None,
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
    input: Vec<String>,
}
#[derive(Deserialize)]
struct EmbedResp {
    data: Vec<EmbedItem>,
}
#[derive(Deserialize)]
struct EmbedItem {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbedCap for OpenAiProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/v1/embeddings", self.base_url);
        let r: EmbedResp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&EmbedBody {
                model: model.into(),
                input: texts.to_vec(),
            })
            .send()
            .await?
            .json()
            .await?;
        Ok(r.data.into_iter().map(|i| i.embedding).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn chat_stream_parses_sse() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\n\
                     data: [DONE]\n\n",
                ),
            )
            .mount(&server)
            .await;
        let p = OpenAiProvider::new("test", server.uri(), "test-key");
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
        while let Some(c) = s.next().await {
            out.push_str(&c.unwrap());
        }
        assert_eq!(out, "Hi there");
    }

    #[test]
    fn is_cloud_kind() {
        let p = OpenAiProvider::new("glm", "https://open.bigmodel.cn/api", "k");
        assert_eq!(p.kind(), ProviderKind::Cloud);
        assert_eq!(p.id(), "glm");
    }
}
