/**
 * 多 Vault 管理小节（FR-STORE-01，ADR-0008 重启式切换）。
 * 挂在 ProviderSettings 顶部：当前库 + 库清单 + 添加/移除/切换。
 */
import { createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import { APP_NAME } from "../components/PromptDialog";

interface VaultInfo {
  path: string;
  name: string;
  current: boolean;
}

export function VaultSection() {
  const [vaults, setVaults] = createSignal<VaultInfo[]>([]);

  const refresh = async () => {
    try {
      setVaults(await invoke<VaultInfo[]>("list_vaults"));
    } catch (e) {
      console.error("list_vaults failed", e);
    }
  };
  onMount(refresh);

  const add = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || typeof selected !== "string") return;
    try {
      await invoke("add_vault", { path: selected });
      await refresh();
    } catch (e) {
      console.error("add_vault failed", e);
    }
  };

  const remove = async (path: string) => {
    if (
      !(await confirmDialog(t("vault.removeConfirm", { name: path }), {
        title: APP_NAME,
      }))
    )
      return;
    try {
      await invoke("remove_vault", { path });
      await refresh();
    } catch (e) {
      console.error("remove_vault failed", e);
    }
  };

  const switchTo = async (path: string) => {
    if (
      !(await confirmDialog(t("vault.switchConfirm"), { title: APP_NAME }))
    )
      return;
    // switch_vault 成功后应用重启，无需刷新
    try {
      await invoke("switch_vault", { path });
    } catch (e) {
      console.error("switch_vault failed", e);
    }
  };

  return (
    <div class="vault-section">
      <h3>📚 {t("vault.title")}</h3>
      <ul class="vault-list">
        <For each={vaults()}>
          {(v) => (
            <li class={`vault-item ${v.current ? "current" : ""}`}>
              <span class="vault-name">
                {v.current ? "● " : ""}
                {v.name}
              </span>
              <span class="muted small vault-path">{v.path}</span>
              <Show when={!v.current}>
                <button class="btn-secondary" onClick={() => switchTo(v.path)}>
                  {t("vault.switch")}
                </button>
                <button class="btn-secondary" onClick={() => remove(v.path)}>
                  {t("vault.remove")}
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <button class="btn-secondary" onClick={add}>
        ＋ {t("vault.add")}
      </button>
      <p class="muted small">{t("vault.hint")}</p>
    </div>
  );
}
