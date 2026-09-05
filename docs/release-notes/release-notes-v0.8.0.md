# LMNotes v0.8.0

> **迁移与回顾 —— 导入 Obsidian 库、每日/每周回顾、用量透明。**

v0.8 的主题是**迁移与回顾**：从 Obsidian/Foam/纯 Markdown 目录一键迁入（wikilink
自动转 OKF 链接）；每日/每周回顾把近期笔记交给 LLM 提炼；LLM 用量仪表盘让
"本地 vs 云端"的隐私承诺变得可观测。

---

## ✨ Highlights / 核心亮点

### 📥 导入外部库（FR-STORE-06）

- 设置 → 「📥 导入外部库」：选目录 → **dry-run 报告**（笔记/资源数量、链接解析
  统计、警告）→ 确认执行。
- `[[wikilink]]` 自动转 OKF 链接（精确路径 > 唯一文件名 > 唯一标题；歧义不猜，
  未解析的**原样保留**并在报告列出）；`![[图片]]` 嵌入转 assets 引用。
- frontmatter best-effort 映射：补 `type: note` / `id` / `title`；`tags`/`aliases`
  字符串转数组；未知键保留。图片/音视频按 SHA-256 去重归档；
  `.obsidian` / `.git` / `node_modules` 自动排除。

### 🗓 每日/每周回顾（FR-LLM-07）

- 命令面板（Ctrl+K）「每日回顾 / 每周回顾」：聚合近 1/7 天修改的笔记（标题 +
  正文开头，上限 30 篇）→ 生成结构化回顾（主题/进展/待跟进）。
- 产物写入 `notes/reviews/`（`tags: [review]`，附来源笔记 OKF 链接表）；走摘要
  路由与隐私护栏（ADR-0005）。

### 📊 LLM 用量仪表盘（FR-MODEL-05）

- 设置 → 「📊 LLM 用量」：按 provider × 任务（chat/embed/transcribe/vision）分组
  的调用次数与**估算** token（字符数/4），区分本地/云端。
- 全覆盖：索引建议、Chat、改写、转录、图片描述、回顾生成（注册期统一包裹，
  零调用点遗漏）。**隐私：只记数量，不记内容。**

### ⚡ 全局热键可配置

- 设置 → 「⚡ 全局热键」可修改快速捕获浮窗热键（默认 `CmdOrCtrl+Shift+L`，
  Tauri accelerator 语法；保存后重启生效）——解决热键被其它程序占用的问题。

---

## 📥 Download / 下载

| Platform | File | Notes |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.8.0_x64-setup.exe` | NSIS（推荐） |
| 🪟 **Windows** | `LMNotes_0.8.0_x64_en-US.msi` | MSI |
| 🐧 **Linux** | `LMNotes_0.8.0_amd64.deb` / `.rpm` / `.AppImage` | |
| 🍎 **macOS (Apple Silicon)** | `LMNotes_0.8.0_aarch64.dmg` | 未签名：右键 → 打开 |

---

## 🔄 Changelog

### Added
- 库导入（dry-run + wikilink→OKF + assets 去重归档）（FR-STORE-06）
- 每日/每周回顾生成（FR-LLM-07，手动触发）
- LLM 用量记录与设置页仪表盘（FR-MODEL-05，脱敏）
- 全局热键可配置（config.capture.hotkey）

### Changed
- 版本 0.7.0 → **0.8.0**

---

## 🧭 Known Issues / 已知限制

- 导入为内存内两阶段，超大库（数千文件）首次导入耗时较长。
- 回顾为手动触发（定时自动化见 roadmap）；token 为估算值。
- macOS 为未签名 dmg（右键打开）；签名需 Apple 开发者账号 Secrets。

---

Full docs: [使用手册](../user-manual.md)（§7.6–7.9） · [路线图](../roadmap.md) · [v0.8 计划](../superpowers/plans/2026-09-05-v0.8.0-migration-and-review.md)
