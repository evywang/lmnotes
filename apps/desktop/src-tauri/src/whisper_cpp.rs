//! 本地 whisper.cpp 转录 Provider（ADR-0007 / FR-MEDIA-05）。
//!
//! 作为云端 Whisper 不可用时的运行时降级（ADR-0007 §决策3）。置 Tauri 壳层而非
//! lmnotes-core，因其要调子进程（tokio::process）与写临时 wav（tokio::fs），
//! 触 ADR-0002 的 std::fs 禁令。核心层零改动——只 impl 已有的 TranscribeCap trait。
//!
//! 工作流：
//!   音频 bytes（webm/opus/mp4）→ ffmpeg sidecar 转 16kHz mono WAV
//!   → whisper.cpp 子进程（-m model -f wav -otxt -l lang）→ 读 <prefix>.txt
//!
//! 二进制与模型均不在仓库（ADR-0006 §决策1：不静态链接权重）：
//!   - whisper / ffmpeg 经 Tauri externalBin sidecar 分发（release 时 CI 下载预编译）
//!   - .bin 模型首次用语音时按需下载到 ~/.lmnotes/models/（见 commands::download_whisper_model）

#![allow(clippy::disallowed_methods)]

use lmnotes_core::llm::provider::{
    AudioInput, Capabilities, LlmProvider, ProviderKind, TranscribeCap, Transcript,
};
use lmnotes_core::Result;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// whisper.cpp 本地转录 Provider。
pub struct WhisperCppProvider {
    id: String,
    /// whisper.cpp 可执行文件路径（sidecar 或用户指定）。
    binary_path: PathBuf,
    /// ffmpeg 可执行文件路径（转码用；None 则跳过转码，假定输入已是 WAV）。
    ffmpeg_path: Option<PathBuf>,
    /// 模型权重路径（~/.lmnotes/models/ggml-<name>.bin）。
    model_path: PathBuf,
    /// CPU 线程数（-t）。
    threads: usize,
}

impl WhisperCppProvider {
    /// id 通常为 "whisper-cpp"，用于 Registry 区分。
    pub fn new(
        id: impl Into<String>,
        binary_path: impl Into<PathBuf>,
        ffmpeg_path: Option<PathBuf>,
        model_path: impl Into<PathBuf>,
        threads: usize,
    ) -> Self {
        Self {
            id: id.into(),
            binary_path: binary_path.into(),
            ffmpeg_path,
            model_path: model_path.into(),
            threads,
        }
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }
}

#[async_trait::async_trait]
impl LlmProvider for WhisperCppProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::TRANSCRIBE
    }
    /// 健康 = 二进制与模型文件都存在。不实际 spawn（避免子进程开销）。
    async fn health(&self) -> Result<bool> {
        Ok(self.binary_path.exists() && self.model_path.exists())
    }
}

#[async_trait::async_trait]
impl TranscribeCap for WhisperCppProvider {
    async fn transcribe(
        &self,
        audio: AudioInput,
        _model: &str,
        language: Option<&str>,
    ) -> Result<Transcript> {
        // 1) 归一化到 16kHz mono WAV（whisper.cpp 强制要求）。
        let tmp = tempfile::tempdir().map_err(lmnotes_core::CoreError::Io)?;
        let in_path = tmp
            .path()
            .join(format!("in.{}", sanitize_ext(&audio.filename)));
        let wav_path = tmp.path().join("in.wav");
        let out_prefix = tmp.path().join("out");

        tokio::fs::write(&in_path, &audio.bytes)
            .await
            .map_err(lmnotes_core::CoreError::Io)?;

        if is_wav(&audio.mime) && audio.bytes.starts_with(b"RIFF") {
            // 已是 WAV：跳过 ffmpeg（但 whisper.cpp 仍要 16kHz mono——多数情况仍需转码，
            // 这里保守地：WAV 且 mime 标 audio/wav 才直通）。
            tokio::fs::rename(&in_path, &wav_path)
                .await
                .map_err(lmnotes_core::CoreError::Io)?;
        } else if let Some(ffmpeg) = &self.ffmpeg_path {
            run_ffmpeg_transcode(ffmpeg, &in_path, &wav_path).await?;
        } else {
            return Err(lmnotes_core::CoreError::Conformance(format!(
                "non-WAV input ({}) needs ffmpeg sidecar for transcoding, but none configured",
                audio.mime
            )));
        }

        // 2) 跑 whisper.cpp 子进程。
        // 超时归调用点所有（v0.5.1：内联 60s / 队列 15min，见 transcribe_with_fallback）；
        // 此处不再自设上限——调用方超时丢弃 future 时 kill_on_drop 终止子进程。
        let mut cmd = build_whisper_cmd(
            &self.binary_path,
            &self.model_path,
            &wav_path,
            &out_prefix,
            language,
            self.threads,
        );
        let output = cmd.output().await.map_err(lmnotes_core::CoreError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(lmnotes_core::CoreError::Conformance(format!(
                "whisper.cpp exit {:?}: {}",
                output.status.code(),
                stderr.chars().take(500).collect::<String>()
            )));
        }

        // 3) 读 <out_prefix>.txt（-otxt 输出）。
        let txt_path = format!("{}.txt", out_prefix.to_string_lossy());
        let text = tokio::fs::read_to_string(&txt_path)
            .await
            .map_err(lmnotes_core::CoreError::Io)?;
        // tempdir drop 时自动清理临时文件。
        Ok(Transcript {
            text: text.trim().to_string(),
        })
    }
}

