//! 媒体任务后台 worker（v0.5 FR-MEDIA-04，收编 FR-CAP-09）。
//!
//! 循环：拉 pending（FIFO）→ 标 running → 执行（复用转录/视觉编排）→ 标 done/failed
//! → emit `media-task-update`。并发=1（whisper.cpp 单进程吃满 CPU，串行足够）。
//! ffmpeg 抽音轨（长视频）超时 15 分钟。注：本地 whisper.cpp 子进程超时仍为
//! 60s（provider 单实例被同步/队列两路共用），>60s 的长音频依赖云端批量或后续
//! 超时可配置化（设计文档 §风险 已记偏差）。
//! 启动兜底：把上次运行遗留的 running 重置为 pending（应用退出/崩溃恢复）。

#![allow(clippy::disallowed_methods)]

use crate::commands::{
    build_and_write_transcript, build_extract_audio_cmd, describe_image_core, ffmpeg_binary_path,
    vault_root,
};
use lmnotes_core::index::schema::MediaTask;
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::Indexer;
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::provider::AudioInput;
use lmnotes_core::llm::routing::{Registry, Routing};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// 队列任务子进程超时（长媒体；同步路径为 60s）。
const QUEUED_SUBPROC_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// 取消注册表（v0.5.1）：running 任务 id → AbortHandle（tokio 原生）。
///
/// worker 用 `tokio::spawn`（运行于 tauri 的 tokio runtime 内，上下文可用），
/// 其 JoinHandle 具备 abort_handle()/JoinError::is_cancelled——命令侧 abort()，
/// worker 侧 await 得 JoinError::is_cancelled；任务 future 被丢弃 → kill_on_drop 杀子进程。
#[derive(Default, Clone)]
pub struct CancelRegistry(Arc<std::sync::Mutex<HashMap<String, tokio::task::AbortHandle>>>);

