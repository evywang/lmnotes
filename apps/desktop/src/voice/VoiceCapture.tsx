/**
 * 语音输入浮窗（FR-CAP-05 + FR-MEDIA-01）。
 *
 * 点击开始录音（getUserMedia + MediaRecorder），再点击停止 → 上传音频 bytes
 * 到 create_voice_note 命令（归档 + 云端 Whisper 转录 + 写 transcript concept）→
 * onNavigate 打开生成的笔记。
 *
 * 仿 Capture.tsx 的 overlay/signal 模式。无独立快捷窗场景：onNavigate 由父级传入。
 * Esc：录音中 = 取消录音（不转录）并关闭；空闲 = 直接关闭。
 * 转录失败且本地 STT 未就绪时，引导用户去设置下载模型（ADR-0007 §7 / 计划 T8）。
 */
import { createSignal, Show, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

interface LocalSttStatus {
  binary_available: boolean;
  ffmpeg_available: boolean;
  models: string[];
}

export function VoiceCapture(props: {
  onClose: () => void;
  onNavigate: (path: string) => void;
  /** 云端不可达且本地未就绪时，跳设置面板下载模型（引导降级，ADR-0007 §7）。 */
  onOpenSettings: () => void;
}) {
  const [recording, setRecording] = createSignal(false);
  const [seconds, setSeconds] = createSignal(0);
  const [processing, setProcessing] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [needsLocalSetup, setNeedsLocalSetup] = createSignal(false);

  let recorder: MediaRecorder | null = null;
  let chunks: BlobPart[] = [];
  let timer: ReturnType<typeof setInterval> | null = null;
  // Esc 取消标记：stop 触发 onstop 时据此跳过转录。
  let cancelled = false;

  onCleanup(() => {
    if (timer) clearInterval(timer);
    if (recorder && recorder.state !== "inactive") recorder.stop();
  });

  const onKeydown = (e: KeyboardEvent) => {
    if (e.key !== "Escape" || processing()) return;
    e.preventDefault();
    if (recording()) {
      cancelled = true;
      stop(); // onstop 检查 cancelled，跳过转录
    }
    props.onClose();
  };
  window.addEventListener("keydown", onKeydown);
  onCleanup(() => window.removeEventListener("keydown", onKeydown));

  const start = async () => {
    setError(null);
    setNeedsLocalSetup(false);
    cancelled = false;
    setSeconds(0);
    chunks = [];
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch {
      setError(t("voice.permissionDenied"));
      return;
    }
    // MediaRecorder 默认产出 webm/opus（OpenAI Whisper 接受）；Safari 走 mp4
    const mimeType = MediaRecorder.isTypeSupported("audio/webm")
      ? "audio/webm"
      : "audio/mp4";
    recorder = new MediaRecorder(stream, { mimeType });
    recorder.ondataavailable = (e) => {
      if (e.data.size > 0) chunks.push(e.data);
    };
    recorder.onstop = async () => {
      stream.getTracks().forEach((tr) => tr.stop());
      if (!cancelled) await transcribe();
    };
    recorder.start();
    setRecording(true);
    timer = setInterval(() => setSeconds((s) => s + 1), 1000);
  };

  const stop = () => {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    setRecording(false);
    if (recorder && recorder.state !== "inactive") {
      recorder.stop(); // 触发 onstop → transcribe
    }
  };

  const transcribe = async () => {
    if (chunks.length === 0) {
      setError(t("voice.errorTranscribe", { msg: "empty" }));
      return;
    }
    setProcessing(true);
    setError(null);
    setNeedsLocalSetup(false);
    try {
      const blob = new Blob(chunks, { type: recorder?.mimeType || "audio/webm" });
      const buf = new Uint8Array(await blob.arrayBuffer());
      // Tauri IPC 期望普通数组（序列化为 JSON）；ext 按 mime 推断
      const ext = (recorder?.mimeType || "audio/webm").includes("mp4") ? "mp4" : "webm";
      const path = await invoke<string>("create_voice_note", {
        audio: Array.from(buf),
        ext,
        mime: blob.type,
        durationMs: seconds() * 1000,
        language: null,
        title: null,
      });
      props.onNavigate(path);
      props.onClose();
    } catch (e) {
      console.error("create_voice_note failed", e);
      // 云端不可达且本地未就绪（无引擎或无模型）→ 引导下载模型启用离线降级。
      try {
        const st = await invoke<LocalSttStatus>("get_local_stt_status");
        if (!st.binary_available || st.models.length === 0) {
          setNeedsLocalSetup(true);
          setError(t("voice.cloudDownNoLocal"));
          return;
        }
      } catch {
        // 状态探测也失败：退回原始错误
      }
      setError(t("voice.errorTranscribe", { msg: String(e) }));
    } finally {
      setProcessing(false);
    }
  };

  return (
    <div class="capture-overlay" onClick={recording() || processing() ? undefined : props.onClose}>
      <div class="capture-box" onClick={(e) => e.stopPropagation()}>
        <h3 class="voice-title">{t("voice.title")}</h3>

        <Show when={error()}>
          <p class="voice-error">{error()}</p>
          <Show when={needsLocalSetup()}>
            <button
              class="btn-secondary"
              onClick={() => {
                props.onOpenSettings();
                props.onClose();
              }}
            >
              {t("voice.openSettings")}
            </button>
          </Show>
        </Show>

        <Show
          when={!processing()}
          fallback={<span class="muted">{t("voice.processing")}</span>}
        >
          <Show
            when={!recording()}
            fallback={
              <button class="voice-stop-btn" onClick={stop}>
                {t("voice.recording", { sec: seconds() })}
              </button>
            }
          >
            <button class="voice-record-btn" onClick={start}>
              ● {t("voice.start")}
            </button>
          </Show>
        </Show>
      </div>
    </div>
  );
}
