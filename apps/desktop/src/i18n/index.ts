/**
 * 轻量 i18n：零依赖，沿用项目 createSignal 模式。
 *
 * - locale() 是响应式 signal，在 SolidJS JSX 中调用 t() 会自动随语言切换重渲染。
 * - 持久化到 localStorage（键 lmnotes.locale）；首次启动按 navigator.language 检测。
 * - 同步 <html lang> 便于无障碍 / 字体回退。
 *
 * 字典见 ./locales/{en,zh}.ts；新增 key 先加 en.ts，再补 zh.ts（类型约束防漏译）。
 */
import { createSignal } from "solid-js";
import { en, type MessageKey } from "./locales/en";
import { zh } from "./locales/zh";

export type { MessageKey };
export type Locale = "zh" | "en";

const STORAGE_KEY = "lmnotes.locale";

/** 消息表，按 locale 取。en 为兜底真值。 */
const messages: Record<Locale, Record<MessageKey, string>> = { en, zh };

/** <html lang> 用值。 */
const HTML_LANG: Record<Locale, string> = { zh: "zh-CN", en: "en" };

/** 检测初始 locale：localStorage 优先，否则按浏览器语言（zh* → 中文）。 */
function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "zh" || saved === "en") return saved;
  } catch {
    // localStorage 不可用时忽略（如禁用 cookie / SSR）
  }
  const nav = navigator.language?.toLowerCase() ?? "";
  return nav.startsWith("zh") ? "zh" : "en";
}

const [locale, setLocaleSignal] = createSignal<Locale>(detect());

export { locale };

/** 设置当前语言：写 signal + localStorage + <html lang>。切换即时生效。 */
export function setLocale(l: Locale): void {
  setLocaleSignal(l);
  try {
    localStorage.setItem(STORAGE_KEY, l);
  } catch {
    // 忽略写入失败（仍保留本次会话的语言）
  }
  document.documentElement.lang = HTML_LANG[l];
}

/**
 * 取翻译文本，替换 {name} 占位符。
 *
 * t("filetree.deleteConfirm", { name }) → "确定删除 \"X\"？…" / "Delete \"X\"? …"
 * 在 SolidJS 响应式上下文中调用即可随 locale() 切换重渲染。
 */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const dict = messages[locale()] ?? en;
  let text = dict[key] ?? en[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
    }
  }
  return text;
}
