//! 转录错误分类与运行时降级（ADR-0007）。
//!
//! 决定云端转录失败时是否应降级到本地 whisper.cpp：
//! - `Network`（连接拒绝/超时/5xx 服务端错误）→ 降级
//! - `Config`（4xx 鉴权/请求错误）→ 不降级，让用户看到配置问题
//! - `Other`（本地错误、解析错误等）→ 不降级

use crate::llm::guard::{check, GuardConfig, GuardDecision};
use crate::llm::provider::{AudioInput, ProviderKind, Transcript};
use crate::llm::routing::{Registry, Routing, Task};
use crate::CoreError;
use crate::Result;

/// 转录错误分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscribeErrorKind {
    /// 网络/服务端不可用（连接拒绝、超时、DNS、HTTP 5xx）→ 应降级到本地。
    Network,
    /// 客户端配置错误（HTTP 4xx：401 鉴权、403 禁止、400 请求格式）→ 不降级。
    Config,
    /// 其他错误（解析失败、IO、子进程失败等）→ 不降级。
    Other,
}

/// 判断云端转录错误应否触发本地降级。
///
/// 设计意图（ADR-0007 §决策5）：
/// - 网络类（云不可达）→ 降级，让本地兜底
/// - 4xx（用户 key/参数错）→ 不降级，避免静默掩盖配置问题
/// - 本地错误 → 不降级（无下一个候选）
pub fn classify_transcribe_error(e: &CoreError) -> TranscribeErrorKind {
    match e {
        // 传输层错误（连接拒绝/超时/DNS）→ 网络
        CoreError::Http(http_err) => {
            if http_err.is_connect() || http_err.is_timeout() {
                return TranscribeErrorKind::Network;
            }
            TranscribeErrorKind::Other
        }
        // 应用层 HTTP 状态码（whisper.rs 检测非 2xx 后包成 TranscribeHttp）
        CoreError::TranscribeHttp { status, .. } => {
            if *status >= 500 {
                TranscribeErrorKind::Network
            } else {
                TranscribeErrorKind::Config // 4xx
            }
        }
        _ => TranscribeErrorKind::Other,
    }
}

