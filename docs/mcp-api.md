# LMNotes MCP 接口说明

> LMNotes 内嵌一个**只读** MCP（Model Context Protocol）server，把你的笔记 vault
> 暴露给 AI agent（Claude Desktop、Cursor、ZCode、Cline 等支持 MCP 的 host）。
>
> 本文是**接口规范**。关于如何把 host 接入、发现文件含义、配置项，见
> [MCP 接入指南](./mcp-agent-integration.md)。

---

## 目录

- [1. 概览](#1-概览)
- [2. Transport 与鉴权](#2-transport-与鉴权)
- [3. 通用约定](#3-通用约定)
- [4. 工具一览](#4-工具一览)
- [5. 工具详解](#5-工具详解)
  - [5.1 `search_notes`](#51-search_notes)
  - [5.2 `read_note`](#52-read_note)
  - [5.3 `list_notes`](#53-list_notes)
  - [5.4 `ask_vault`](#54-ask_vault)
  - [5.5 `get_note_links`](#55-get_note_links)
- [6. JSON-RPC 交互示例](#6-json-rpc-交互示例)
- [7. 错误处理](#7-错误处理)
- [8. 能力声明（`initialize`）](#8-能力声明initialize)
- [9. 设计与限制](#9-设计与限制)

---

## 1. 概览

| 项目 | 值 |
|---|---|
| 协议 | MCP（Model Context Protocol），Streamable HTTP transport |
| 端点 | `http://127.0.0.1:<port>/mcp`（仅 loopback，默认端口 `21920`） |
| 鉴权 | `Authorization: Bearer <token>` |
| 能力 | **只读**：检索 / 读取 / 列目录 / RAG 问答 / 反向链接 |
| 工具数 | 5 个 |
| 生命周期 | 由桌面进程内嵌，桌面端需保持运行 |
| 实现版本 | `lmnotes-mcp`（见 `crate` 的 `CARGO_PKG_VERSION`） |

**为什么只读**：把风险面降到最低——agent 可读、可问、可遍历图谱，但**不能创建/修改/删除笔记**。`ask_vault` 仅写入应用内部的 `chat_history`（与桌面 Chat 行为一致），不触碰你的笔记文件。

**为什么是 HTTP 而非 stdio**：stdio 要求 host 自己 spawn 独立可执行子进程，与「桌面端内嵌」冲突；且 SQLite/Tantivy 不支持跨进程并发写句柄，内嵌直接复用桌面已打开的句柄，零锁竞争、数据始终一致。

---

## 2. Transport 与鉴权

### 2.1 端点与端口

- 端点固定为 `/mcp`。
- 默认端口 `21920`；若被占用，桌面端会自动退到 OS 分配端口（`:0`）。
- **实际 url 与 token 见发现文件** `~/.lmnotes/mcp.json`（每次启动更新）：

```json
{
  "url": "http://127.0.0.1:21920/mcp",
  "token": "<64 位 hex>",
  "transport": "http",
  "tools": ["search_notes", "read_note", "list_notes", "ask_vault", "get_note_links"],
  "vault_root": "/home/you/.lmnotes/default"
}
```

### 2.2 Bearer 鉴权

所有请求必须带：

```
Authorization: Bearer <token>
```

- token 缺失或不符 → 返回 `401 Unauthorized`。
- token 默认每次启动随机生成（64 位 hex）；可在 `~/.lmnotes/config.json` 的 `mcp.token` 固定。
- 仅绑 `127.0.0.1`，不对外网暴露；发现文件仅属主可读（Unix `0600`）。

---

## 3. 通用约定

### 3.1 路径约定

工具参数中的 `path` 一律为 **vault 相对路径**，正斜杠分隔，如：

```
notes/ai/attention.md
notes/daily/2026-06-28.md
```

`read_note` / `get_note_links` 的 `path` 即此相对路径。路径穿越（`../`）会被拒绝（见 7.3）。

### 3.2 数据类型

- 所有请求/响应均为 **JSON**（`Content-Type: application/json`）。
- 可空字段：`title`、`link_text` 等可能为 `null`。
- `score`：浮点相关度，越大越相关。
- 时间戳：`ask_vault` 内部用 unix 时间戳，但接口不直接暴露。

### 3.3 工具结果封装

MCP `tools/call` 返回的 `result.content` 是一段文本（JSON 字符串）。本 server 把结构化结果序列化成 JSON 文本返回，其根为 object。下面「响应」部分给出的是该 JSON 文本解析后的结构。

---

## 4. 工具一览

| 工具 | 作用 | 是否需要 LLM |
|---|---|---|
| [`search_notes`](#51-search_notes) | 全文检索笔记（BM25 + 中文分词） | 否 |
| [`read_note`](#52-read_note) | 读单条笔记原文（markdown） | 否 |
| [`list_notes`](#53-list_notes) | 列 vault 目录树（递归） | 否 |
| [`ask_vault`](#54-ask_vault) | 基于检索的 RAG 问答 | **是** |
| [`get_note_links`](#55-get_note_links) | 反向链接：哪些笔记链接到目标笔记 | 否 |

> 只有 `ask_vault` 依赖 LLM provider。其余 4 个工具即使没配 provider 也可用。

---

## 5. 工具详解

### 5.1 `search_notes`

全文检索笔记。底层为 Tantivy BM25 + jieba 中文分词，并用 SQLite 元数据富化（路径/标题）——与桌面侧边栏搜索同一引擎。

**入参**（`arguments`）：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `query` | string | 是 | 搜索关键词（支持中文分词） |
| `limit` | int | 否 | 返回条数上限，默认 `20`，最大 `200` |

**请求示例**：

```json
{ "query": "注意力机制", "limit": 5 }
```

**响应**：

```json
{
  "hits": [
    {
      "path": "notes/ai/attention.md",
      "title": "注意力机制",
      "score": 3.8125
    },
    {
      "path": "notes/ml/transformer.md",
      "title": null,
      "score": 1.9062
    }
  ]
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `hits[].path` | string | vault 相对路径 |
| `hits[].title` | string \| null | 笔记标题（取自 frontmatter，可能为空） |
| `hits[].score` | number | 相关度（越大越相关） |

> 注：此为**纯全文**检索。向量检索仅用于 `ask_vault`。中文分词的已知限制见使用手册。

---

### 5.2 `read_note`

按 vault 相对路径读取一条笔记的完整原文（含 YAML frontmatter + markdown 正文）。底层用沙箱化的 `FsBackend`，带路径穿越保护。

**入参**：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | vault 相对路径，如 `notes/ai/attention.md` |

**请求示例**：

```json
{ "path": "notes/ai/attention.md" }
```

**响应**：

```json
{
  "text": "---\ntype: note\nid: nt_20260628_1430_AB34\ntitle: 注意力机制\n---\n\n# 注意力机制\n\n注意力是稀缺资源...\n"
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `text` | string | 笔记原文（frontmatter + markdown 正文） |

**错误**：路径不存在或越界 → 调用失败（见 7.3）。

---

### 5.3 `list_notes`

递归列出 vault 目录树。跳过 `.lmnotes/` 与所有隐藏项（`.` 开头），仅含 `.md` 笔记；目录排在文件前，同类按名排序——与桌面文件树规则一致。

**入参**：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `rel_path` | string | 否 | 起始子目录（vault 相对）；缺省为 vault 根 |

**请求示例**：

```json
{ "rel_path": "notes" }
```

或缺省：

```json
{}
```

**响应**（树形结构，顶层为虚拟根）：

```json
{
  "name": "notes",
  "path": "notes",
  "is_dir": true,
  "children": [
    {
      "name": "ai",
      "path": "notes/ai",
      "is_dir": true,
      "children": [
        {
          "name": "attention.md",
          "path": "notes/ai/attention.md",
          "is_dir": false,
          "children": []
        }
      ]
    },
    {
      "name": "daily",
      "path": "notes/daily",
      "is_dir": true,
      "children": [
        {
          "name": "2026-06-28.md",
          "path": "notes/daily/2026-06-28.md",
          "is_dir": false,
          "children": []
        }
      ]
    }
  ]
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | string | 文件/目录名 |
| `path` | string | vault 相对路径 |
| `is_dir` | bool | 是否为目录 |
| `children` | array | 子节点（文件为空数组） |

---

### 5.4 `ask_vault`

基于检索到的笔记回答问题（RAG）。**需要已配置可用的 LLM provider**（embed + chat）。

**工作流**（复刻桌面 `chat_stream`）：

1. 取 Embed provider，把问题向量化。
2. 向量 KNN 召回（top 2K）+ 全文 BM25 召回（top 2K）。
3. **RRF（倒数排名融合，k=60）** 合并，取 top 5 片段。
4. 拼成带 `[1][2]...` 编号引用的上下文（约 6000 字预算，按段落截断）。
5. 护栏检查（沿用桌面配置：`cloud_allowed` / `sensitive_patterns`）。
6. 发给 Chat provider（system 含上下文 + 最近 20 条历史 + 当前问题，温度 0.4）。
7. **流式输出聚合为一次性返回**（MCP tool 一次 call 一个 result）。
8. 把问答落库到 `chat_history`（与桌面 Chat 共享历史）。

**入参**：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `query` | string | 是 | 向 vault 提出的问题 |
| `history` | array | 否 | 多轮对话历史；最近 20 条参与上下文 |
| `history[].role` | string | 是（在 history 内） | `"user"` / `"assistant"` |
| `history[].content` | string | 是（在 history 内） | 该轮内容 |

**请求示例**：

```json
{
  "query": "注意力机制的公式是什么？",
  "history": [
    { "role": "user", "content": "什么是 transformer？" },
    { "role": "assistant", "content": "Transformer 是一种基于自注意力的模型…" }
  ]
}
```

**响应**：

```json
{
  "answer": "注意力机制的核心公式是 Attention(Q,K,V)=softmax(QK^T/√d_k)V [1]...",
  "citations": [
    { "index": 1, "path": "notes/ai/attention.md" },
    { "index": 2, "path": "notes/ml/transformer.md" }
  ]
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `answer` | string | LLM 基于检索到的笔记给出的回答（内含 `[n]` 引用标记） |
| `citations[].index` | int | 引用编号，对应 answer 中的 `[n]` |
| `citations[].path` | string | 该引用所依据的笔记路径 |

**错误**：

- 未配置 / 无可用 embed 或 chat provider → 失败。
- 命中护栏（敏感词，或云端未授权） → 失败并返回拒绝原因。
- 笔记无相关信息时，answer 会说明「我的笔记中暂无相关信息」（非错误）。

> 注：`ask_vault` 会**写入** `chat_history`（user 与 assistant 各一条），与桌面 Chat 共享历史。这不修改你的笔记文件。

---

### 5.5 `get_note_links`

查询**反向链接**：哪些笔记链接到了目标笔记。便于 agent 遍历知识图谱、理解笔记间关联。

> 仅以 `/` 开头的 bundle 相对链接（如 `[文本](/notes/b.md)`）被计为内部边；相对/外部链接被忽略。

**入参**：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 目标笔记的 vault 相对路径 |

**请求示例**：

```json
{ "path": "notes/ai/attention.md" }
```

**响应**：

```json
{
  "backlinks": [
    { "src_path": "notes/ml/transformer.md", "link_text": "注意力机制" },
    { "src_path": "notes/study/nlp.md", "link_text": null }
  ]
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `backlinks[].src_path` | string | 链接到目标的源笔记路径 |
| `backlinks[].link_text` | string \| null | 链接的显示文本（可能为空） |

> 解析：`path` → concept id（优先取 frontmatter 的 `id`，缺省为 path 本身）→ 查 `backrefs(dst_id)` → 富化回源笔记路径。

---

## 6. JSON-RPC 交互示例

MCP 基于 JSON-RPC 2.0。下面是完整的 `curl` 流程（用发现文件里的 url 与 token）。

```bash
TOKEN="<mcp.json 里的 token>"
URL="http://127.0.0.1:<port>/mcp"
```

### 6.1 `initialize`

```bash
curl -s -X POST "$URL" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "initialize",
    "params": {
      "protocolVersion": "2024-11-05",
      "capabilities": {},
      "clientInfo": { "name": "my-agent", "version": "1.0" }
    },
    "id": 1
  }'
```

### 6.2 `tools/list`

```bash
curl -s -X POST "$URL" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{ "jsonrpc": "2.0", "method": "tools/list", "id": 2 }'
```

返回 5 个工具及其 `inputSchema`（由 `schemars` 从 Rust DTO 自动生成）。

### 6.3 `tools/call`

调用 `search_notes`：

```bash
curl -s -X POST "$URL" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "search_notes",
      "arguments": { "query": "注意力", "limit": 5 }
    },
    "id": 3
  }'
```

返回的 `result.content[0].text` 是一段 JSON 文本，解析后即第 5.1 节的响应对象。

---

## 7. 错误处理

### 7.1 HTTP 层

| 状态 | 含义 |
|---|---|
| `200` | 成功（含 JSON-RPC 成功或错误体） |
| `401` | 缺失/无效 Bearer token |

### 7.2 JSON-RPC 层

工具内部失败时，`tools/call` 返回 `result.isError: true`，错误文本在 `result.content[0].text`，例如：

- `"no routing for task Chat"` —— 未配置 Chat provider。
- `"no embed provider for task Embed"` —— 未配置 Embed provider。
- `"cloud not globally authorized"` / `"sensitive pattern matched: ..."` —— 护栏拒绝（`ask_vault`）。
- `"path escapes vault: ../../etc/passwd"` —— 路径越界（`read_note`）。

### 7.3 路径安全

`read_note` 经 `FsBackend` 沙箱化：词法归一化 `.`/`..` 后必须仍在 vault 根之下，否则拒绝。`../../etc/passwd` 这类会失败，不会读出 vault 外文件。

---

## 8. 能力声明（`initialize`）

`initialize` 响应的 `serverInfo` / `capabilities` 大致为：

```json
{
  "serverInfo": {
    "name": "lmnotes-mcp",
    "version": "<crate 版本>"
  },
  "capabilities": {
    "tools": {}
  },
  "instructions": "只读访问 LMNotes vault（笔记知识库）。可用工具：search_notes 全文检索、read_note 读单条笔记原文、list_notes 列目录树、ask_vault 基于 RAG 问答、get_note_links 查反向链接。所有工具均为只读，不会修改笔记。",
  "protocolVersion": "2024-11-05"
}
```

仅声明 `tools` 能力（无 resources / prompts / sampling）。

---

## 9. 设计与限制

### 9.1 设计要点

- **Transport 无关的核心**：工具逻辑在 `crates/lmnotes-mcp` 内只依赖 `lmnotes-core` + `rmcp`，不 import Tauri。当前挂 HTTP transport；将来补独立 stdio 二进制只需少量胶水。
- **零拷贝共享句柄**：server 字段全是 `Arc`，与桌面进程共享同一份已打开的 SQLite / Tantivy 句柄——无跨进程并发写锁竞争，数据始终一致。
- **护栏一致**：`ask_vault` 与桌面 `chat_stream` 走同一套 `guard::check`。

### 9.2 当前限制

1. **只读**：无创建/编辑/删除工具。如需 agent 写回笔记，用「零代码兜底」（见接入指南附录，直接写 `.md`，文件监听器会自动重索引）。
2. **需桌面端运行**：server 进程内嵌，桌面退出即不可用。
3. **`ask_vault` 依赖 LLM**：未配置 provider 时该工具失败，其余 4 个不受影响。
4. **HTTP only**：当前内嵌形态不直接兼容仅支持 stdio 的 host。
5. **`get_note_links` 仅内部链接**：相对/外部链接不计为边。
6. **搜索为纯全文**：`search_notes` 是 BM25；向量检索只在 `ask_vault` 内部使用。
7. **`ask_vault` 写 `chat_history`**：会与桌面 Chat 共享历史（这是设计行为，非笔记修改）。
