import { render } from "solid-js/web";
import { App } from "./App";
import { QuickCaptureApp } from "./capture/QuickCaptureApp";
import { locale } from "./i18n";
import "./styles.css";

// 启动即把 <html lang> 对齐当前 locale（index.html 默认 zh-CN，此处按检测/记忆值覆盖）。
document.documentElement.lang = locale() === "zh" ? "zh-CN" : "en";

// v0.7 FR-CAP-01：全局快捷键浮窗走同一 dist 的 #quick-capture 路由
//（兜底也认 ?window=quick-capture，防 Url::join 对 fragment 的平台差异）。
const isQuickCapture =
  location.hash.includes("quick-capture") ||
  new URLSearchParams(location.search).get("window") === "quick-capture";

render(
  () => (isQuickCapture ? <QuickCaptureApp /> : <App />),
  document.getElementById("root")!,
);
