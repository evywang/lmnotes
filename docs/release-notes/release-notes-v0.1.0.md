# LMNotes v0.1.0

> **Local-first, LLM-native note app — your data is yours, AI is the engine, multimodal in one vault.**
>
> **本地优先、LLM 原生的笔记应用 —— 你的数据永远是你的,AI 不是插件而是引擎,多模态一处收纳。**

This is the first public release of LMNotes. 🎉

---

## ✨ Highlights / 核心亮点

- **Markdown 原生编辑** — CodeMirror 驱动的实时编辑器,标准 CommonMark + GFM。
- **OKF 格式落盘** — 每条笔记 = 一个 `concept`,纯 Markdown + YAML frontmatter,开放规范、零锁定,删除派生索引后可完全重建。
- **本地优先** — 所有数据都是磁盘上的纯文件,vault = 一个目录树,可 git、可迁移、可备份。
- **三层混合索引** — Tantivy 全文 + sqlite-vec 向量 + SQLite 结构化,RRF 融合检索。
- **LLM 智能建议** — 自动生成摘要 / 标签建议,带溯源与可回滚,一键或批量接受。
- **选段改写** — 选中正文调用 LLM 改写,支持润色 / 扩写 / 翻译为英文 / 总结要点。
- **Chat with Vault** — 基于全库 RAG 的问答,引用真实笔记片段作答。
- **MCP Agent 接入** — 桌面端内嵌只读 MCP server,让 Claude / Cursor / ZCode 等读写你的 vault。
- **可插拔 Provider** — 支持 Ollama(全本地)与 OpenAI 兼容接口,带隐私护栏与按条目授权。
- **i18n** — 中文 / 英文界面。

---

## 📥 Download / 下载

Choose the installer for your platform:

| Platform | File | Notes |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.1.0_x64-setup.exe` | NSIS 安装程序(推荐) |
| 🪟 **Windows** | `LMNotes_0.1.0_x64_en-US.msi` | MSI 安装包(企业部署) |
| 🐧 **Linux (Debian/Ubuntu)** | `LMNotes_0.1.0_amd64.deb` | `sudo dpkg -i` 安装 |
| 🐧 **Linux (Fedora/RHEL)** | `LMNotes-0.1.0-1.x86_64.rpm` | `sudo rpm -i` 安装 |
| 🐧 **Linux (通用)** | `LMNotes_0.1.0_amd64.AppImage` | 免安装,双击运行 |

根据你的平台选择安装包:

| 平台 | 文件 | 说明 |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.1.0_x64-setup.exe` | NSIS 安装程序(推荐) |
| 🪟 **Windows** | `LMNotes_0.1.0_x64_en-US.msi` | MSI 安装包(企业部署) |
| 🐧 **Linux (Debian/Ubuntu)** | `LMNotes_0.1.0_amd64.deb` | `sudo dpkg -i` 安装 |
| 🐧 **Linux (Fedora/RHEL)** | `LMNotes-0.1.0-1.x86_64.rpm` | `sudo rpm -i` 安装 |
| 🐧 **Linux (通用)** | `LMNotes_0.1.0_amd64.AppImage` | 免安装,双击运行 |

---

## 🖥️ System Requirements / 系统要求

| Platform | Requirement |
|---|---|
| **Windows** | Windows 10/11 (x64), WebView2 runtime(系统通常已预装) |
| **Linux** | webkit2gtk-4.1, GTK3,现代发行版(Ubuntu 22.04+ / Fedora 等) |
| **macOS** | 暂未提供安装包,可从源码构建 |

| 平台 | 要求 |
|---|---|
| **Windows** | Windows 10/11(x64),WebView2 运行时(系统通常已预装) |
| **Linux** | webkit2gtk-4.1、GTK3,现代发行版(Ubuntu 22.04+ / Fedora 等) |
| **macOS** | 暂未提供安装包,可从源码构建 |

> 💡 **LLM Provider:** 首次启动后,在设置里配置 Provider。推荐本地用 [Ollama](https://ollama.com)(数据不出本机);也可填 OpenAI 兼容接口。

---

## 🚀 Getting Started / 快速开始

1. 安装并打开 LMNotes
2. 新建或打开一个本地 vault(一个普通文件夹)
3. 在 **设置 → Provider** 配置你的 LLM(Ollama / OpenAI 兼容)
4. 开始写笔记,体验智能建议与全库问答

---

## 🔒 Privacy / 隐私

- **数据归属**:所有笔记以纯 Markdown 文件存在你指定的本地目录,LMNotes 不收集、不上传你的笔记内容。
- **本地优先**:搭配 Ollama 时,LLM 推理完全在本地完成,敏感数据不出本机。
- **按条目授权**:上云 Provider 需显式授权,带隐私护栏。
- **MCP 只读**:暴露给外部 AI agent 的接口是只读的,agent 不能修改你的笔记。

---

## 📦 Built With / 技术栈

**Rust** · **Tauri 2** · **SolidJS** · **CodeMirror** · **Tantivy** · **sqlite-vec**

---

## 🛣️ Roadmap / 后续路线

- [ ] macOS 安装包
- [ ] 双向链接 / wiki 网络可视化
- [ ] 更丰富的全库 RAG 问答(引用溯源 UI)
- [ ] 移动端适配(复用 Rust 核心)

> 完整产品规划见 [`docs/specs/PRD.md`](https://github.com/evywang/lmnotes/blob/main/docs/specs/PRD.md)。

---

## 📄 License / 许可

Dual-licensed under **MIT** or **Apache-2.0**, at your option.

双授权:**MIT** 或 **Apache-2.0**,任选其一。

---

**Full Changelog / 完整变更**: https://github.com/evywang/lmnotes/commits/v0.1.0
