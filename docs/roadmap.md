# LMNotes 功能路线图

> 基于 **v0.2.0**(语音输入 + 本地 STT 降级,见 [release-notes](release-notes/release-notes-v0.2.0.md))的实现现状,对照 [PRD §5](specs/PRD.md) 需求清单整理。
> 制定日期:2026-08-26。优先级遵循 PRD(MVP / P1 / P2);状态均已对照代码核实。

---

## 现状盘点:PRD 承诺 vs 实际实现

| PRD 需求 | 优先级 | 状态 | 说明 |
|---|---|---|---|
| FR-CAP-03 双向链接补全 | **MVP** | ✅ v0.3.0 | `[` 触发,标题/别名/路径补全,插入 OKF 内链 |
| FR-STORE-01 多 Vault | **MVP** | ❌ 未实现 | vault 硬编码 `~/.lmnotes/default`(`lib.rs::vault_dir`) |
| FR-MEDIA-04 媒体任务队列 | **MVP** | ❌ 未实现 | 转录内联执行(60s 超时),无队列/重试/暂停 |
| FR-CAP-04 拖拽/粘贴多媒体 | MVP | ⚠️ 半成品 | 只处理 `image/`;音频/视频被跳过。`insert_audio` 后端已就绪,差 UI 接线 |
| FR-LLM-09 建议回滚 UI | P1 | ✅ v0.3.0 | 历史面板:列表/预览/恢复(接受建议写回仍留 M1c) |
| FR-LLM-06 行动项抽取 | P1 | ✅ v0.3.0 | transcript/meeting 笔记一键抽取 checklist |
| FR-MEDIA-02 图片 OCR / 视觉描述 | P1 | ❌ 未实现 | |
| FR-CAP-08 模板系统 | P1 | ❌ 未实现 | |
| 语音遗留(按住说话 / 流式 / 断点续传) | P1 | ❌ | 见 [ADR-0006](adr/ADR-0006-voice-transcription.md) / [ADR-0007](adr/ADR-0007-local-stt-fallback.md) 遗留清单 |
| FR-STORE-05 导出 zip / git init | P1 | ❌ | |
| macOS 打包 | — | ❌ | release 矩阵仅 Win + Linux(sidecar 需签名公证) |
| FR-CAP-09 长音频后台转录 | P2 | ❌ | 依赖 FR-MEDIA-04 任务队列 |
| FR-LLM-07 每日回顾 / 周报 | P2 | ❌ | |
| FR-STORE-06 Obsidian/Foam 导入 | P2 | ❌ | |
| FR-CAP-06 移动端速记 | P2 | ❌ | 依赖核心跨端复用验证 |

---

## 迭代规划(三个版本,主题化)

### v0.3.0「连接」— 补齐 Wiki 核心体验 ✅ 已实现(分支 feat/voice-input)

| 功能 | 实现路径(复用度) | 量级 | 状态 |
|---|---|---|---|
| **FR-CAP-03 双链补全** | `[` 触发 CM6 autocompletion → `list_note_titles`(aliases 已入索引)→ 插入 OKF 路径链接 | M | ✅ |
| **FR-LLM-06 行动项抽取** | transcript/meeting 笔记工具栏按钮 → `extract_action_items`(复用 Summarize 路由 + 护栏读 `llm_local_only`)→ checklist 追加正文 | S | ✅ |
| **FR-LLM-09 快照恢复 UI** | 「🕘 历史」面板:`list_snapshots`/`read_snapshot` + 预览 + 全文替换恢复(进 CM history 可撤销) | S | ✅ |

### v0.4.0「多库与媒体」— 基础能力 + 多模态收口 🥇 下一站([实施设计](superpowers/plans/2026-08-29-v0.4.0-vaults-and-media.md))

| 功能 | 实现路径 | 量级 |
|---|---|---|
| **FR-STORE-01 多 Vault** | vault 路径进 config(`last_vault`)+ 启动重建索引/watcher + 设置页选择器;改动集中 `lib.rs::vault_dir()` 与状态重建 | M |
| **FR-CAP-04 补全** | Editor 拖拽/粘贴分支放开 `audio/`、`video/` → `insert_audio` 归档(已有)→ 复用 `create_voice_note` 编排触发转录 | S |
| **FR-MEDIA-02 OCR / 视觉描述** | 与 transcript 同构:图片 → 多模态 LLM → `type: image-desc` concept(PRD §3.5 已定义格式);OpenAI 兼容 vision + Ollama llava 双路由,护栏同 ADR-0005 | M |
| 语音打磨包 | 按住说话(push-to-talk)、模型下载断点续传(HTTP Range) | S |

### v0.5.0「效率与分发」([实施设计](superpowers/plans/2026-08-29-v0.5.0-efficiency-and-distribution.md))

| 功能 | 实现路径 | 量级 |
|---|---|---|
| **FR-MEDIA-04 任务队列**(MVP 欠账) | SQLite 任务表 + 后台 worker + 前端任务中心(暂停/重试/进度);顺带收编 FR-CAP-09 长音频后台转录 | L |
| FR-CAP-08 模板系统 | `templates/` 目录 + frontmatter 占位符替换 + 新建笔记时选模板 | S |
| FR-STORE-05 导出 zip / git init | 后端命令 + 设置页按钮 | S |
| macOS 打包 | release 矩阵加 `aarch64-apple-darwin` sidecar + 签名公证(需开发者账号) | M |

---

## 刻意押后(有真实需求再立项)

- **流式转录**:whisper.cpp 以 batch 为主,真流式需 Deepgram/Groq WebSocket,管线重构大、速记场景体验增益有限 —— 维持 P1/P2。
- **端到端加密 / 同步**:PRD 未承诺;git / 文件夹同步对 MVP 用户足够。

**整体押后**:移动端速记(FR-CAP-06)、Obsidian 导入(FR-STORE-06)、每日回顾(FR-LLM-07)—— 均为 P2。

---

## 维护约定

- 每发一版,更新对应行的状态(❌→✅)并把「当前首选」移到下一迭代。
- 新增需求先进 [PRD §5](specs/PRD.md) 拿 FR-ID,再进本表。
- 量级:S ≈ 半天内,M ≈ 1-3 天,L ≈ 一周+(单人有效开发时间)。