/// 构造 whisper.cpp 子进程命令（纯函数，便于单测参数拼装）。
/// CLI：`whisper -m <model> -f <wav> -otxt -l <lang> -of <out_prefix> -t <threads>`
fn build_whisper_cmd(
    binary: &Path,
    model: &Path,
    wav: &Path,
    out_prefix: &Path,
    language: Option<&str>,
    threads: usize,
) -> Command {
    let mut cmd = Command::new(binary);
    // 超时 drop future 时必须连带 kill 子进程，否则留下孤儿进程持续烧 CPU。
    cmd.kill_on_drop(true);
    cmd.arg("-m").arg(model);
    cmd.arg("-f").arg(wav);
    cmd.arg("-otxt");
    cmd.arg("-l").arg(language.unwrap_or("auto"));
    cmd.arg("-of").arg(out_prefix);
    cmd.arg("-t").arg(threads.to_string());
    // 静默 stderr 噪声（whisper.cpp 打印很多诊断行）。
    cmd.arg("--no-prints");
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd
}

/// 跑 ffmpeg 把任意音频转 16kHz mono PCM WAV（whisper.cpp 输入要求）。
async fn run_ffmpeg_transcode(ffmpeg: &Path, in_path: &Path, out_path: &Path) -> Result<()> {
    let mut cmd = Command::new(ffmpeg);
    cmd.kill_on_drop(true);
    cmd.arg("-y")
        .arg("-i")
        .arg(in_path)
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(out_path);
    // 超时归调用点所有（v0.5.1，与 whisper 步骤一致）：调用方以 budget 包裹整条链，
    // 超时丢弃 future 时 kill_on_drop 终止 ffmpeg。
    let output = cmd.output().await.map_err(lmnotes_core::CoreError::Io)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(lmnotes_core::CoreError::Conformance(format!(
            "ffmpeg exit {:?}: {}",
            output.status.code(),
            stderr.chars().take(500).collect::<String>()
        )));
    }
    Ok(())
}

fn is_wav(mime: &str) -> bool {
    mime.eq_ignore_ascii_case("audio/wav") || mime.eq_ignore_ascii_case("audio/x-wav")
}

/// 从文件名取扩展名，防路径穿越/非法字符。
fn sanitize_ext(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| {
            ext.chars()
                .filter(|c| c.is_alphanumeric())
                .take(8)
                .collect()
        })
        .unwrap_or_else(|| "bin".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cmd_assembles_expected_args() {
        let binary = Path::new("/usr/bin/whisper");
        let model = Path::new("/models/ggml-base.bin");
        let wav = Path::new("/tmp/in.wav");
        let out = Path::new("/tmp/out");
        let cmd = build_whisper_cmd(binary, model, wav, out, Some("zh"), 4);
        // Command 内部 args 不可直接读，但可通过 format! 粗检（Debug 输出）。
        let dbg = format!("{cmd:?}");
        // v0.5.1：超时归调用点 → kill_on_drop 必须开启（丢弃 future 即杀子进程）
        assert!(
            dbg.contains("kill_on_drop: true"),
            "missing kill_on_drop: {dbg}"
        );
        for needle in [
            "/usr/bin/whisper",
            "-m",
            "/models/ggml-base.bin",
            "-f",
            "/tmp/in.wav",
            "-otxt",
            "-l",
            "zh",
            "-of",
            "/tmp/out",
            "-t",
            "4",
            "--no-prints",
        ] {
            assert!(dbg.contains(needle), "cmd missing {needle:?}: {dbg}");
        }
    }

    #[test]
    fn build_cmd_defaults_language_to_auto() {
        let cmd = build_whisper_cmd(
            Path::new("w"),
            Path::new("m"),
            Path::new("f"),
            Path::new("o"),
            None,
            2,
        );
        let dbg = format!("{cmd:?}");
        assert!(
            dbg.contains("-l") && dbg.contains("auto"),
            "no auto lang: {dbg}"
        );
    }

    #[test]
    fn sanitize_ext_handles_filenames() {
        assert_eq!(sanitize_ext("rec.webm"), "webm");
        assert_eq!(sanitize_ext("audio.mp4"), "mp4");
        assert_eq!(sanitize_ext("noext"), "bin");
    }

    #[test]
    fn is_wav_detects_mime() {
        assert!(is_wav("audio/wav"));
        assert!(is_wav("audio/x-wav"));
        assert!(!is_wav("audio/webm"));
    }

    #[test]
    fn health_checks_files_exist() {
        // 给一个不存在的路径，health 应为 false。
        let p = WhisperCppProvider::new(
            "whisper-cpp",
            "/nonexistent/whisper",
            None,
            "/nonexistent/model.bin",
            4,
        );
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ok = rt.block_on(p.health()).unwrap();
        assert!(!ok, "health should be false for nonexistent files");
    }
}
