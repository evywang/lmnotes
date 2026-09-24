/**
 * 主题引擎（v1.0 spec §4.3）：主题 = 对 19 个颜色令牌的声明式 JSON 覆盖。
 * - 内置 4 主题（石墨暗/亮、暖纸、墨黑）+ 用户目录 ~/.lmnotes/themes/*.theme.json
 *   （经 Rust list_themes 扫描，校验在此做：插件是数据不是代码）。
 * - 应用 = documentElement 批量 setProperty + data-theme-mode / data-theme-id，
 *   切换即时生效；持久化 localStorage lmnotes.theme（"auto" 跟随系统，默认）。
 * - 缺省颜色键回退到同 mode 的石墨预设（部分覆盖合法）。
 */
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { MessageKey } from "../i18n";

export type ThemeMode = "light" | "dark";

export const COLOR_KEYS = [
  "surface-0", "surface-1", "surface-2", "surface-inset", "surface-hover", "surface-active",
  "border-subtle", "border-default", "border-strong",
  "text-1", "text-2", "text-3",
  "accent", "accent-hover", "accent-fg", "accent-soft",
  "success", "warning", "danger",
] as const;
export type ColorKey = (typeof COLOR_KEYS)[number];
export type ThemeColors = Record<ColorKey, string>;

export interface ThemeDef {
  id: string;
  /** 内置主题的 i18n 键（显示时本地化）；用户主题直接用 name 原文。 */
  nameKey?: MessageKey;
  name: string;
  mode: ThemeMode;
  colors: Partial<ThemeColors>;
  builtin?: boolean;
}

// 与 styles.tokens.css :root / [data-theme-mode="light"] 保持同步（TS 侧为首帧前与回退真值）
const GRAPHITE_DARK: ThemeColors = {
  "surface-0": "#101014", "surface-1": "#16161b", "surface-2": "#1c1c23",
  "surface-inset": "#0c0c10", "surface-hover": "#22232a", "surface-active": "#2a2b34",
  "border-subtle": "#232329", "border-default": "#2c2c34", "border-strong": "#3a3a44",
  "text-1": "#dcdde3", "text-2": "#a3a5b1", "text-3": "#6f7280",
  "accent": "#7c9cf5", "accent-hover": "#8facf7", "accent-fg": "#0f1016",
  "accent-soft": "rgba(124, 156, 245, 0.14)",
  "success": "#6dbf8b", "warning": "#e0b352", "danger": "#e07a7a",
};

// 与 styles.tokens.css :root / [data-theme-mode="light"] 保持同步（TS 侧为首帧前与回退真值）
const GRAPHITE_LIGHT: ThemeColors = {
  "surface-0": "#f0f1f4", "surface-1": "#fafafa", "surface-2": "#ffffff",
  "surface-inset": "#eef0f3", "surface-hover": "#eceef2", "surface-active": "#e2e5ec",
  "border-subtle": "#e6e7eb", "border-default": "#d9dbe2", "border-strong": "#c4c7d1",
  "text-1": "#26282e", "text-2": "#5c5f6a", "text-3": "#8d919e",
  "accent": "#3b66d8", "accent-hover": "#4a74de", "accent-fg": "#ffffff",
  "accent-soft": "rgba(59, 102, 216, 0.1)",
  "success": "#2f9e5f", "warning": "#b07d1a", "danger": "#cc4b4b",
};

/** 内置主题（编译进代码，永不可删）。 */
export const BUILTIN_THEMES: ThemeDef[] = [
  { id: "graphite-dark", nameKey: "theme.graphiteDark", name: "Graphite Dark", mode: "dark", colors: GRAPHITE_DARK, builtin: true },
  { id: "graphite-light", nameKey: "theme.graphiteLight", name: "Graphite Light", mode: "light", colors: GRAPHITE_LIGHT, builtin: true },
  {
    id: "warm-paper", nameKey: "theme.warmPaper", name: "Warm Paper", mode: "light", builtin: true,
    colors: {
      "surface-0": "#f7f3ec", "surface-1": "#efe9df", "surface-2": "#fffdf8",
      "surface-inset": "#e8e1d4", "surface-hover": "#e9e2d5", "surface-active": "#e2dacb",
      "border-subtle": "#ddd4c6", "border-default": "#d2c7b5", "border-strong": "#c0b29c",
      "text-1": "#33302b", "text-2": "#5f5a51", "text-3": "#8d877c",
      "accent": "#a3543a", "accent-hover": "#b26044", "accent-fg": "#ffffff",
      "accent-soft": "rgba(163, 84, 58, 0.12)",
    },
  },
  {
    id: "ink-black", nameKey: "theme.inkBlack", name: "Ink Black", mode: "dark", builtin: true,
    colors: {
      "surface-0": "#0a0a0a", "surface-1": "#121212", "surface-2": "#191919",
      "surface-inset": "#0e0e0e", "surface-hover": "#1f1f1f", "surface-active": "#262626",
      "border-subtle": "#1f1f1f", "border-default": "#2a2a2a", "border-strong": "#3a3a3a",
      "text-1": "#e5e5e5", "text-2": "#a3a3a3", "text-3": "#6f6f6f",
      "accent": "#4ade80", "accent-hover": "#66e99a", "accent-fg": "#05230f",
      "accent-soft": "rgba(74, 222, 128, 0.14)",
    },
  },
];

