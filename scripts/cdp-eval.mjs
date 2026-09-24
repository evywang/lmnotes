// CDP 烟测脚本：连接 WebView2 调试端口，在页面内执行表达式并打印结果
// 用法: node scripts/cdp-eval.mjs "<expression>"
const expr = process.argv[2];
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.find((t) => t.type === "page");
if (!page) {
  console.error("NO_PAGE");
  process.exit(1);
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
const timeout = setTimeout(() => {
  console.error("TIMEOUT");
  process.exit(1);
}, 15000);
await new Promise((r, j) => {
  ws.onopen = r;
  ws.onerror = () => j(new Error("ws error"));
});
ws.send(
  JSON.stringify({
    id: 1,
    method: "Runtime.evaluate",
    params: { expression: expr, returnByValue: true, awaitPromise: true },
  }),
);
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id === 1) {
    clearTimeout(timeout);
    console.log(JSON.stringify(msg.result?.result?.value ?? msg.result, null, 1));
    ws.close();
    process.exit(0);
  }
};
