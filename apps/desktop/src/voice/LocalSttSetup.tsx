/**
 * 本地 STT（whisper.cpp）设置面板（ADR-0007）。
 *
 * 显示 whisper.cpp / ffmpeg sidecar 状态、已下载模型、可下载模型列表与进度。
 * 两个挂载点：ProviderSettings（设置页，完整模式）与 VoiceCapture（语音弹窗，
 * inline 模式——无标题/描述，弹窗自带引导文案）。
 *
 * 后端命令：
 *   get_local_stt_status -> { binary_available, ffmpeg_available, models: string[] }
 *   list_whisper_models  -> [{ name, label, size_mb, downloaded, url }]
 *   download_whisper_model(name) -> path（流式下载，emit whisper-model-progress 事件）
 * 模型推荐语文案在后端只出结构化字段，由前端 i18n（localStt.modelNote.<name>）渲染。
 */
import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t, type MessageKey } from "../i18n";

/** 模型短名 → 推荐语 i18n key（后端只出结构化字段，文案随 locale 切换）。 */
const MODEL_NOTES: Record<string, MessageKey> = {
  base: "localStt.modelNote.base",
  small: "localStt.modelNote.small",
  medium: "localStt.modelNote.medium",
};

interface LocalSttStatus {
  binary_available: boolean;
  ffmpeg_available: boolean;
  models: string[];
}

interface WhisperModel {
  name: string;
  label: string;
  size_mb: number;
  downloaded: boolean;
  url: string;
}

interface ProgressEvent {
  name: string;
  downloaded?: number; // bytes
  total?: number | null; // bytes, may be null if unknown
  done?: boolean;
}

export function LocalSttSetup(props: { inline?: boolean }) {
  const [status, setStatus] = createSignal<LocalSttStatus | null>(null);
  const [models, setModels] = createSignal<WhisperModel[]>([]);
  const [downloading, setDownloading] = createSignal<string | null>(null);
  const [progress, setProgress] = createSignal<{ downloaded: number; total: number | null } | null>(
    null,
  );
  const [error, setError] = createSignal<string | null>(null);

  const refresh = async () => {
    try {
      const [s, m] = await Promise.all([
        invoke<LocalSttStatus>("get_local_stt_status"),
        invoke<WhisperModel[]>("list_whisper_models"),
      ]);
      setStatus(s);
      setModels(m);
    } catch (e) {
      console.error("local stt status failed", e);
    }
  };

  let unlisten: UnlistenFn | null = null;
  onMount(async () => {
    await refresh();
    unlisten = await listen<ProgressEvent>("whisper-model-progress", (ev) => {
      const p = ev.payload;
      if (p.done) {
        setDownloading(null);
        setProgress(null);
        refresh();
        return;
      }
      setProgress({ downloaded: p.downloaded ?? 0, total: p.total ?? null });
    });
  });
  onCleanup(() => unlisten?.());

  const download = async (name: string) => {
    setError(null);
    setDownloading(name);
    setProgress({ downloaded: 0, total: null });
    try {
      await invoke<string>("download_whisper_model", { name });
      // done 事件会触发 refresh；这里兜底也刷一次
      await refresh();
    } catch (e) {
      setError(t("localStt.downloadFailed", { msg: String(e) }));
    } finally {
      setDownloading(null);
      setProgress(null);
    }
  };

  const fmtMb = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);

  return (
    <div class="local-stt-section">
      <Show when={!props.inline}>
        <h3>{t("localStt.title")}</h3>
        <p class="muted small">{t("localStt.description")}</p>
      </Show>

      <div class="local-stt-status">
        <div>
          {status()?.binary_available ? t("localStt.binaryOk") : t("localStt.binaryMissing")}
        </div>
        <div>
          {status()?.ffmpeg_available ? t("localStt.ffmpegOk") : t("localStt.ffmpegMissing")}
        </div>
      </div>

      <Show when={error()}>
        <p class="voice-error">{error()}</p>
      </Show>

      <div class="local-stt-models">
        <h4>{t("localStt.models")}</h4>
        <Show
          when={models().length > 0}
          fallback={<p class="muted small">{t("localStt.noModelsDownloaded")}</p>}
        >
          <For each={models()}>
            {(m) => {
              const isDownloaded = () => m.downloaded || (status()?.models.includes(m.name) ?? false);
              const isDownloading = () => downloading() === m.name;
              return (
                <div class="local-stt-model-row">
                  <div class="local-stt-model-info">
                    <strong>
                      {isDownloaded() ? "✓ " : ""}
                      {m.label}
                    </strong>
                    <span class="muted small">
                      {m.size_mb} MB
                      <Show when={MODEL_NOTES[m.name]}>
                        {" · "}
                        {t(MODEL_NOTES[m.name]!)}
                      </Show>
                    </span>
                    <Show when={isDownloading() && progress()}>
                      {(p) => (
                        <span class="muted small">
                          {p().total
                            ? t("localStt.downloading", {
                                downloaded: fmtMb(p().downloaded),
                                total: fmtMb(p().total!),
                              })
                            : t("localStt.downloadingIndeterminate", {
                                downloaded: fmtMb(p().downloaded),
                              })}
                        </span>
                      )}
                    </Show>
                  </div>
                  <Show when={!isDownloaded() && !isDownloading()}>
                    <button
                      class="btn-secondary"
                      onClick={() => download(m.name)}
                      disabled={!status()?.binary_available}
                      title={!status()?.binary_available ? t("localStt.binaryMissing") : ""}
                    >
                      {t("localStt.download")}
                    </button>
                  </Show>
                </div>
              );
            }}
          </For>
        </Show>
      </div>
    </div>
  );
}