const ID_RE = /^[a-z0-9-]+$/;
const COLOR_RE = /^(#[0-9a-fA-F]{6}|rgba\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*(,\s*(0|1|0?\.\d+)\s*)?\))$/;

/** 校验单个用户主题文件；非法返回 null（调用方收集文件名做警示）。 */
export function parseTheme(json: string): Omit<ThemeDef, "nameKey" | "builtin"> | null {
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null) return null;
  const o = raw as Record<string, unknown>;
  if (typeof o.id !== "string" || !ID_RE.test(o.id)) return null;
  if (typeof o.name !== "string" || o.name.length === 0 || o.name.length > 16) return null;
  if (o.mode !== "light" && o.mode !== "dark") return null;
  const colors: Partial<ThemeColors> = {};
  if (o.colors !== undefined) {
    if (typeof o.colors !== "object" || o.colors === null) return null;
    for (const [k, v] of Object.entries(o.colors as Record<string, unknown>)) {
      if ((COLOR_KEYS as readonly string[]).includes(k) && typeof v === "string" && COLOR_RE.test(v)) {
        colors[k as ColorKey] = v;
      }
    }
  }
  return { id: o.id, name: o.name, mode: o.mode, colors };
}

/** 部分覆盖回退到同 mode 石墨预设（spec §4.3）。 */
export function resolveColors(theme: Pick<ThemeDef, "mode" | "colors">): ThemeColors {
  const base = theme.mode === "light" ? GRAPHITE_LIGHT : GRAPHITE_DARK;
  return { ...base, ...theme.colors } as ThemeColors;
}

// ── 全局信号 ──────────────────────────────────────────────
const [userThemes, setUserThemes] = createSignal<ThemeDef[]>([]);
/** 解析失败的用户主题文件名（设置·外观里标警示）。 */
const [themeWarnings, setThemeWarnings] = createSignal<string[]>([]);
/** "auto"（默认）或主题 id。 */
const [currentThemeId, setCurrentThemeId] = createSignal<string>(
  (() => {
    try {
      return localStorage.getItem("lmnotes.theme") ?? "auto";
    } catch {
      return "auto";
    }
  })(),
);

export { userThemes, themeWarnings, currentThemeId };

export function allThemes(): ThemeDef[] {
  return [...BUILTIN_THEMES, ...userThemes()];
}

function findById(id: string): ThemeDef | undefined {
  return allThemes().find((t) => t.id === id);
}

function systemPrefersLight(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ?? false;
}

/** 把指定主题（或 auto 解析结果）写入 DOM。 */
function applyTheme(): void {
  const id = currentThemeId();
  let theme: ThemeDef;
  if (id === "auto") {
    theme = systemPrefersLight() ? findById("graphite-light")! : findById("graphite-dark")!;
  } else {
    theme = findById(id) ?? BUILTIN_THEMES[0];
  }
  const root = document.documentElement;
  root.dataset.themeMode = theme.mode;
  root.dataset.themeId = theme.id;
  const colors = resolveColors(theme);
  for (const k of COLOR_KEYS) {
    root.style.setProperty(`--${k}`, colors[k]);
  }
}

/** 切换主题：写 signal + localStorage + 立即应用。未知 id 回退 auto。 */
export function setTheme(id: string): void {
  const safe = id === "auto" || findById(id) ? id : "auto";
  setCurrentThemeId(safe);
  try {
    localStorage.setItem("lmnotes.theme", safe);
  } catch {
    /* localStorage 不可用时仅会话内生效 */
  }
  applyTheme();
}

/** 加载用户主题目录（list_themes 失败静默——只影响可选主题，不影响启动）。 */
export async function loadUserThemes(): Promise<void> {
  try {
    const files = await invoke<{ file: string; json: string }[]>("list_themes");
    const ok: ThemeDef[] = [];
    const bad: string[] = [];
    const seen = new Set(BUILTIN_THEMES.map((t) => t.id));
    for (const f of files) {
      const parsed = parseTheme(f.json);
      if (!parsed || seen.has(parsed.id)) {
        bad.push(f.file);
        continue;
      }
      seen.add(parsed.id);
      ok.push({ ...parsed, builtin: false });
    }
    setUserThemes(ok);
    setThemeWarnings(bad);
    // 用户主题就位后重应用：启动早期 findById 未命中已回退内置主题
    applyTheme();
  } catch (e) {
    /* 命令不可用（如单测环境）→ 无用户主题 */
    console.warn("list_themes failed", e);
  }
}

/**
 * 启动初始化（main.tsx 在 render 前调）：应用当前主题 + auto 模式监听系统切换。
 * QuickCapture 小窗走同一 main.tsx，自动同享主题。
 */
export function initTheme(): void {
  applyTheme();
  // 无条件注册（回调内判 auto）：用户中途切回「跟随系统」依然实时生效
  window
    .matchMedia?.("(prefers-color-scheme: light)")
    ?.addEventListener("change", () => {
      if (currentThemeId() === "auto") applyTheme();
    });
}
