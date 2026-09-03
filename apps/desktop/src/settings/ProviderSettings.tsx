import { createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t, locale, setLocale } from "../i18n";
import { LocalSttSetup } from "../voice/LocalSttSetup";
import { save as saveDialog, message } from "@tauri-apps/plugin-dialog";
import { APP_NAME } from "../components/PromptDialog";
import { VaultSection } from "./VaultSection";

interface ProviderRefSer {
  provider: string;
  model: string;
}

interface Config {
  providers: Array<
    | {
        type: "ollama";
        base_url: string;
        chat_model: string;
        embed_model: string;
        embed_dim?: number;
        vision_model?: string;
      }
    | {
        type: "openai";
        id: string;
        base_url: string;
        api_key: string;
        chat_model: string;
        embed_model: string;
        embed_dim?: number;
        transcribe_model?: string;
        vision_model?: string;
      }
  >;
  routing: {
    summarize?: ProviderRefSer;
    link_suggest?: ProviderRefSer;
    embed?: ProviderRefSer;
    chat?: ProviderRefSer;
    rewrite?: ProviderRefSer;
    transcribe?: ProviderRefSer;
    vision?: ProviderRefSer;
  };
  guard: { cloud_allowed: boolean; sensitive_patterns: string[] };
  media: { background_threshold_ms: number };
}

interface ProviderHealth {
  provider_id: string;
  healthy: boolean;
}

