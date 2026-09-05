/**
 * 标签云（FR-SEARCH-05，v0.7）：侧栏只读区块，列出全部标签及笔记数。
 * 点击标签 → 由 App 打开按标签过滤的列表视图（TimelineView 复用）。
 */
import { createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

interface TagCountDto {
  tag: string;
  count: number;
}

export function TagCloud(props: { onPick: (tag: string) => void }) {
  const [tags, setTags] = createSignal<TagCountDto[]>([]);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  const refresh = async () => {
    try {
      setTags(await invoke<TagCountDto[]>("list_tags"));
    } catch (e) {
      console.error("list_tags", e);
    }
  };

  onMount(() => {
    void refresh();
    // 低频兜底刷新（标签随索引变化；主入口是重新打开侧栏/窗口时自然重挂载）
    refreshTimer = setInterval(() => void refresh(), 60_000);
    return () => refreshTimer && clearInterval(refreshTimer);
  });

  return (
    <div class="tag-cloud">
      <div class="palette-section">{t("tags.section")}</div>
      <Show when={tags().length > 0} fallback={<p class="muted small">{t("tags.empty")}</p>}>
        <div class="tag-pills">
          <For each={tags()}>
            {(tc) => (
              <button class="tag-pill" onClick={() => props.onPick(tc.tag)}>
                {tc.tag}
                <span class="tag-count">{tc.count}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
