/**
 * 语音输入浮窗（FR-CAP-05 + FR-MEDIA-01）。
 *
 * 打开时即探测本地 STT 就绪状态（get_local_stt_status）：引擎在而无模型 →
 * 直接在弹窗内嵌模型下载面板（LocalSttSetup inline），下载完即可离线转录，
 * 不必先跳设置页（下载后模型路径动态解析，无需重启）。
 *
 * 点击开始录音（getUserMedia + MediaRecorder），再点击停止 → 上传音频 bytes
 * 到 create_voice_note 命令（归档 + 云端 Whisper 转录 + 写 transcript concept）→
 * onNavigate 打开生成的笔记。
 *
 * 仿 Capture.tsx 的 overlay/signal 模式。无独立快捷窗场景：onNavigate 由父级传入。
 * Esc：录音中 = 取消录音（不转录）并关闭；空闲 = 直接关闭。
 * 转录失败且本地 STT 未就绪时，兜底引导用户去设置下载模型（ADR-0007 §7 / 计划 T8）。
 */
import { createSignal, Show, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t } from "../i18n";
import { LocalSttSetup } from "./LocalSttSetup";

interface LocalSttStatus {
  binary_available: boolean;
  ffmpeg_available: boolean;
  models: string[];
}

interface ModelProgressEvent {
  name: string;
  done?: boolean;
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
  const [queued, setQueued] = createSignal(false); // 长录音已入队（任务中心可查，v0.5 分流）
  // null = 探测中；true = 本地就绪；false = 引擎在但无模型 → 弹窗内嵌下载面板
  const [localReady, setLocalReady] = createSignal<boolean | null>(null);

  const probeLocal = async () => {
    try {
      const st = await invoke<LocalSttStatus>("get_local_stt_status");
      setLocalReady(st.binary_available && st.models.length > 0);
    } catch {
      setLocalReady(true); // 探测失败不阻塞录音（云端可用时仍可录）
    }
  };
  void probeLocal();
  // 模型下载完成 → 重新探测（面板消失，录音走本地无需重启）。
  let unlistenModel: UnlistenFn | null = null;
  void listen<ModelProgressEvent>("whisper-model-progress", (ev) => {
    if (ev.payload.done) void probeLocal();
  }).then((fn) => (unlistenModel = fn));
  onCleanup(() => unlistenModel?.());

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

  // push-to-talk：async 授权期间可能已松手——记 flag，recorder 就绪即丢弃（零时长）。
  let releasedBeforeReady = false;

  const start = async () => {
    setError(null);
    setNeedsLocalSetup(false);
    cancelled = false;
    releasedBeforeReady = false;
    setSeconds(0);
    chunks = [];
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch {
      setError(t("voice.permissionDenied"));
      return;
    }
    if (releasedBeforeReady) {
      stream.getTracks().forEach((tr) => tr.stop());
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
    } else {
      releasedBeforeReady = true; // recorder 未就绪即松手：start() 侧丢弃
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
      // v0.5 分流：长录音（超阈值）入队后台处理，速记保持同步（即时反馈）
      let threshold = 60_000;
      try {
        const cfg = await invoke<{ media: { background_threshold_ms: number } }>("get_config");
        threshold = cfg.media?.background_threshold_ms ?? 60_000;
      } catch {
        /* 读不到配置用默认 60s */
      }
      const durationMs = seconds() * 1000;
      if (durationMs > threshold) {
        await invoke("enqueue_media_task", {
          kind: "transcribe",
          assetRel: null,
          data: Array.from(buf),
          ext,
          mime: blob.type,
          durationMs,
          language: null,
        });
        setQueued(true);
        props.onClose();
        return;
      }
      const path = await invoke<string>("create_voice_note", {
        audio: Array.from(buf),
        ext,
        mime: blob.type,
        durationMs,
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

        <Show when={queued()}>
          <p class="muted small">{t("voice.queuedHint")}</p>
        </Show>

        <Show when={localReady() === false}>
          <p class="muted small">{t("voice.localSetupHint")}</p>
          <LocalSttSetup inline />
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
            <button
              class="voice-record-btn voice-ptt"
              onPointerDown={(e) => {
                e.preventDefault(); // 防聚焦/选中
                void start();
              }}
              onPointerUp={stop}
              onPointerLeave={() => recording() && stop()}
              onPointerCancel={stop}
              onContextMenu={(e) => e.preventDefault()}
            >
              ● {t("voice.holdToRecord")}
            </button>
          </Show>
        </Show>
      </div>
    </div>
  );
}
