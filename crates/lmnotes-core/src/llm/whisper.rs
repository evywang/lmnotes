//! Whisper 兼容 Provider（OpenAI / Groq / GLM 等兼容端点）。
//!
//! 转录：POST /audio/transcriptions（multipart/form-data，`file` 携带音频 bytes，
//! `model`/`language`/`response_format=text` 为表单字段）。OpenAI 在 response_format=text
//! 时返回裸文本 body；少数兼容实现可能返回 JSON `{"text": "..."}`，两种均尝试解析。

use super::provider::*;
use crate::Result;
use async_trait::async_trait;
use reqwest::multipart;
use reqwest::Client;
use serde::Deserialize;

pub struct WhisperProvider {
    id: String,
    base_url: String,
    api_key: String,
    client: Client,
}

impl WhisperProvider {
    /// id 用于 Registry 区分多个 Whisper 兼容端点（如 "openai-whisper"、"groq"）。
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for WhisperProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Cloud
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::TRANSCRIBE
    }
    async fn health(&self) -> Result<bool> {
        // 复用 OpenAI 约定：GET /models 探活
        let url = format!("{}/models", self.base_url);
        let r = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await;
        Ok(r.map(|x| x.status().is_success()).unwrap_or(false))
    }
}

/// 部分 JSON 兼容实现可能返回 `{"text": "..."}`。
#[derive(Deserialize)]
struct JsonTranscript {
    text: String,
}

#[async_trait]
impl TranscribeCap for WhisperProvider {
    async fn transcribe(
        &self,
        audio: AudioInput,
        model: &str,
        language: Option<&str>,
    ) -> Result<Transcript> {
        let url = format!("{}/audio/transcriptions", self.base_url);

        // multipart：file = 音频 bytes（带 mime 与 filename），其余为文本字段
        let part = multipart::Part::bytes(audio.bytes)
            .file_name(audio.filename)
            .mime_str(&audio.mime)
            .map_err(|e| crate::CoreError::Conformance(format!("invalid mime: {e}")))?;
        let mut form = multipart::Form::new()
            .text("model", model.to_string())
            .text("response_format", "text".to_string())
            .part("file", part);
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            // 用 TranscribeHttp 携带状态码，供 classify_transcribe_error 判断
            // 5xx 降级 / 4xx 不降级（ADR-0007）。原 Conformance 会丢失 status。
            return Err(crate::CoreError::TranscribeHttp {
                status: status.as_u16(),
                body: body.chars().take(500).collect(),
            });
        }

        // response_format=text → 裸文本；但兼容端点可能返回 JSON，尝试两种解析。
        let trimmed = body.trim();
        let text = if let Some(rest) = trimmed
            .strip_prefix('{')
            .and_then(|_| serde_json::from_str::<JsonTranscript>(trimmed).ok())
        {
            rest.text
        } else {
            body
        };
        Ok(Transcript { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn transcribe_parses_plain_text_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string("你好世界"))
            .mount(&server)
            .await;
        let p = WhisperProvider::new("test", server.uri(), "test-key");
        let t = p
            .transcribe(
                AudioInput {
                    bytes: vec![0x1A, 0x45, 0xDF, 0xA3],
                    mime: "audio/webm".into(),
                    filename: "rec.webm".into(),
                },
                "whisper-1",
                Some("zh"),
            )
            .await
            .unwrap();
        assert_eq!(t.text, "你好世界");
    }

    #[tokio::test]
    async fn transcribe_parses_json_fallback() {
        // 某些兼容端点忽略 response_format=text，仍返回 JSON
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"text":"hello world"}"#))
            .mount(&server)
            .await;
        let p = WhisperProvider::new("test", server.uri(), "k");
        let t = p
            .transcribe(
                AudioInput {
                    bytes: vec![0],
                    mime: "audio/webm".into(),
                    filename: "x.webm".into(),
                },
                "whisper-1",
                None,
            )
            .await
            .unwrap();
        assert_eq!(t.text, "hello world");
    }

    #[tokio::test]
    async fn transcribe_surfaces_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;
        let p = WhisperProvider::new("test", server.uri(), "bad-key");
        let res = p
            .transcribe(
                AudioInput {
                    bytes: vec![0],
                    mime: "audio/webm".into(),
                    filename: "x.webm".into(),
                },
                "whisper-1",
                None,
            )
            .await;
        assert!(res.is_err());
        // 关键：非 2xx 应包成 TranscribeHttp（带 status），而非 Conformance——
        // 否则 classify_transcribe_error 无法识别 5xx 触发降级（ADR-0007）。
        let err = res.unwrap_err();
        match err {
            crate::CoreError::TranscribeHttp { status, .. } => assert_eq!(status, 401),
            other => panic!("expected TranscribeHttp, got {other:?}"),
        }
    }

    #[test]
    fn is_cloud_kind() {
        let p = WhisperProvider::new("groq", "https://api.groq.com/openai/v1", "k");
        assert_eq!(p.kind(), ProviderKind::Cloud);
        assert_eq!(p.capabilities(), Capabilities::TRANSCRIBE);
        assert_eq!(p.id(), "groq");
    }
}