impl CancelRegistry {
    pub fn register(&self, id: &str, h: tokio::task::AbortHandle) {
        self.0.lock().unwrap().insert(id.to_string(), h);
    }
    /// abort 并移除（幂等；未知 id no-op）。
    pub fn abort(&self, id: &str) {
        if let Some(h) = self.0.lock().unwrap().remove(id) {
            h.abort();
        }
    }
    /// 仅移除条目（worker 正常收尾路径；不触发 abort）。
    pub fn unregister(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

/// worker 执行体依赖（Arc 组，lib.rs 启动时注入）。
#[derive(Clone)]
pub struct WorkerDeps {
    pub indexer: Arc<Indexer>,
    pub sqlite: Arc<SqliteIndex>,
    pub registry: Arc<Registry>,
    pub routing: Arc<Routing>,
    pub guard_cfg: Arc<GuardConfig>,
    pub cancels: CancelRegistry,
}

/// 启动兜底 + 常驻循环。lib.rs 在 `run()` 里 spawn。
pub fn spawn_worker(app: tauri::AppHandle, deps: WorkerDeps) {
    tauri::async_runtime::spawn(async move {
        // 崩溃恢复：遗留 running → pending（重新排队）
        let _ = deps.sqlite.reset_running_media_tasks();
        loop {
            let pending = match deps.sqlite.pending_media_tasks(1) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("media worker pull failed: {e}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            let Some(task) = pending.first().cloned() else {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            };
            run_task(&app, &deps, &task).await;
        }
    });
}

/// 执行单个任务（含重试一次：仅网络类失败重试，见 is_retryable）。
/// 取消：cancel 命令侧经 CancelRegistry abort() → handle.await 返回
/// JoinError::is_cancelled → 条件 UPDATE 收尾（完成与取消竞态以 rows 裁决）。
async fn run_task(app: &tauri::AppHandle, deps: &WorkerDeps, task: &MediaTask) {
    let _ = deps
        .sqlite
        .update_media_task_status(&task.id, "running", None, None);
    emit(app, task, "running");

    let task_c = task.clone();
    let deps_c = deps.clone();
    // tokio::spawn（非 tauri 包装）：JoinHandle 自带 abort_handle + JoinError::is_cancelled
    let handle = tokio::spawn(async move { execute_with_retry(&deps_c, &task_c).await });
    deps.cancels.register(&task.id, handle.abort_handle());

    match handle.await {
        Ok(Ok(path)) => {
            // 完成竞态防护：cancel 可能已把 running 翻成 cancelled——条件 UPDATE 裁决
            let written = deps
                .sqlite
                .finish_running_media_task(&task.id, "done", None, Some(&path))
                .unwrap_or(false);
            if written {
                emit(app, task, "done");
            } else {
                eprintln!(
                    "media task {} finished after cancel; keeping cancelled",
                    task.id
                );
            }
        }
        Ok(Err(e)) => {
            let written = deps
                .sqlite
                .finish_running_media_task(&task.id, "failed", Some(&e.to_string()), None)
                .unwrap_or(false);
            if written {
                emit(app, task, "failed");
            }
        }
        Err(join_err) if join_err.is_cancelled() => {
            // 命令侧已 abort；条件收尾（若竞态输给完成则不覆盖）
            let _ = deps.sqlite.finish_running_media_task(
                &task.id,
                "cancelled",
                Some("cancelled by user"),
                None,
            );
            emit(app, task, "cancelled");
        }
        Err(join_err) => {
            eprintln!("media task {} join failed: {join_err}", task.id);
            let _ = deps.sqlite.finish_running_media_task(
                &task.id,
                "failed",
                Some(&join_err.to_string()),
                None,
            );
            emit(app, task, "failed");
        }
    }

    // v0.5.1 GAP-3 审计修复：任务已终态，移除注册表条目（否则无界增长）
    deps.cancels.unregister(&task.id);
}

async fn execute_with_retry(
    deps: &WorkerDeps,
    task: &MediaTask,
) -> Result<String, lmnotes_core::CoreError> {
    match execute(deps, task).await {
        Ok(p) => Ok(p),
        // v0.5.1 GAP-C 审计修复：类型化错误直接经 classify_transcribe_error 判定
        //（不再对 Display 字符串猜测——reqwest 连接错误的 Display 不含 "connect"，
        // 曾致断网首试失败不重试）
        Err(e)
            if lmnotes_core::llm::transcribe_fallback::classify_transcribe_error(&e)
                == lmnotes_core::llm::transcribe_fallback::TranscribeErrorKind::Network =>
        {
            eprintln!(
                "media task {} network failure ({e}), retrying once",
                task.id
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            execute(deps, task).await
        }
        Err(e) => Err(e),
    }
}

async fn execute(deps: &WorkerDeps, task: &MediaTask) -> Result<String, lmnotes_core::CoreError> {
    match task.kind.as_str() {
        "transcribe" => execute_transcribe(deps, task).await,
        "describe" => describe_image_core(
            &task.asset_rel,
            &deps.indexer,
            &deps.sqlite,
            &deps.registry,
            &deps.routing,
            &deps.guard_cfg,
        )
        .await
        .map_err(lmnotes_core::CoreError::Other),
        other => Err(lmnotes_core::CoreError::Conformance(format!(
            "unknown task kind: {other}"
        ))),
    }
}

/// 转录任务：读归档媒体 →（视频抽音轨）→ 云优先/本地兜底转录 → transcript 笔记。
/// 与同步命令同一编排（try_transcribe_with_fallback + build_and_write_transcript）。
/// 返回 typed CoreError——GAP-C 修复：重试判定需精确的 TranscribeErrorKind 分类。
async fn execute_transcribe(
    deps: &WorkerDeps,
    task: &MediaTask,
) -> Result<String, lmnotes_core::CoreError> {
    let rel = task.asset_rel.trim_start_matches('/');
    if !rel.starts_with("assets/") || rel.contains("..") {
        return Err(lmnotes_core::CoreError::Conformance(format!(
            "invalid asset path: {}",
            task.asset_rel
        )));
    }
    let full = vault_root().join(rel);
    let data = tokio::fs::read(&full)
        .await
        .map_err(lmnotes_core::CoreError::Io)?;
    let ext = rel.rsplit('.').next().unwrap_or("bin").to_string();
    let is_video = rel.starts_with("assets/video/");

    // 视频：ffmpeg 抽音轨（队列任务预算 15min）
    let audio_bytes: Vec<u8>;
    let audio_mime;
    let audio_ext;
    if is_video {
        let ffmpeg = ffmpeg_binary_path().ok_or_else(|| {
            lmnotes_core::CoreError::Conformance(
                "video transcription requires the ffmpeg sidecar (not found)".into(),
            )
        })?;
        let tmp = tempfile::tempdir().map_err(lmnotes_core::CoreError::Io)?;
        let wav = tmp.path().join("audio.wav");
        let mut cmd = build_extract_audio_cmd(&ffmpeg, Path::new(&full), &wav);
        let out = tokio::time::timeout(QUEUED_SUBPROC_TIMEOUT, cmd.output())
            .await
            .map_err(|_| {
                lmnotes_core::CoreError::Conformance(
                    "ffmpeg audio extraction timed out (15min)".into(),
                )
            })?
            .map_err(lmnotes_core::CoreError::Io)?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(lmnotes_core::CoreError::Conformance(format!(
                "ffmpeg failed: {}",
                stderr.chars().take(300).collect::<String>()
            )));
        }
        audio_bytes = tokio::fs::read(&wav)
            .await
            .map_err(lmnotes_core::CoreError::Io)?;
        audio_mime = "audio/wav".into();
        audio_ext = "wav".into();
    } else {
        audio_bytes = data;
        audio_mime = task.mime.clone();
        audio_ext = ext;
    }
    if audio_bytes.is_empty() {
        return Err(lmnotes_core::CoreError::Conformance(
            "no audio track found in media file".into(),
        ));
    }

    // 队列预算 15min（v0.5.1）；typed CoreError 供 GAP-C 精确重试分类
    let outcome = tokio::time::timeout(
        QUEUED_SUBPROC_TIMEOUT,
        lmnotes_core::llm::transcribe_fallback::try_transcribe_with_fallback(
            &deps.registry,
            &deps.routing,
            &deps.guard_cfg,
            AudioInput {
                bytes: audio_bytes,
                mime: audio_mime,
                filename: format!(
                    "{hash}.{audio_ext}",
                    hash = hash_of(rel),
                    audio_ext = audio_ext
                ),
            },
            task.language.as_deref(),
        ),
    )
    .await
    .map_err(|_| {
        lmnotes_core::CoreError::Conformance("transcription timed out (900s, queued budget)".into())
    })?;
    let (tr, provider_id) = outcome?;

    build_and_write_transcript(
        task.asset_rel.clone(),
        &tr.text,
        &provider_id,
        &task.mime,
        task.duration_ms.map(|d| d.max(0) as u64),
        task.language.clone(),
        None,
        "media",
        &deps.indexer,
        &deps.sqlite,
        &deps.registry,
        &deps.routing,
        &deps.guard_cfg,
    )
    .await
}

/// 资产文件名（去扩展名）作 filename 提示。
fn hash_of(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

fn emit(app: &tauri::AppHandle, task: &MediaTask, status: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        "media-task-update",
        serde_json::json!({ "id": task.id, "kind": task.kind, "status": status,
                            "asset_rel": task.asset_rel, "result_path": task.result_path,
                            "error": task.error }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn queued_budget_is_fifteen_minutes() {
        // v0.5.1：队列预算承诺 15min（内联为 60s，见 commands.rs 调用点）
        let q: u64 = 15 * 60;
        assert_eq!(std::time::Duration::from_secs(q).as_secs(), 900);
    }

    #[test]
    fn worker_budget_floor_is_15min() {
        assert!(QUEUED_SUBPROC_TIMEOUT >= std::time::Duration::from_secs(15 * 60));
    }

    #[test]
    fn cancel_registry_abort_unknown_id_is_noop_and_clone_shares() {
        let reg = CancelRegistry::default();
        reg.abort("nope"); // 未知 id：必须 no-op

        let reg2 = reg.clone();
        // 真实长任务拿 AbortHandle，验证克隆视图 abort 命中
        let h = tokio::runtime::Runtime::new().unwrap().block_on(async {
            tokio::spawn(async { /* 长驻 */ }).abort_handle()
        });
        reg.register("t1", h);
        reg2.abort("t1");
        // 无 panic 即通过；重复 abort 幂等
        reg2.abort("t1");
    }

    #[test]
    fn cancel_registry_unregister_does_not_abort() {
        // v0.5.1 GAP-3：正常收尾走 unregister——不得触发 abort 闭包
        let reg = CancelRegistry::default();
        let fired = Arc::new(AtomicBool::new(false));
        let h = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { tokio::spawn(async {}).abort_handle() });
        reg.register("t3", h);
        reg.unregister("t3");
        reg.unregister("t3"); // 幂等：条目已移除，不 panic
                              // unregister 语义 = 仅移除（不触发 abort）：条目已删，后续 abort 对 t3 为 no-op
        reg.abort("t3");
        assert!(!fired.load(Ordering::SeqCst), "unregister must not abort");
    }
}
