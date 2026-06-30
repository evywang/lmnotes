import { render } from "solid-js/web";
import { App } from "./App";
import { locale } from "./i18n";
import "./styles.css";

// 启动即把 <html lang> 对齐当前 locale（index.html 默认 zh-CN，此处按检测/记忆值覆盖）。
document.documentElement.lang = locale() === "zh" ? "zh-CN" : "en";

render(() => <App />, document.getElementById("root")!);
