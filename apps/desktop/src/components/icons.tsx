/**
 * 内联 SVG 图标（v1.0，spec §5）：24×24 / stroke 1.5 / currentColor。
 * 几何形态派生自 Lucide（ISC License），内联零依赖；替代跨平台渲染不一致的 Emoji。
 * 用法：<Icon name="pencil" size={16} />
 */
import type { JSX } from "solid-js";

const PATHS = {
  pencil: '<path d="M4 20l1-4L16 5l3 3L8 19l-4 1z"/><path d="M13 8l3 3"/>',
  calendar:
    '<rect x="4" y="5" width="16" height="15" rx="2"/><path d="M8 3v4M16 3v4M4 11h16"/>',
  clock: '<circle cx="12" cy="12" r="8"/><path d="M12 8v4l3 2"/>',
  network:
    '<circle cx="5" cy="6" r="2"/><circle cx="19" cy="6" r="2"/><circle cx="12" cy="18" r="2"/><path d="M6.5 7.3l4.3 9.1M17.5 7.3l-4.3 9.1M7 6h10"/>',
  spark:
    '<path d="M12 3l1.7 4.6L18.3 9.3l-4.6 1.7L12 15.6l-1.7-4.6L5.7 9.3l4.6-1.7L12 3z"/><path d="M18.8 15.2l.7 1.9 1.9.7-1.9.7-.7 1.9-.7-1.9-1.9-.7 1.9-.7.7-1.9z"/>',
  tasks:
    '<path d="M4 5.5l1.2 1.2 2.3-2.3M4 11.5l1.2 1.2 2.3-2.3M4 17.5l1.2 1.2 2.3-2.3M10.5 6H20M10.5 12H20M10.5 18H20"/>',
  sliders:
    '<path d="M4 6h16M4 12h16M4 18h16"/><circle cx="9" cy="6" r="2"/><circle cx="15" cy="12" r="2"/><circle cx="7" cy="18" r="2"/>',
  book: '<path d="M5 4h12a2 2 0 012 2v14H7a2 2 0 01-2-2V4z"/><path d="M9 4v14"/>',
  search: '<circle cx="11" cy="11" r="6"/><path d="M16.5 16.5L21 21"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0014 0M12 18v3"/>',
  import:
    '<path d="M12 3v10M8 9l4 4 4-4M4 17v2a2 2 0 002 2h12a2 2 0 002-2v-2"/>',
  tag: '<path d="M3 11V4a1 1 0 011-1h7l10 10-8 8L3 11z"/><circle cx="7.5" cy="7.5" r="1.2"/>',
  folder:
    '<path d="M3 6a2 2 0 012-2h4l2 2h8a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V6z"/>',
  file: '<path d="M6 3h8l4 4v13a1 1 0 01-1 1H6a1 1 0 01-1-1V4a1 1 0 011-1z"/><path d="M14 3v4h4"/>',
  "chevron-down": '<path d="M6 9l6 6 6-6"/>',
  "chevron-right": '<path d="M9 6l6 6-6 6"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  history: '<path d="M3 12a9 9 0 109-9 9 9 0 00-6.4 2.6L3 8"/><path d="M12 7v5l3 2"/>',
  eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/>',
  send: '<path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>',
  check: '<path d="M4 12l5 5L20 6"/>',
  trash:
    '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6"/>',
  alert: '<path d="M12 3L2 20h20L12 3z"/><path d="M12 9v5M12 17.5v.5"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 8v.5M12 11v6"/>',
  swatch:
    '<circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 010 18V3z" fill="currentColor" stroke="none"/>',
} as const;

export type IconName = keyof typeof PATHS;
/** edit 与 pencil 同形（spec §5 清单中的别名）。 */
const ALIASES: Record<string, IconName> = { edit: "pencil" };

export function Icon(props: {
  name: IconName;
  size?: number;
  class?: string;
}): JSX.Element {
  const key = () => (ALIASES[props.name] ?? props.name) as IconName;
  return (
    <svg
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      class={props.class}
      aria-hidden="true"
      innerHTML={PATHS[key()]}
    />
  );
}
