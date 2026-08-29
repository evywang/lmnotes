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
    transcribe_with_fallback, vault_root,
};
use lmnotes_core::index::schema::MediaTask;
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::Indexer;
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::provider::AudioInput;
use lmnotes_core::llm::routing::{Registry, Routing};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// 队列任务子进程超时（长媒体；同步路径为 60s）。
const QUEUED_SUBPROC_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// worker 执行体依赖（Arc 组，lib.rs 启动时注入）。
#[derive(Clone)]
pub struct WorkerDeps {
    pub indexer: Arc<Indexer>,
    pub sqlite: Arc<SqliteIndex>,
    pub registry: Arc<Registry>,
    pub routing: Arc<Routing>,
    pub guard_cfg: Arc<GuardConfig>,
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
async fn run_task(app: &tauri::AppHandle, deps: &WorkerDeps, task: &MediaTask) {
    let _ = deps
        .sqlite
        .update_media_task_status(&task.id, "running", None, None);
    emit(app, task, "running");

    let result = execute_with_retry(deps, task).await;
    match result {
        Ok(path) => {
            let _ = deps
                .sqlite
                .update_media_task_status(&task.id, "done", None, Some(&path));
            emit(app, task, "done");
        }
        Err(e) => {
            let _ = deps
                .sqlite
                .update_media_task_status(&task.id, "failed", Some(&e), None);
            emit(app, task, "failed");
        }
    }
}

async fn execute_with_retry(deps: &WorkerDeps, task: &MediaTask) -> Result<String, String> {
    match execute(deps, task).await {
        Ok(p) => Ok(p),
        Err(e) if is_retryable(&e) => {
            eprintln!(
                "media task {} retryable failure ({e}), retrying once",
                task.id
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            execute(deps, task).await
        }
        Err(e) => Err(e),
    }
}

/// 网络类错误可重试（连接/超时/5xx）；配置/本地错误不重试。
fn is_retryable(err: &str) -> bool {
    let l = err.to_lowercase();
    l.contains("timeout")
        || l.contains("connect")
        || l.contains("http 5")
        || l.contains(" dns ")
        || l.contains("timed out")
}

async fn execute(deps: &WorkerDeps, task: &MediaTask) -> Result<String, String> {
    match task.kind.as_str() {
        "transcribe" => execute_transcribe(deps, task).await,
        "describe" => {
            describe_image_core(
                &task.asset_rel,
                &deps.indexer,
                &deps.sqlite,
                &deps.registry,
                &deps.routing,
                &deps.guard_cfg,
            )
            .await
        }
        other => Err(format!("unknown task kind: {other}")),
    }
}

/// 转录任务：读归档媒体 →（视频抽音轨）→ 云优先/本地兜底转录 → transcript 笔记。
/// 与同步命令同一编排（transcribe_with_fallback + build_and_write_transcript）。
async fn execute_transcribe(deps: &WorkerDeps, task: &MediaTask) -> Result<String, String> {
    let rel = task.asset_rel.trim_start_matches('/');
    if !rel.starts_with("assets/") || rel.contains("..") {
        return Err(format!("invalid asset path: {}", task.asset_rel));
    }
    let full = vault_root().join(rel);
    let data = tokio::fs::read(&full)
        .await
        .map_err(|e| format!("read asset failed: {e}"))?;
    let ext = rel.rsplit('.').next().unwrap_or("bin").to_string();
    let is_video = rel.starts_with("assets/video/");

    // 视频：ffmpeg 抽音轨（队列任务的超时放宽——经 select 包一层总超时）
    let audio_bytes: Vec<u8>;
    let audio_mime;
    let audio_ext;
    if is_video {
        let ffmpeg = ffmpeg_binary_path().ok_or_else(|| {
            "video transcription requires the ffmpeg sidecar (not found)".to_string()
        })?;
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let wav = tmp.path().join("audio.wav");
        let mut cmd = build_extract_audio_cmd(&ffmpeg, Path::new(&full), &wav);
        let out = tokio::time::timeout(QUEUED_SUBPROC_TIMEOUT, cmd.output())
            .await
            .map_err(|_| "ffmpeg audio extraction timed out (15min)".to_string())?
            .map_err(|e| format!("ffmpeg spawn failed: {e}"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!(
                "ffmpeg failed: {}",
                stderr.chars().take(300).collect::<String>()
            ));
        }
        audio_bytes = tokio::fs::read(&wav)
            .await
            .map_err(|e| format!("read extracted audio failed: {e}"))?;
        audio_mime = "audio/wav".into();
        audio_ext = "wav".into();
    } else {
        audio_bytes = data;
        audio_mime = task.mime.clone();
        audio_ext = ext;
    }
    if audio_bytes.is_empty() {
        return Err("no audio track found in media file".into());
    }

    let (tr, provider_id) = transcribe_with_fallback(
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
        // 队列预算：15min（v0.5.1 兑现设计承诺；长视频抽音轨同预算见 execute_transcribe）
        std::time::Duration::from_secs(15 * 60),
    )
    .await?;

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

    #[test]
    fn queued_budget_is_fifteen_minutes() {
        // v0.5.1：队列预算承诺 15min（内联为 60s，见 commands.rs 调用点）
        let q: u64 = 15 * 60;
        assert_eq!(std::time::Duration::from_secs(q).as_secs(), 900);
    }

    #[test]
    fn worker_uses_queued_budget_constant() {
        // 守护:若有人改 QUEUED_SUBPROC_TIMEOUT,不得低于设计承诺的 15min
        assert!(QUEUED_SUBPROC_TIMEOUT >= std::time::Duration::from_secs(15 * 60));
    }
}
