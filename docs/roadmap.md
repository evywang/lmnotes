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
| FR-CAP-08 模板系统 | P1 | ✅ v0.5.0 | templates/ + {{title}}/{{date}} 等占位符 |
| 语音遗留(流式转录) | P1 | ❌ | 按住说话 ✅ v0.4.0、模型断点续传 ✅ v0.4.0、多 Provider 云端 STT ✅ v0.6.0;见 [ADR-0006](adr/ADR-0006-voice-transcription.md) / [ADR-0007](adr/ADR-0007-local-stt-fallback.md) |
| FR-STORE-05 导出 zip / git init | P1 | ✅ v0.5.0 | 流式 zip(排除派生数据) + 探测式 git init |
| macOS 打包 | — | ✅ v0.5.0 | CI 源码构建 sidecar;无 Apple 账号发未签名 dmg |
| FR-CAP-09 长音频后台转录 | P2 | ✅ v0.5.0 | 已并入任务队列;本地子进程预算 60s/队列 15min 见 [v0.5.1 设计](superpowers/plans/2026-08-29-v0.5.1-task-cancel-and-timeouts.md) |
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

### v0.4.0「多库与媒体」— 基础能力 + 多模态收口 ✅ 已实现([实施设计](superpowers/plans/2026-08-29-v0.4.0-vaults-and-media.md))

| 功能 | 实现路径 | 量级 |
|---|---|---|
| **FR-STORE-01 多 Vault** | vault 路径进 config(`last_vault`)+ 启动重建索引/watcher + 设置页选择器;改动集中 `lib.rs::vault_dir()` 与状态重建 | M | ✅([ADR-0008](adr/ADR-0008-multi-vault-restart-switch.md)) |
| **FR-CAP-04 补全** | Editor 拖拽/粘贴分支放开 `audio/`、`video/` → `insert_audio` 归档(已有)→ 复用 `create_voice_note` 编排触发转录 | S | ✅ |
| **FR-MEDIA-02 OCR / 视觉描述** | 与 transcript 同构:图片 → 多模态 LLM → `type: image-desc` concept(PRD §3.5 已定义格式);OpenAI 兼容 vision + Ollama llava 双路由,护栏同 ADR-0005 | M | ✅ |
| 语音打磨包 | 按住说话(push-to-talk)、模型下载断点续传(HTTP Range) | S | ✅(续传于评审轮 2 落地) |

### v0.5.0「效率与分发」✅ 已实现([实施设计](superpowers/plans/2026-08-29-v0.5.0-efficiency-and-distribution.md))

| 功能 | 实现路径 | 量级 |
|---|---|---|
| **FR-MEDIA-04 任务队列**(MVP 欠账) | SQLite 任务表 + 后台 worker + 前端任务中心(暂停/重试/进度);顺带收编 FR-CAP-09 长音频后台转录 | L | ✅ |
| FR-CAP-08 模板系统 | `templates/` 目录 + frontmatter 占位符替换 + 新建笔记时选模板 | S | ✅ |
| FR-STORE-05 导出 zip / git init | 后端命令 + 设置页按钮 | S | ✅ |
| macOS 打包 | release 矩阵加 `aarch64-apple-darwin` sidecar + 签名公证(需开发者账号) | M | ✅(未签名 dmg 无账号可发) |

### v0.6.0「任务可控与语音打磨」✅ 已实现([v0.5.1 设计](superpowers/plans/2026-08-29-v0.5.1-task-cancel-and-timeouts.md))

> v0.5.1(任务取消+超时预算)与语音可用性修复并入本版交付。

| 功能 | 说明 | 量级 | 状态 |
|---|---|---|---|
| **FR-MEDIA-04 收口(v0.5.1)** | running 任务强杀取消(AbortHandle + kill_on_drop + 条件 UPDATE 防竞态);超时预算上移调用点(队列 15min / 内联 60s,云端请求首次有上限) | M | ✅ |
| 长媒体后台分流(GAP-A) | 拖拽/粘贴与语音弹窗按实际时长探测,>阈值(默认 60s)自动入队;设置页可调阈值/设为全部后台 | M | ✅ |
| 队列修复包(GAP-B/C + 审计) | 排队视频正确归位 `assets/video/`;重试判定改类型化错误分类(断网真的会重试);ffmpeg kill_on_drop、取消注册表泄漏 | S | ✅ |
| 语音可用性包 | **Windows 打包版语音修复**(sidecar 平台配置改名 `tauri.*.conf.json` 被 Tauri 2 自动加载);模型下载 404 修复 + hf-mirror 镜像回退 + 30s 超时;弹窗内联下载免重启 | M | ✅ |
| 多 Provider 云端 STT | 设置页增删 OpenAI 兼容 provider,Transcribe Model 自动派生转录路由(主 LLM 无转录端点时可专配 STT provider) | S | ✅ |

### v0.7.0「导航与 MVP 收口」✅ 已实现([实施设计](superpowers/plans/2026-09-04-v0.7.0-navigation-mvp-closure.md))

> **里程碑：本版完成后 PRD §5 全部 MVP 级行 ✅（M0–M4 达成）。**

| 功能 | 实现路径 | 量级 | 状态 |
|---|---|---|---|
| **FR-SEARCH-01 命令面板** | Ctrl+K 浮层:命令 + list_note_titles 笔记检索 + 最近打开(localStorage 上限 8);↑↓/Enter/Esc 全键盘 | M | ✅ |
| **FR-CAP-01 全局快捷键浮窗** | tauri-plugin-global-shortcut(Rust 侧注册,失败降级);置顶无边框小窗 #quick-capture 路由,Ctrl+Enter 存当日日记 | M | ✅ |
| FR-SEARCH-05 时间线/每日/标签 | concepts.tags 列(老库迁移)+ mtime 倒序按日分组视图 + 标签云过滤 + 今日笔记幂等入口 | M | ✅ |
| 门禁与实测 | fmt/clippy/187 tests/tsc/build + CDP 实测 10/10 | — | ✅ |

### v0.8.0「迁移与回顾」✅ 已实现([实施设计](superpowers/plans/2026-09-05-v0.8.0-migration-and-review.md))

| 功能 | 实现路径 | 量级 | 状态 |
|---|---|---|---|
| **FR-STORE-06 库导入** | core import 纯逻辑(wikilink→OKF 链接五形态/frontmatter 映射/路径去重,9 测试)+ import_vault dry-run 报告→确认执行 + assets SHA-256 去重归档 + 设置页入口 | L | ✅ |
| **FR-MODEL-05 用量仪表盘** | Recording 包装器单一挂点(流式 chat 部分成功计数,失败不记)+ llm_usage 脱敏表(无内容)+ 设置页按 provider×任务分组表格 | S | ✅ |
| **FR-LLM-07 每日/每周回顾** | 近 1/7 天笔记聚合(30 篇×200 字)→ Summarize 路由 + ADR-0005 护栏 → notes/reviews/ 产物(来源 OKF 链接表);命令面板动作 | M | ✅ |
| 热键可配置 | config.capture.hotkey(默认 CmdOrCtrl+Shift+L,旧 config 兼容);设置页输入,重启生效 | S | ✅ |
| 门禁与实测 | fmt/clippy/204 tests/tsc/build + CDP 冒烟 8/8(含真实 GLM 回顾与用量落库) | — | ✅ |

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
