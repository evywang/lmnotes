# LMNotes MCP Agent 接入指南

LMNotes 把你的笔记 vault（`~/.lmnotes/default`，OKF markdown）通过一个 **MCP server**
只读暴露给 AI agent（Claude Desktop、Cursor、ZCode、Cline 等支持 MCP 的 host）。

- **接入方式**：MCP，transport 为 **Streamable HTTP**（绑定 `127.0.0.1`）。
- **能力范围**：**只读**。agent 可检索 / 读取 / 问答 / 查链接图，但**不能修改笔记**。
- **运行形态**：桌面进程**内嵌**——启动 LMNotes 桌面端即自动拉起 MCP server，无需独立进程。
  因此桌面端需保持运行。

> 为什么是 HTTP 而非 stdio？stdio 要求 host 自己 spawn 独立可执行子进程，与「桌面端内嵌」
> 冲突；且 SQLite/Tantivy 不支持跨进程并发写句柄，内嵌直接复用桌面已打开的句柄，零锁竞争、
> 数据始终一致。主流 host 已支持 streamable HTTP transport。

## 1. 发现文件 `~/.lmnotes/mcp.json`

桌面端启动时会写入发现文件（仅属主可读，Unix `0600`），agent 据此接入：

```json
{
  "url": "http://127.0.0.1:21920/mcp",
  "token": "<64 位 hex>",
  "transport": "http",
  "tools": ["search_notes", "read_note", "list_notes", "ask_vault", "get_note_links"],
  "vault_root": "/home/you/.lmnotes/default"
}
```

- `url` —— MCP 端点。端口默认 `21920`；若被占用则自动退到 OS 分配端口，以实际值为准。
- `token` —— Bearer token。请求时放 `Authorization: Bearer <token>`，缺失或不符返回 `401`。

## 2. Host 配置示例

### Claude Desktop（`claude_desktop_config.json`）

```json
{
  "mcpServers": {
    "lmnotes": {
      "type": "http",
      "url": "http://127.0.0.1:21920/mcp",
      "headers": {
        "Authorization": "Bearer <粘贴 mcp.json 的 token>"
      }
    }
  }
}
```

### Cursor / ZCode / 其它支持 streamable HTTP 的 host

填入同样的 `url` 与 `Authorization: Bearer <token>` 头即可。

> 若 host 仅支持 stdio：当前内嵌形态不直接兼容。可用下方「附录：零代码兜底方案」直接读文件，
> 或后续按需新增独立 stdio 二进制（处理逻辑已 transport 无关，加 10 行胶水即可）。

## 3. 工具清单

| 工具 | 入参 | 出参 | 说明 |
|---|---|---|---|
| `search_notes` | `query: string`, `limit?: int`（默认 20） | `[{path, title, score}]` | 全文检索（BM25，支持中文分词） |
| `read_note` | `path: string`（vault 相对路径） | `{text}` | 读单条笔记原文（含 frontmatter + markdown） |
| `list_notes` | `rel_path?: string` | 目录树 `[{name, path, is_dir, children}]` | 列 vault 目录树（跳过 `.lmnotes/` 与隐藏项，仅 `.md`） |
| `ask_vault` | `query: string`, `history?: [{role, content}]` | `{answer, citations:[{index, path}]}` | RAG 问答（向量+全文 RRF → LLM）。需已配置可用 LLM provider |
| `get_note_links` | `path: string` | `[{src_path, link_text}]` | 反向链接：哪些笔记链接到目标笔记 |

路径约定：`path` 均为 **vault 相对路径**，如 `notes/ai/attention.md`。

## 4. 配置（`~/.lmnotes/config.json`）

在顶层加 `mcp` 段（向后兼容，缺省即默认开启）：

```json
{
  "mcp": {
    "enabled": true,
    "port": 21920,
    "token": null
  }
}
```

- `enabled`（默认 `true`）—— 设为 `false` 可关闭 MCP server。
- `port`（默认 `21920`）—— `0` 表示由 OS 分配空闲端口。
- `token`（默认 `null`）—— 固定 token；`null` 则每次启动随机生成并写入 `mcp.json`。
  多 host 复用时建议填一个固定值，避免每次重启都变。

修改后重启桌面端生效。

## 5. 安全与护栏

- 仅绑定 `127.0.0.1`，不对外网暴露；附 Bearer token；发现文件仅属主可读。
- `ask_vault` 会调用 LLM，沿用 LMNotes 的隐私护栏（`guard`）：
  - `cloud_allowed` —— 未开启时，云端 provider 被拒绝（仅允许本地 provider 如 Ollama）。
  - `sensitive_patterns` —— 命中敏感词的内容不会发给云端 LLM。
- 其余 4 个工具（`search_notes`/`read_note`/`list_notes`/`get_note_links`）**不依赖 LLM**，
  即使没配 provider 也可用。

## 6. 集成自测（curl）

```bash
# 取 url 与 token
cat ~/.lmnotes/mcp.json

# 1. initialize
curl -s -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}},"id":1}' \
  http://127.0.0.1:<port>/mcp

# 2. 列工具
curl -s -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/list","id":2}' \
  http://127.0.0.1:<port>/mcp

# 3. 调用 search_notes
curl -s -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"search_notes","arguments":{"query":"注意力","limit":5}},"id":3}' \
  http://127.0.0.1:<port>/mcp
```

## 附录：零代码兜底方案（仅文件系统访问）

无需 MCP，agent 也可直接读文件——LMNotes vault 就是纯 markdown 目录：

```
~/.lmnotes/default/          ← vault 根（OKF Bundle）
├── index.md                 ← 根索引（声明 okf_version）
├── notes/**/*.md            ← 笔记（YAML frontmatter + markdown）
├── assets/img/...           ← 图片（内容寻址）
└── .lmnotes/                ← 应用内部数据（索引/向量/配置，agent 无需读）
```

- 笔记格式见 `docs/okf/SPEC.v0.1.md`：YAML frontmatter（`type` 必填，`id`/`title`/`tags` 可选）
  + markdown 正文 + 标准链接 `[文本](/相对路径.md)`。
- LMNotes 的 `notify` 文件监听器会自动把外部对 `.md` 的改动重新索引——agent 写回笔记后，
  桌面端的搜索/向量会自动更新。

适合仅有文件系统访问的 agent（含部分 MCP host 的 Bash 工具）。
