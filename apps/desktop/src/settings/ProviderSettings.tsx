import { createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t, locale, setLocale } from "../i18n";

interface ProviderRefSer {
  provider: string;
  model: string;
}

interface Config {
  providers: Array<
    | { type: "ollama"; base_url: string; chat_model: string; embed_model: string }
    | {
        type: "openai";
        id: string;
        base_url: string;
        api_key: string;
        chat_model: string;
        embed_model: string;
      }
  >;
  routing: {
    summarize?: ProviderRefSer;
    link_suggest?: ProviderRefSer;
    embed?: ProviderRefSer;
    chat?: ProviderRefSer;
    rewrite?: ProviderRefSer;
  };
  guard: { cloud_allowed: boolean; sensitive_patterns: string[] };
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

              <For each={cfg().providers}>
                {(p, i) => (
                  <div class="provider-block">
                    <h3>
                      {p.type === "ollama"
                        ? t("settings.ollamaLocal")
                        : t("settings.openaiCompat", { id: (p as { id: string }).id })}
                    </h3>
                    <label>
                      Base URL
                      <input
                        type="text"
                        value={p.base_url}
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
                  </div>
                )}
              </For>

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