/// 运行时转录降级（ADR-0007 核心）：按候选顺序试，云端网络错误自动切本地。
///
/// 纯核心层逻辑（无 Tauri State），便于单测。Tauri 壳层的 `transcribe_with_fallback`
/// 薄封装调它。返回 (Transcript, provider_id)。
///
/// - 候选顺序：`Registry::transcribe_candidates`（primary + fallbacks）。
/// - 每个候选独立过护栏；云端被 cloud_allowed=false 拒时**降级**到下一个（让本地接手）。
/// - 仅 Cloud provider 的 Network 类错误降级；Config（4xx）/Other 不降级。
pub async fn try_transcribe_with_fallback(
    registry: &Registry,
    routing: &Routing,
    guard_cfg: &GuardConfig,
    audio: AudioInput,
    language: Option<&str>,
) -> Result<(Transcript, String)> {
    let candidates = registry.transcribe_candidates(routing, Task::Transcribe);
    if candidates.is_empty() {
        return Err(CoreError::Conformance(
            "no transcribe provider configured".into(),
        ));
    }
    let mut last_err: Option<CoreError> = None;
    for (provider, model) in &candidates {
        // 护栏：音频不可字符串扫描，传空串 + local_only=false。
        match check(guard_cfg, provider.kind(), "", false) {
            GuardDecision::Allow => {}
            GuardDecision::Deny(_) => {
                if provider.kind() == ProviderKind::Cloud {
                    // 云端被拒（cloud_allowed=false）：降级到下一个候选
                    continue;
                }
                // 本地被拒（理论不会，保守跳过）
                continue;
            }
        }
        match provider.transcribe(audio.clone(), model, language).await {
            Ok(t) => return Ok((t, provider.id().to_string())),
            Err(e) => {
                let kind = classify_transcribe_error(&e);
                if provider.kind() == ProviderKind::Cloud && kind == TranscribeErrorKind::Network {
                    last_err = Some(e);
                    continue; // 降级
                }
                // 非网络错误 / 本地错误：不降级，直接返回
                return Err(e);
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| CoreError::Conformance("all transcribe providers failed".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreError;

    // ── 网络错误（应降级）─────────────────────────────────────────────────

    #[test]
    fn classifies_connect_error_as_network() {
        // 真实请求一个无人监听的端口 → reqwest 产生 connect/timeout 类 Http 错误。
        // 这是唯一可靠产生 reqwest::Error 的方式（无需 mock 整个 transport）。
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err: CoreError = rt
            .block_on(async {
                reqwest::get("http://127.0.0.1:1/no-such-port")
                    .await
                    .map_err(CoreError::from)
            })
            .unwrap_err();
        assert_eq!(
            classify_transcribe_error(&err),
            TranscribeErrorKind::Network
        );
    }

    #[test]
    fn classifies_other_error_as_other() {
        // Http 但无 status 且非 connect/timeout（如 decode 错误）→ 不降级。
        let err = CoreError::Other("some parse error".into());
        assert_eq!(classify_transcribe_error(&err), TranscribeErrorKind::Other);
    }

    #[test]
    fn classifies_http_5xx_as_network() {
        // 云端返回 503 → 端点宕 → 应降级。
        // whisper.rs 检测到非 2xx 后用 TranscribeHttp 携带 status，分类器识别 5xx → Network。
        let err = CoreError::TranscribeHttp {
            status: 503,
            body: "service unavailable".into(),
        };
        assert_eq!(
            classify_transcribe_error(&err),
            TranscribeErrorKind::Network
        );
    }

    // ── 配置错误（不降级）─────────────────────────────────────────────────

    #[test]
    fn classifies_http_401_as_config() {
        // 401 鉴权失败 → 用户 key 错，不应静默降级掩盖。
        let err = CoreError::TranscribeHttp {
            status: 401,
            body: "unauthorized".into(),
        };
        assert_eq!(classify_transcribe_error(&err), TranscribeErrorKind::Config);
    }

    #[test]
    fn classifies_http_403_as_config() {
        let err = CoreError::TranscribeHttp {
            status: 403,
            body: "forbidden".into(),
        };
        assert_eq!(classify_transcribe_error(&err), TranscribeErrorKind::Config);
    }

    #[test]
    fn classifies_http_400_as_config() {
        let err = CoreError::TranscribeHttp {
            status: 400,
            body: "bad request".into(),
        };
        assert_eq!(classify_transcribe_error(&err), TranscribeErrorKind::Config);
    }

    // ── 其他错误（不降级）─────────────────────────────────────────────────

    #[test]
    fn classifies_conformance_error_as_other() {
        // 本地 whisper.cpp 子进程失败、YAML 解析等 → 不降级。
        let err = CoreError::Conformance("whisper.cpp exit 1".into());
        assert_eq!(classify_transcribe_error(&err), TranscribeErrorKind::Other);
    }

    #[test]
    fn classifies_io_error_as_other() {
        let err = CoreError::Io(std::io::Error::other("disk full"));
        assert_eq!(classify_transcribe_error(&err), TranscribeErrorKind::Other);
    }

    // ── try_transcribe_with_fallback 行为测试（ADR-0007 核心）──────────────

    use crate::llm::guard::GuardConfig;
    use crate::llm::provider::{
        AudioInput, Capabilities, LlmProvider, ProviderKind, TranscribeCap,
    };
    use crate::llm::routing::{ProviderRef, Registry, Routing, Task};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// 可配置的 fake transcriber：按 kind / 是否失败 / 返回文本 编排。
    struct FakeTranscriber {
        id: &'static str,
        kind: ProviderKind,
        /// 返回的文本（Ok）；None 则返回指定错误。
        text: Option<&'static str>,
        /// 失败时返回的错误。
        err: Option<CoreError>,
        /// 调用计数（验证是否被调用）。
        calls: Arc<AtomicU32>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for FakeTranscriber {
        fn id(&self) -> &str {
            self.id
        }
        fn kind(&self) -> ProviderKind {
            self.kind
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::TRANSCRIBE
        }
        async fn health(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[async_trait::async_trait]
    impl TranscribeCap for FakeTranscriber {
        async fn transcribe(
            &self,
            _audio: AudioInput,
            _model: &str,
            _language: Option<&str>,
        ) -> Result<Transcript> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(t) = self.text {
                Ok(Transcript { text: t.into() })
            } else if let Some(e) = &self.err {
                Err(clone_error(e))
            } else {
                Ok(Transcript {
                    text: "default".into(),
                })
            }
        }
    }

    // CoreError 不 derive Clone，需手动复制我们用到的变体。
    fn clone_error(e: &CoreError) -> CoreError {
        match e {
            CoreError::TranscribeHttp { status, body } => CoreError::TranscribeHttp {
                status: *status,
                body: body.clone(),
            },
            CoreError::Conformance(s) => CoreError::Conformance(s.clone()),
            CoreError::Other(s) => CoreError::Other(s.clone()),
            other => CoreError::Other(format!("{other}")),
        }
    }

    fn routing_with(primary: &str, fallbacks: &[&str]) -> Routing {
        let mut map = std::collections::HashMap::new();
        let fbs: Vec<ProviderRef> = fallbacks
            .iter()
            .map(|f| ProviderRef {
                provider_id: f.to_string(),
                model: "m".into(),
            })
            .collect();
        map.insert(
            Task::Transcribe,
            (
                ProviderRef {
                    provider_id: primary.into(),
                    model: "m".into(),
                },
                fbs,
            ),
        );
        Routing { map }
    }

    fn audio() -> AudioInput {
        AudioInput {
            bytes: vec![0],
            mime: "audio/webm".into(),
            filename: "x.webm".into(),
        }
    }

    #[tokio::test]
    async fn cloud_success_does_not_invoke_local() {
        // 云端成功 → 不调本地。
        let cloud_calls = Arc::new(AtomicU32::new(0));
        let local_calls = Arc::new(AtomicU32::new(0));
        let mut reg = Registry::new();
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "cloud",
            kind: ProviderKind::Cloud,
            text: Some("from cloud"),
            err: None,
            calls: cloud_calls.clone(),
        }));
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "whisper-cpp",
            kind: ProviderKind::Local,
            text: Some("from local"),
            err: None,
            calls: local_calls.clone(),
        }));
        let routing = routing_with("cloud", &["whisper-cpp"]);
        let guard = GuardConfig {
            cloud_allowed: true,
            ..Default::default()
        };
        let (tr, pid) = try_transcribe_with_fallback(&reg, &routing, &guard, audio(), None)
            .await
            .unwrap();
        assert_eq!(tr.text, "from cloud");
        assert_eq!(pid, "cloud");
        assert_eq!(cloud_calls.load(Ordering::SeqCst), 1);
        assert_eq!(local_calls.load(Ordering::SeqCst), 0); // 本地未被调
    }

    #[tokio::test]
    async fn cloud_network_error_falls_back_to_local() {
        // 云端 5xx（Network）→ 降级到本地。
        let local_calls = Arc::new(AtomicU32::new(0));
        let mut reg = Registry::new();
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "cloud",
            kind: ProviderKind::Cloud,
            text: None,
            err: Some(CoreError::TranscribeHttp {
                status: 503,
                body: "down".into(),
            }),
            calls: Arc::new(AtomicU32::new(0)),
        }));
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "whisper-cpp",
            kind: ProviderKind::Local,
            text: Some("from local"),
            err: None,
            calls: local_calls.clone(),
        }));
        let routing = routing_with("cloud", &["whisper-cpp"]);
        let guard = GuardConfig {
            cloud_allowed: true,
            ..Default::default()
        };
        let (tr, pid) = try_transcribe_with_fallback(&reg, &routing, &guard, audio(), None)
            .await
            .unwrap();
        assert_eq!(tr.text, "from local");
        assert_eq!(pid, "whisper-cpp");
        assert_eq!(local_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cloud_401_does_not_fall_back() {
        // 401 是 Config 错（用户 key 问题）→ 不降级，直接报错让用户看到。
        let local_calls = Arc::new(AtomicU32::new(0));
        let mut reg = Registry::new();
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "cloud",
            kind: ProviderKind::Cloud,
            text: None,
            err: Some(CoreError::TranscribeHttp {
                status: 401,
                body: "unauthorized".into(),
            }),
            calls: Arc::new(AtomicU32::new(0)),
        }));
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "whisper-cpp",
            kind: ProviderKind::Local,
            text: Some("from local"),
            err: None,
            calls: local_calls.clone(),
        }));
        let routing = routing_with("cloud", &["whisper-cpp"]);
        let guard = GuardConfig {
            cloud_allowed: true,
            ..Default::default()
        };
        let result = try_transcribe_with_fallback(&reg, &routing, &guard, audio(), None).await;
        assert!(result.is_err(), "401 should NOT fall back");
        assert_eq!(local_calls.load(Ordering::SeqCst), 0); // 本地没被调
    }

    #[tokio::test]
    async fn cloud_blocked_by_guard_falls_back_to_local() {
        // cloud_allowed=false → 云端被护栏拒 → 应降级到本地（而非直接报错）。
        let local_calls = Arc::new(AtomicU32::new(0));
        let mut reg = Registry::new();
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "cloud",
            kind: ProviderKind::Cloud,
            text: Some("from cloud"),
            err: None,
            calls: Arc::new(AtomicU32::new(0)),
        }));
        reg.register_transcribe_arc(Arc::new(FakeTranscriber {
            id: "whisper-cpp",
            kind: ProviderKind::Local,
            text: Some("from local"),
            err: None,
            calls: local_calls.clone(),
        }));
        let routing = routing_with("cloud", &["whisper-cpp"]);
        let guard = GuardConfig::default(); // cloud_allowed=false
        let (tr, pid) = try_transcribe_with_fallback(&reg, &routing, &guard, audio(), None)
            .await
            .unwrap();
        assert_eq!(tr.text, "from local");
        assert_eq!(pid, "whisper-cpp");
        assert_eq!(local_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_candidates_returns_error() {
        let reg = Registry::new();
        let routing = Routing::default();
        let guard = GuardConfig::default();
        let result = try_transcribe_with_fallback(&reg, &routing, &guard, audio(), None).await;
        assert!(result.is_err());
    }
}