export function ProviderSettings(props: { onClose: () => void }) {
  const [config, setConfig] = createSignal<Config | null>(null);
  const [health, setHealth] = createSignal<ProviderHealth[]>([]);
  const [saving, setSaving] = createSignal(false);

  onMount(async () => {
    try {
      const c = await invoke<Config>("get_config");
      setConfig(c);
      const h = await invoke<ProviderHealth[]>("probe_providers", { config: c });
      setHealth(h);
    } catch (e) {
      console.error("load config", e);
    }
  });

  const save = async () => {
    setSaving(true);
    try {
      await invoke("set_config", { config: config() });
      props.onClose();
    } catch (e) {
      console.error("save config", e);
    } finally {
      setSaving(false);
    }
  };

  const reprobe = async () => {
    const h = await invoke<ProviderHealth[]>("probe_providers", { config: config() });
    setHealth(h);
  };

  /** 新增 OpenAI 兼容 provider（id 唯一自动生成；Ollama 单实例不加）。
   *  典型场景：主 LLM 用 GLM（无转录端点），另加一个支持
   *  /audio/transcriptions 的服务（如硅基流动/OpenAI）专供在线 STT。 */
  const addProvider = () => {
    const cfg = config();
    if (!cfg) return;
    const ids = new Set(
      cfg.providers.map((p) => (p.type === "openai" ? p.id : "")).filter(Boolean),
    );
    let n = cfg.providers.length + 1;
    let id = `cloud-${n}`;
    while (ids.has(id)) {
      n += 1;
      id = `cloud-${n}`;
    }
    setConfig({
      ...cfg,
      providers: [
        ...cfg.providers,
        {
          type: "openai",
          id,
          base_url: "",
          api_key: "",
          chat_model: "",
          embed_model: "",
        },
      ],
    });
  };

  /** 移除 provider（仅 OpenAI 兼容类）并清掉指向它的 routing 引用，防悬空。 */
  const removeProvider = (idx: number) => {
    const cfg = config();
    if (!cfg) return;
    const pid = cfg.providers[idx].type === "openai" ? cfg.providers[idx].id : null;
    const strip = (r?: ProviderRefSer): ProviderRefSer | undefined =>
      pid && r?.provider === pid ? undefined : r;
    setConfig({
      ...cfg,
      providers: cfg.providers.filter((_, i) => i !== idx),
      routing: {
        ...cfg.routing,
        summarize: strip(cfg.routing.summarize),
        link_suggest: strip(cfg.routing.link_suggest),
        embed: strip(cfg.routing.embed),
        chat: strip(cfg.routing.chat),
        rewrite: strip(cfg.routing.rewrite),
        transcribe: strip(cfg.routing.transcribe),
        vision: strip(cfg.routing.vision),
      },
    });
  };

  return (
    <div class="capture-overlay" onClick={props.onClose}>
      <div class="settings-box" onClick={(e) => e.stopPropagation()}>
        <h2>{t("settings.title")}</h2>
        <Show when={config()} fallback={<p class="muted">{t("settings.loading")}</p>}>
          {(cfg) => (
            <div class="settings-form">
              {/* 通用：语言切换 */}
              <div class="general-section">
                <h3>{t("settings.generalSection")}</h3>
                <div class="lang-field">
                  <span class="lang-label">{t("settings.language")}</span>
                  <div class="lang-toggle">
                    <button
                      type="button"
                      class={locale() === "zh" ? "lang-btn active" : "lang-btn"}
                      onClick={() => setLocale("zh")}
                    >
                      {t("settings.languageZh")}
                    </button>
                    <button
                      type="button"
                      class={locale() === "en" ? "lang-btn active" : "lang-btn"}
                      onClick={() => setLocale("en")}
                    >
                      {t("settings.languageEn")}
                    </button>
                  </div>
                </div>
              </div>

              <VaultSection />

              <For each={cfg().providers}>
                {(p, i) => (
                  <div class="provider-block">
                    <div class="provider-header">
                      <h3>
                        {p.type === "ollama"
                          ? t("settings.ollamaLocal")
                          : t("settings.openaiCompat", { id: (p as { id: string }).id })}
                      </h3>
                      <Show when={p.type === "openai"}>
                        <button
                          type="button"
                          class="btn-secondary btn-small"
                          onClick={() => removeProvider(i())}
                        >
                          {t("settings.removeProvider")}
                        </button>
                      </Show>
                    </div>
                    <label>
                      Base URL
                      <input
                        type="text"
                        value={p.base_url}
                        placeholder="https://api.openai.com/v1"
                        onInput={(e) => {
                          const next = [...cfg().providers];
                          next[i()] = { ...p, base_url: e.currentTarget.value } as typeof p;
                          setConfig({ ...cfg(), providers: next });
                        }}
                      />
                    </label>
                    <Show when={p.type === "openai"}>
                      <label>
                        API Key
                        <input
                          type="password"
                          value={(p as { api_key: string }).api_key}
                          onInput={(e) => {
                            const next = [...cfg().providers];
                            next[i()] = { ...p, api_key: e.currentTarget.value } as typeof p;
                            setConfig({ ...cfg(), providers: next });
                          }}
                        />
                      </label>
                    </Show>
                    <label>
                      Chat Model
                      <input
                        type="text"
                        value={p.chat_model}
                        onInput={(e) => {
                          const next = [...cfg().providers];
                          next[i()] = { ...p, chat_model: e.currentTarget.value } as typeof p;
                          setConfig({ ...cfg(), providers: next });
                        }}
                      />
                    </label>
                    <label>
                      {t("settings.visionModel")}
                      <input
                        type="text"
                        value={(p as { vision_model?: string }).vision_model ?? ""}
                        placeholder={t("settings.visionModelPlaceholder")}
                        onInput={(e) => {
                          const next = [...cfg().providers];
                          const v = e.currentTarget.value.trim();
                          next[i()] = {
                            ...p,
                            vision_model: v === "" ? undefined : v,
                          } as typeof p;
                          setConfig({ ...cfg(), providers: next });
                        }}
                      />
                    </label>
                    <label>
                      Embed Model
                      <input
                        type="text"
                        value={p.embed_model}
                        onInput={(e) => {
                          const next = [...cfg().providers];
                          next[i()] = { ...p, embed_model: e.currentTarget.value } as typeof p;
                          setConfig({ ...cfg(), providers: next });
                        }}
                      />
                    </label>
                    <Show when={p.type === "openai"}>
                      <label>
                        {t("settings.transcribeModel")}
                        <input
                          type="text"
                          value={(p as { transcribe_model?: string }).transcribe_model ?? ""}
                          placeholder={t("settings.transcribeModelPlaceholder")}
                          onInput={(e) => {
                            const next = [...cfg().providers];
                            const v = e.currentTarget.value.trim();
                            next[i()] = {
                              ...p,
                              transcribe_model: v === "" ? undefined : v,
                            } as typeof p;
                            setConfig({ ...cfg(), providers: next });
                          }}
                        />
                      </label>
                      <p class="muted small">{t("settings.transcribeModelHint")}</p>
                    </Show>
                  </div>
                )}
              </For>

              <button type="button" class="btn-secondary" onClick={addProvider}>
                + {t("settings.addProvider")}
              </button>

              <div class="health-section">
                <h3>{t("settings.health")}</h3>
                <For each={health()}>
                  {(h) => (
                    <div class="health-item">
                      <span>{h.healthy ? "✓" : "✕"}</span>
                      <span>{h.provider_id}</span>
                      <span class="muted small">
                        {h.healthy ? t("settings.healthy") : t("settings.unhealthy")}
                      </span>
                    </div>
                  )}
                </For>
                <button class="btn-secondary" onClick={reprobe}>
                  {t("settings.reprobe")}
                </button>
              </div>

              <div class="guard-section">
                <label class="checkbox">
                  <input
                    type="checkbox"
                    checked={cfg().guard.cloud_allowed}
                    onChange={(e) =>
                      setConfig({
                        ...cfg(),
                        guard: {
                          ...cfg().guard,
                          cloud_allowed: e.currentTarget.checked,
                        },
                      })
                    }
                  />
                  {t("settings.cloudAllowed")}
                </label>
              </div>

              <div class="guard-section">
                <label class="checkbox">
                  <input
                    type="checkbox"
                    checked={cfg().media.background_threshold_ms === 0}
                    onChange={(e) =>
                      setConfig({
                        ...cfg(),
                        media: {
                          background_threshold_ms: e.currentTarget.checked ? 0 : 60_000,
                        },
                      })
                    }
                  />
                  {t("settings.backgroundMedia")}
                </label>
                <p class="muted small">{t("settings.backgroundMediaHint")}</p>
              </div>

              <LocalSttSetup />

              <div class="data-section">
                <h3>{t("data.title")}</h3>
                <div class="settings-actions" style={{ "justify-content": "flex-start" }}>
                  <button
                    class="btn-secondary"
                    onClick={async () => {
                      const dest = await saveDialog({
                        title: t("data.exportDialog"),
                        defaultPath: "lmnotes-vault.zip",
                        filters: [{ name: "ZIP", extensions: ["zip"] }],
                      });
                      if (!dest || typeof dest !== "string") return;
                      try {
                        const n = await invoke<number>("export_vault_zip", { dest });
                        void message(t("data.exportDone", { n }), { title: APP_NAME });
                      } catch (e) {
                        void message(String(e), { title: APP_NAME, kind: "error" });
                      }
                    }}
                  >
                    {t("data.exportZip")}
                  </button>
                  <button
                    class="btn-secondary"
                    onClick={async () => {
                      try {
                        const msg = await invoke<string>("init_git_repo");
                        void message(msg, { title: APP_NAME });
                      } catch (e) {
                        void message(String(e), { title: APP_NAME, kind: "error" });
                      }
                    }}
                  >
                    {t("data.gitInit")}
                  </button>
                </div>
                <p class="muted small">{t("data.hint")}</p>
              </div>

              <div class="settings-actions">
                <button class="btn-primary" onClick={save} disabled={saving()}>
                  {saving() ? t("settings.saving") : t("settings.save")}
                </button>
                <button class="btn-secondary" onClick={props.onClose}>
                  {t("settings.cancel")}
                </button>
              </div>
              <p class="muted small">{t("settings.restartHint")}</p>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
