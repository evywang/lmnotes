<div align="center">

# LMNotes

**本地优先、LLM 原生的个人知识应用**

你的数据永远是你的 · AI 不是插件而是引擎 · 多模态一处收纳

[功能特性](#-功能特性) · [快速开始](#-快速开始) · [文档](#-文档) · [架构](#-架构) · [许可](#-许可)

</div>

---

## ✨ 简介

LMNotes 是一个 **local-first、LLM-native** 的笔记应用。它用一套机器与人都能直接读写的人类可读格式——**Open Knowledge Format (OKF)**——把笔记组织成一个**自我组织的 wiki**,并由可插拔的 LLM 持续为其建立连接、生成摘要、回答提问。

与传统笔记软件的「AI 外挂」不同,LMNotes 的 LLM 是知识的引擎:它能扫描全库、自动发现笔记间的语义关联、维护双向链接、生成摘要,并对整个知识库做 RAG 问答——所有原始数据都以纯 Markdown 落盘,**无锁定、可 git、可迁移**。

> 📖 完整产品定位与设计理念见 [`docs/specs/PRD.md`](docs/specs/PRD.md) §1–§2。

## 🌟 功能特性

| 能力 | 说明 |
|---|---|
| **Markdown 原生编辑** | 基于 CodeMirror 的编辑器,实时编辑/预览,标准 CommonMark + GFM |
| **OKF 格式落盘** | 每条笔记 = 一个 `concept`,纯 Markdown + YAML frontmatter,开放规范、零锁定 |
| **本地优先存储** | 所有数据是磁盘上的纯文件,vault = 一个目录树,删除派生索引后可完全重建 |
| **三层混合索引** | Tantivy 全文 + sqlite-vec 向量 + SQLite 结构化,RRF 融合检索(ADR-0003) |
| **LLM 智能建议** | 自动生成摘要/标签建议,带溯源与可回滚,用户一键接受或批量接受 |
| **选段改写** | 选中正文调用 LLM 改写,支持润色 / 扩写 / 翻译为英文 / 总结要点 |
| **Chat with Vault** | 基于全库 RAG 的问答,引用真实笔记片段作答 |
| **MCP Agent 接入** | 桌面端内嵌只读 MCP server,Claude/Cursor/ZCode 等 host 可检索问答你的 vault |
| **可插拔 Provider** | 支持 Ollama(全本地)与 OpenAI 兼容接口,带隐私护栏与按条目授权 |
| **文件树管理** | 拖拽移动、右键菜单(新建/删除/在资源管理器中显示)、文件夹组织 |
| **快速捕获** | 全局快捷键弹窗,随手记下素材即时归位 |

## 🚀 快速开始

### 前置要求

| 工具 | 版本 | 说明 |
|---|---|---|
| **Rust** | stable | 由 [`rust-toolchain.toml`](rust-toolchain.toml) 锁定 |
| **Node.js** | ≥ 20 | 前端构建 |
| **WebView2** | 预装 | Windows 10/11 通常已自带 |

**Linux** 额外需要(见 release.yml):
```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev build-essential
```

### 从源码构建运行

```bash
# 克隆
git clone https://github.com/evywang/lmnotes.git
cd lmnotes

# 安装前端依赖
cd apps/desktop
npm install

# 开发模式(热重载)
npm run tauri dev

# 生产构建(产出 .exe / .deb 等安装包)
npm run tauri:build
```

构建产物位于 `apps/desktop/src-tauri/target/release/bundle/`。

### 仅运行核心库 / CLI

```bash
# 运行全部测试(核心库无 UI 依赖,可独立 cargo test)
cargo test --workspace --all-targets

# 使用 CLI(OKF Validator 等调试命令)
cargo run -p lmnotes-cli -- --help
```

## 📚 文档

| 文档 | 说明 |
|---|---|
| [产品需求规格 (PRD)](docs/specs/PRD.md) | 完整的产品定位、功能清单、设计决策 |
| [使用手册](docs/user-manual.md) | 界面操作、快捷键、FAQ |
| [OKF 规范 v0.1](docs/okf/SPEC.v0.1.md) | LMNotes 遵循的开放知识格式(本地存档) |
| [MCP Agent 接入指南](docs/mcp-agent-integration.md) | 让 Claude/Cursor 等 host 读写你的 vault |
| [MCP API 参考](docs/mcp-api.md) | MCP server 工具清单与协议细节 |
| [架构决策记录 (ADR)](docs/adr/) | 5 篇 ADR:核心边界、三层索引、前端栈、Provider 护栏 |

## 🏗️ 架构

LMNotes 采用 **Tauri** 架构:Rust 核心 `lmnotes-core` + Web UI + Tauri 壳,核心业务逻辑与 UI 严格解耦,保证跨端复用。

```
┌─────────────────────────────────────────────┐
│  Web UI(SolidJS + CodeMirror,独立构建)      │  ← 视图与交互
├─────────────────────────────────────────────┤
│  Tauri IPC 层(薄)                           │  ← 命令注册、事件、权限白名单
├─────────────────────────────────────────────┤
│  lmnotes-core(Rust,可独立测试)              │  ← 所有业务逻辑
│   ├─ okf       OKF 解析/校验/ID/Validator    │
│   ├─ backend   Vault/文件 IO/资源去重        │
│   ├─ index     Tantivy/sqlite-vec/SQLite     │
│   ├─ llm       Provider 抽象/路由/护栏       │
│   ├─ qa        RAG 检索/上下文/提示           │
│   └─ mcp       内嵌只读 MCP server           │
├─────────────────────────────────────────────┤
│  Tauri Runtime + 平台原生能力                │
└─────────────────────────────────────────────┘
```

### 仓库结构

```
lmnotes/
├── crates/
│   ├── lmnotes-core/      # 业务核心(无 UI 依赖,纯 cargo test)
│   ├── lmnotes-cli/       # 调试/Validator CLI
│   └── lmnotes-mcp/       # MCP server 实现(transport 无关)
├── apps/
│   └── desktop/           # Tauri 桌面应用(Rust 壳 + SolidJS 前端)
├── docs/                  # PRD / ADR / OKF / 手册
└── .github/workflows/     # CI(fmt+clippy+test) + Release(tag 触发)
```

> 硬边界:`lmnotes-core` 禁止直接 `use std::fs`,所有 IO 经 `StorageBackend` trait——由 CI clippy 强制。这是「未来跨端核心零改动」承诺的技术基础(见 [ADR-0002](docs/adr/ADR-0002-tauri-core-boundary.md))。

## 🔒 隐私与 Provider

- **全本地优先**:LLM 可完全跑在本地(Ollama),敏感数据不出本机。
- **按条目授权**:上云 Provider(OpenAI 兼容)需显式授权,带隐私护栏(ADR-0005)。
- **MCP 只读**:暴露给外部 agent 的 MCP server 是**只读**的,agent 不能修改你的笔记。

## 🛠️ 开发

```bash
# 质量门禁(与 CI 一致)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets

# 前端检查
cd apps/desktop && npm ci && npx tsc --noEmit && npm run build
```

提交规范采用 Conventional Commits(`feat:` / `fix:` / `docs:` / `ci:` 等)。

## 📦 发布

推送 `v*` 标签即触发 [Release 工作流](.github/workflows/release.yml),自动构建 Windows(NSIS)与 Linux(deb)安装包并草拟 GitHub Release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 📄 许可

双授权,任选其一:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

除非另有声明,所有贡献均按上述双授权许可。

<div align="center">

<sub>Built with Rust · Tauri · SolidJS · CodeMirror · Tantivy</sub>

</div>
