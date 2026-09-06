/**
 * 导航栏（v1.0 spec §3.1）：48px 图标纵列 = 全局导航。
 * 上组按使用频率：笔记/今日/时间线/图谱/问答/任务；底部：库指示 + 设置。
 * 浮层（问答/图谱）打开期间对应图标保持激活态。
 */
import { createSignal, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./icons";
import { activeMediaTaskCount } from "../voice/MediaTasks";
import { t } from "../i18n";

export type RailActive = "chat" | "graph" | null;

interface Props {
  active: RailActive;
  onDaily: () => void;
  onTimeline: () => void;
  onGraph: () => void;
  onAsk: () => void;
  onTasks: () => void;
  onSettings: () => void;
}

export function Rail(props: Props) {
  const [vaultName, setVaultName] = createSignal<string | null>(null);
  onMount(async () => {
    try {
      const vs = await invoke<{ name: string; current: boolean }[]>("list_vaults");
      setVaultName(vs.find((v) => v.current)?.name ?? null);
    } catch {
      // 静默：指示器失败不打扰
    }
  });

  return (
    <nav class="rail" aria-label={t("nav.label")}>
      {/* 笔记 = 应用默认态（上下文栏始终在场），恒激活 */}
      <button
        class="rail-item active"
        title={t("nav.notes")}
        aria-label={t("nav.notes")}
        aria-current="page"
      >
        <Icon name="pencil" size={18} />
      </button>
      <button
        class="rail-item"
        title={t("nav.daily")}
        aria-label={t("nav.daily")}
        onClick={props.onDaily}
      >
        <Icon name="calendar" size={18} />
      </button>
      <button
        class="rail-item"
        title={t("nav.timeline")}
        aria-label={t("nav.timeline")}
        onClick={props.onTimeline}
      >
        <Icon name="clock" size={18} />
      </button>
      <button
        class={`rail-item ${props.active === "graph" ? "active" : ""}`}
        title={t("nav.graph")}
        aria-label={t("nav.graph")}
        onClick={props.onGraph}
      >
        <Icon name="network" size={18} />
      </button>
      <button
        class={`rail-item ${props.active === "chat" ? "active" : ""}`}
        title={t("nav.ask")}
        aria-label={t("nav.ask")}
        onClick={props.onAsk}
      >
        <Icon name="spark" size={18} />
      </button>
      <button
        class="rail-item"
        title={t("nav.tasks")}
        aria-label={activeMediaTaskCount() > 0 ? `${t("nav.tasks")} (${activeMediaTaskCount()})` : t("nav.tasks")}
        onClick={props.onTasks}
      >
        <span class="rail-icon-wrap">
          <Icon name="tasks" size={18} />
          <Show when={activeMediaTaskCount() > 0}>
            <span class="rail-badge">{activeMediaTaskCount()}</span>
          </Show>
        </span>
      </button>

      <div class="rail-spacer" />
      <Show when={vaultName()}>
        <button
          class="rail-item"
          title={t("nav.vaultTooltip", { name: vaultName()! })}
          aria-label={t("nav.vaultTooltip", { name: vaultName()! })}
          onClick={props.onSettings}
        >
          <span class="rail-vault-dot">{vaultName()!.slice(0, 1)}</span>
        </button>
      </Show>
      <button
        class="rail-item"
        title={t("nav.settings")}
        aria-label={t("nav.settings")}
        onClick={props.onSettings}
      >
        <Icon name="sliders" size={18} />
      </button>
    </nav>
  );
}
