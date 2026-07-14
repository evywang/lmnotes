/**
 * 语音输入浮窗（FR-CAP-05 + FR-MEDIA-01）。
 *
 * 点击开始录音（getUserMedia + MediaRecorder），再点击停止 → 上传音频 bytes
 * 到 create_voice_note 命令（归档 + 云端 Whisper 转录 + 写 transcript concept）→
 * onNavigate 打开生成的笔记。
 *
 * 仿 Capture.tsx 的 overlay/signal 模式。无独立快捷窗场景：onNavigate 由父级传入。
 */
import { createSignal, Show, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

export function VoiceCapture(props: {
  onClose: () => void;
  onNavigate: (path: string) => void;
}) {
  const [recording, setRecording] = createSignal(false);
  const [seconds, setSeconds] = createSignal(0);
  const [processing, setProcessing] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  let recorder: MediaRecorder | null = null;
  let chunks: BlobPart[] = [];
  let timer: ReturnType<typeof setInterval> | null = null;

  onCleanup(() => {
    if (timer) clearInterval(timer);
    if (recorder && recorder.state !== "inactive") recorder.stop();
  });

  const start = async () => {
    setError(null);
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
      await transcribe();
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
