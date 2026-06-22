# LMNotes 产品需求规格说明书（PRD）

| 字段 | 值 |
|---|---|
| 文档版本 | v0.2（决策已锁定，待 ADR） |
| 创建日期 | 2026-06-20 |
| 最后更新 | 2026-06-21（§15 全部决策完成） |
| 状态 | Decision-Complete — §15 全部决策已锁定，下一步进入 ADR 与 M0 实现计划 |
| 文档类型 | Product Requirements Document（产品规格说明书） |
| 后续产物 | ADR（架构决策记录）→ 实现计划（writing-plans）→ 编码（TDD） |

> **阅读路径建议：** 先读 §1（它是什么）→ §2（核心理念）→ §3（OKF 规范）→ §5（功能清单）→ §15（需要你拍板的决策）。其余为支撑性细则。

---

## 1. 产品概述

### 1.1 一句话定位
**LMNotes 是一个本地优先（local-first）、LLM 原生的个人知识应用：** 它用一套机器与人都能直接读写的人类可读格式（Open Knowledge Format, OKF）把笔记、语音、图片、视频组织成一个**自我组织的 wiki**，并由可插拔的 LLM 持续为其建立连接、生成摘要、回答提问。

### 1.2 要解决的问题
| # | 用户痛点 | LMNotes 的应对 |
|---|---|---|
| P1 | 笔记越写越多却彼此孤立，找不到、连不起来 | LLM 自动发现笔记间的语义关联并维护双向链接图谱 |
| P2 | 录音/截图/随手记下的素材堆积，没人整理 | 多模态捕获后由 LLM 自动转录、打标签、生成摘要并归位 |
| P3 | 笔记软件把数据锁在私有数据库/云服务里，导入导出困难、撤离成本高 | OKF 以纯文本（Markdown + YAML frontmatter + 平铺资源文件）落盘，无锁定 |
| P4 | 云端 AI 笔记涉及隐私敏感数据外泄 | 本地优先架构；LLM 可全本地（Ollama/llama.cpp）或按条目显式授权上云 |
| P5 | AI 助手是"外挂"而非"原生"，理解不了你的全部笔记 | LLM 可对全库做索引/RAG/图谱问答，是知识的操作系统而非聊天框 |

### 1.3 目标用户
- **主要：** 研究者、工程师、写作者、终身学习者——拥有大量异构素材，重视数据主权与知识结构化。
- **次要：** 学生、产品经理、知识工作者中的"重笔记用户"。

### 1.4 价值主张
1. **你的数据永远是你的**——纯文本、可 git、可迁移。
2. **AI 不是插件，是引擎**——它替你织网、提炼、检索、对话。
3. **多模态一处收纳**——文字、语音、图片、视频同等公民。
4. **优雅且高效**——键盘驱动的"高级感"交互，跨桌面 / 移动 / Web 一致。

### 1.5 非目标（Non-Goals，本期不做）
- ❌ 协同编辑 / 多人共享工作区（单用户优先，同步见 §6.6 但不做 OT 协同）。
- ❌ 富文本所见即所得排版器（保持 Markdown 原生，不做 Word 式排版）。
- ❌ 内置出版 / 博客发布流水线。
- ❌ 通用视频/图片编辑器。
- ❌ 自研 LLM 训练（仅做适配与编排）。

---

## 2. 核心理念：LLM Wiki

传统 wiki（如 MediaWiki、Obsidian）的链接由**人**手动建立。LMNotes 提出 **"LLM Wiki"**：链接、摘要、结构由 **LLM 持续生成并维护**，人负责创造原始原子知识。

### 2.1 LLM Wiki 的五条原则
1. **原子知识（Atomic Notes）**：每条笔记聚焦一个概念，有稳定 ID，可被无限引用。
2. **机器可读写 = 人可读写**：所有结构化信息（标题、标签、链接、摘要、向量、转录）都以 OKF（Markdown + YAML frontmatter）或纯文本+派生索引文件形式表达，不依赖专有二进制数据库即可还原全部语义。
3. **链接是涌现的**：LLM 在后台扫描新建/修改的笔记，提出标准 markdown 链接候选（符合 OKF §5）；用户可一键接受、批量接受、或设为自动接受。
4. **多层视图同源**：图谱、时间线、大纲、问答——所有视图都从同一份 OKF 数据实时派生，无单一数据库瓶颈。
5. **可解释、可撤销**：每条 LLM 产出（链接建议、摘要、标签）都带溯源（来自哪条笔记、哪次运行），用户可拒绝并回滚。

### 2.2 与传统笔记/wiki 的区别
| 维度 | 传统 Wiki | LLM Wiki（LMNotes） |
|---|---|---|
| 链接来源 | 人工 | 人工 + LLM 涌现 |
| 摘要 | 人工写 | LLM 生成 + 人工校订 |
| 检索 | 关键词 | 关键词 + 向量 + 图谱遍历 |
| 入口 | 手动分类/文件夹 | 标签 + 图谱 + 对话 |
| 数据格式 | 私有 DB 或专有 md 变体 | 开放 OKF 规范 |

---

## 3. Open Knowledge Format (OKF) 规范

### 3.0 规范来源与对齐策略

LMNotes **遵循 Google 发布的 Open Knowledge Format (OKF) v0.1 草案**。

- **官方规范：** `https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md`（Google Cloud，2026-06-12 发布）
- **本地存档：** `docs/okf/SPEC.v0.1.md`（2026-06-21 抓取存档，便于离线查阅与回归对齐）
- **建设者参考站：** `https://openknowledgeformat.com/`（第三方，由 Mathias Onea 维护，提供在线 Validator / Examples / Templates，内容与官方规范一致）

**对齐原则：** LMNotes 产出的每一个 Vault 都必须是一个 **OKF-conformant bundle**（满足官方 §9 Conformance）。在此基础上，LMNotes 通过 OKF 明确允许的 **producer-defined frontmatter keys**（官方 §4.1 Extensions："Producers MAY include any additional keys"）做合规扩展，不偏离、不 fork 规范。

> OKF 一句话定义（官方原文）：*"an open, human- and agent-friendly format for representing knowledge — a directory of markdown files with YAML frontmatter. There is no schema registry, no central authority, and no required tooling."*

### 3.1 OKF v0.1 核心规则（LMNotes 必须遵守）

| 规则 | 官方条款 | LMNotes 实现 |
|---|---|---|
| Bundle = 目录树 | §3 | Vault = 一个目录树 |
| 单元 = Concept（一个 `.md`） | §2, §4 | 一条笔记 = 一个 concept |
| Concept ID = 文件路径去 `.md` | §2 | 如 `notes/ai/llm-wiki` |
| 必填 frontmatter：仅 `type` | §4.1, §9 | 每条笔记都有 `type` |
| 链接用标准 markdown link | §5 | `[文本](/notes/ai/llm-wiki.md)` |
| 保留文件名 `index.md` / `log.md` | §3.1, §6, §7 | 不用作笔记文件名 |
| 消费者必须容忍未知字段/类型/断链 | §9 | 导入第三方 bundle 时不拒绝 |
| 版本声明：根 `index.md` 的 `okf_version` | §11 | Vault 根 `index.md` 写 `okf_version: "0.1"` |

**官方 Conformance（§9）三条硬约束**——LMNotes 产出的 bundle 必须满足：
1. 每个非保留名 `.md` 文件含可解析的 YAML frontmatter。
2. 每个 frontmatter 含非空 `type` 字段。
3. `index.md` / `log.md`（若存在）符合 §6 / §7 结构。

### 3.2 LMNotes Vault 目录布局（OKF bundle 实例）

严格遵循 OKF §3 的"自由目录树 + 保留文件名"模型：

```
my-vault/                          # Vault = OKF Bundle = 一个目录
├── index.md                       # OKF §6：根目录索引（含 okf_version 声明，见 §11）
├── log.md                         # OKF §7：变更历史（可选，应用可代写）
├── notes/                         # 笔记 concept 的存放区（子目录自由组织）
│   ├── index.md                   # 该目录的 progressive disclosure 索引
│   ├── ai/
│   │   ├── index.md
│   │   ├── llm-wiki.md            # 一个 concept = 一条笔记
│   │   └── attention.md
│   └── daily/
│       └── 2026-06-20.md
├── assets/                        # 二进制资源（OKF 未规范，见 §3.5 扩展）
│   ├── img/ab/ab12cd34.png        # 按 SHA-256 哈希分桶去重
│   ├── audio/9f/9f2c...m4a
│   └── video/...
├── transcripts/                   # 转录/OCR/描述 concept（type=Transcript 等）
│   └── 2026-06-20-meeting.md      # 作为独立 OKF concept，正文是转录稿
├── templates/                     # 笔记模板（应用内部用，不强制 OKF 约束）
└── .lmnotes/                      # 派生数据（可删，应用重建）—— 非 OKF 规范范畴
    ├── index.sqlite               # 全文/向量/图谱派生索引（缓存）
    ├── embeddings/
    ├── llm/                       # LLM 运行日志、建议队列、回滚快照
    └── cache/
```

**关键设计要点：**
- `notes/` + `transcripts/` + `assets/` 是唯一真源（source of truth），全部为 OKF 可读内容或 OKF 引用的二进制。
- `.lmnotes/` 全部为派生数据，删除后应用可基于 OKF bundle 完全重建（"无锁定"承诺的技术保证）。
- `index.md` / `log.md` 严格遵循 OKF §6/§7，可被任何 OKF 消费者识别。
- 资源按 SHA-256 分桶去重存储（`assets/<kind>/<前2位>/<哈希>.<ext>`）。

### 3.3 笔记（Concept）文件格式

严格遵循 OKF §4.1 的 frontmatter 约定，**必填字段只有 `type`**；LMNotes 用合规的 producer-defined keys 做扩展：

```markdown
---
# === OKF 官方字段（§4.1）===
type: note                         # REQUIRED by OKF。LMNotes 约定值见下表
title: LLM Wiki 的核心概念         # OKF recommended
description: LLM Wiki 是由大模型自动维护链接与摘要的 wiki。  # OKF recommended
tags: [ai, knowledge-management]   # OKF optional
timestamp: 2026-06-20T11:48:21+08:00  # OKF optional，最近有意义变更时间
# resource: ...                    # OKF optional，描述外部资产时用（笔记一般留空）

# === LMNotes 扩展字段（OKF §4.1 明确允许的 producer-defined keys）===
id: nt_20260620_1142_a3f9          # 稳定 ID，抗改名/移动（见 §3.4 决策）
aliases: [LLM维基, llm-wiki]       # 别名，供链接补全
status: draft                      # draft | active | archived
language: zh-CN                    # 驱动分词与模型选择
created: 2026-06-20T11:42:03+08:00 # 创建时间
summary_source: llm                # summary 写在 OKF description 字段；此字段标记来源
---

# LLM Wiki 的核心概念

正文使用标准 Markdown。引用其它笔记用 OKF 标准链接（§5）：

- 绝对（bundle-relative，推荐）：见 [注意力机制](/notes/ai/attention.md)
- 相对：见 [邻近概念](./transformer.md)

图片/音视频用标准 markdown 语法引用 assets：![示意图](/assets/img/ab/ab12cd34.png)

> 引用、代码块、表格、脚注使用 CommonMark + GFM。
```

**LMNotes 的 `type` 约定值**（OKF 明确不强制注册，producer 自定，consumer 容错）：
| type 值 | 用途 |
|---|---|
| `note` | 普通原子笔记（默认） |
| `daily` | 每日笔记 |
| `source` | 外部资料摘录/读书笔记 |
| `meeting` | 会议纪要 |
| `transcript` | 音频/视频转录稿（§3.5） |
| `image-desc` | 图片描述/OCR（§3.5） |

**与官方 OKF 的关系（自检）：** 上例 frontmatter 删掉所有"LMNotes 扩展字段"后，仍完全满足 OKF §9 Conformance（有 `type`、有 frontmatter、body 是 markdown）。任何 OKF 消费者都能读它。

### 3.4 Concept ID 策略（合规扩展）

OKF §2 定义 Concept ID = 文件路径去 `.md`。LMNotes **不改动这一官方定义**，同时增加一个 frontmatter 扩展字段 `id` 作为内部稳定引用键：

- **对外（OKF 消费者）：** Concept ID 仍是路径，如 `notes/ai/llm-wiki`。链接用官方 markdown link 指向路径。
- **对内（LMNotes 应用）：** 额外用 frontmatter 的 `id`（如 `nt_20260620_1142_a3f9`，全局唯一、不可变）作为图谱/索引/回滚的主键，**抗改名/移动**——用户重命名或移动笔记时，应用自动重写所有引用该 `id` 的链接路径，对外链接始终不断。
- **合规性：** `id` 是 OKF §4.1 明确允许的 producer-defined key，不破坏 Conformance。
- **ID 生成规则：** `nt_YYYYMMDD_HHMM_<4位base32>`（笔记）、`at_<slug>_<4位>`（资源），全局唯一、与路径解耦。

### 3.5 多模态资源（OKF 合规扩展）

OKF v0.1 只规范 markdown，未涉及二进制资源。LMNotes 用**资源作为引用 URI + 描述性 concept**的方式做合规扩展（不引入 OKF 之外的专有格式）：

1. **二进制本体**平铺存 `assets/<kind>/<哈希前2位>/<sha256>.<ext>`，去重。OKF 不规范它，它也不是 concept。
2. **资源的语义**（转录稿、OCR、视觉描述）写成**一个独立的 OKF concept**，存 `transcripts/` 或 `descriptions/`，`type` 用 `transcript` / `image-desc`，其 frontmatter 的 `resource` 字段（OKF 官方字段）指向二进制：
```markdown
---
type: transcript
title: 2026-06-20 会议转录
description: LLM Wiki 发布节奏讨论，含 3 条行动项。
resource: file://vault/assets/audio/9f/9f2c...m4a   # OKF 官方字段，指向二进制
tags: [meeting, llm-wiki]
timestamp: 2026-06-20T15:30:00+08:00
id: at_20260620_meeting_7g2h                         # LMNotes 扩展
duration_ms: 1834000                                 # LMNotes 扩展
mime: audio/mp4                                      # LMNotes 扩展
language: zh-CN
transcribed_by: whisper-large-v3@local               # LMNotes 扩展
---

# 2026-06-20 会议转录

[00:00:12] 张三：我们讨论一下 LLM Wiki 的发布节奏……
（完整转录稿，标准 markdown body）
```
3. **笔记引用资源**就用 OKF 标准链接：`[会议录音转录](/transcripts/2026-06-20-meeting.md)`，或在正文嵌图 `![](/assets/img/...)`。
4. **视频**额外抽关键帧，关键帧列表作为该 concept body 的一个表格或列表（不引入专有 sidecar 格式）。

**合规性自检：** 删掉所有 LMNotes 扩展字段后，上面的 transcript concept 仍是合规 OKF（有 `type`、有 frontmatter、`resource` 是官方字段、body 是 markdown）。第三方 OKF 消费者读到它会看到一个描述某 `resource` URI 的概念文档。

### 3.6 OKF 合规性要求（LMNotes 应用必须满足）

在官方 §9 Conformance 之上，LMNotes 自我加压四条：

1. **可重建性：** 删除 `.lmnotes/` 后，应用启动时能基于 OKF bundle（`notes/` + `transcripts/` + `assets/` + `index.md`）重建全部派生索引（向量/全文/图谱），且不丢失任何用户可见信息。
2. **可导出性：** 一键导出整个 Vault 为纯目录（zip / git），任何 OKF 消费者可读；导出物不应包含 `.lmnotes/`。
3. **可导入性：** 能导入任意 OKF-conformant bundle（含从 Obsidian/Foam 转换而来的），导入时不因未知 `type` 或扩展字段而拒绝（遵守官方 §9 容错要求）。
4. **前向兼容：** 读取时忽略未知 frontmatter 字段；OKF 版本升级（minor/major）按官方 §11 语义处理；LMNotes 自有扩展字段的演进不影响 OKF 合规。
5. **Validator 对齐：** 应用内置校验器，规则与官方 §9 + 社区 Validator（openknowledgeformat.com/validator）一致，可在应用内一键校验 Vault 合规性。

---

## 4. 用户故事（代表性场景）

- **US-1（快速捕获）：** 我在地铁上有个想法，按全局快捷键 → 语音口述 30 秒 → 应用本地转录 → 自动打标签归档到今天的 daily note。3 秒内回到主界面。
- **US-2（涌现链接）：** 我写完一篇"注意力机制"的笔记，几秒后界面侧栏出现 3 条建议链接到"Transformer""QKV""位置编码"，我批量接受，图谱即时生长。
- **US-3（图谱问答）：** 我问"我过去半年关于 RAG 的所有笔记里，主流的 chunk 策略有哪些？" 应用做向量+图谱检索，给出带引用的答案，每个论点可点击跳转到源笔记。
- **US-4（多模态整理）：** 我把会议录音拖进来，自动转录、生成摘要、抽取行动项、链接到相关项目笔记。
- **US-5（数据主权）：** 我把整个 vault 目录 `git push` 到私有仓库，换台电脑 clone 后打开 LMNotes，一切如旧。
- **US-6（自定义模型）：** 我在设置里把对话模型切到本地 Ollama 的 `qwen3:32b`，把摘要模型切到云端 GLM，不同任务用不同模型，按条目决定是否上云。

---

## 5. 功能需求（Functional Requirements）

> 编号规则：`FR-<域>-<序号>`。每条标注 **MVP / P1 / P2** 三档优先级。

### 5.1 OKF 存储与 Vault 管理（域：STORE）
| ID | 需求 | 优先级 |
|---|---|---|
| FR-STORE-01 | 创建/打开/切换多个 Vault（每个 Vault = 一个本地目录） | MVP |
| FR-STORE-02 | 读写符合 §3.3 的笔记文件；frontmatter 字段强校验，损坏文件只读保护不丢数据 | MVP |
| FR-STORE-03 | 资源文件 SHA-256 去重存储；引用通过相对路径 | MVP |
| FR-STORE-04 | 文件系统实时监听（vault 目录被外部编辑能感知并重建增量索引） | P1 |
| FR-STORE-05 | 一键导出 Vault 为 zip / 初始化 git 仓库 | P1 |
| FR-STORE-06 | 从 Obsidian/Foam/纯 Markdown 目录 best-effort 导入（含 wikilink 转换） | P2 |

### 5.2 捕获与编辑（域：CAPTURE）
| ID | 需求 | 优先级 |
|---|---|---|
| FR-CAP-01 | 全局快捷键唤起"快速捕获"浮窗（桌面端），支持文本/语音 | MVP |
| FR-CAP-02 | 所见即所写 Markdown 编辑器（CommonMark + GFM：表格/任务列表/脚注/数学） | MVP |
| FR-CAP-03 | 实时双向链接补全：键入 `[](` 时按 title/alias/id 补全为目标 concept 的 OKF 路径链接 | MVP |
| FR-CAP-04 | 拖拽 / 粘贴 / 选择文件 添加图片、音频、视频；自动落 `assets/` 并生成对应描述 concept（§3.5） | MVP |
| FR-CAP-05 | 语音输入：按住说话 / 流式转录，可选用本地 Whisper 或云端 | MVP |
| FR-CAP-06 | 移动端"速记"入口（下拉通知/小组件/Share Sheet 接收外部分享） | P1 |
| FR-CAP-07 | 块级拖拽、折叠、列表大纲模式 | P1 |
| FR-CAP-08 | 模板系统（每日笔记模板、会议模板等，支持 frontmatter 占位符） | P1 |
| FR-CAP-09 | 离线录制视频/长音频上传后后台转录，完成通知 | P2 |

### 5.3 多模态处理（域：MEDIA）
| ID | 需求 | 优先级 |
|---|---|---|
| FR-MEDIA-01 | 音频自动转录（Whisper 兼容引擎），转录稿写入 `type: transcript` 的描述 concept（§3.5） | MVP |
| FR-MEDIA-02 | 图片 OCR + 视觉描述（多模态 LLM 或专用模型） | P1 |
| FR-MEDIA-03 | 视频抽关键帧 + 转录音轨；关键帧索引化 | P2 |
| FR-MEDIA-04 | 媒体处理任务队列，可暂停/重试，失败不阻塞编辑 | MVP |
| FR-MEDIA-05 | 处理引擎可插拔（本地 whisper.cpp / 云端 API） | P1 |

### 5.4 LLM 智能（域：LLM）—— 核心差异化
| ID | 需求 | 优先级 |
|---|---|---|
| FR-LLM-01 | **后台索引器：** 新建/修改笔记后增量触发，生成：摘要、标签建议、链接建议，写入建议队列 | MVP |
| FR-LLM-02 | **向量索引：** 对笔记正文 + 资源转录/描述建向量索引，支持语义检索 | MVP |
| FR-LLM-03 | **建议中心：** 统一查看/接受/拒绝/批量处理 LLM 建议；每条带溯源与 diff | MVP |
| FR-LLM-04 | **图谱问答（Chat with Vault）：** 基于向量+图谱 RAG，回答带可点击引用 | MVP |
| FR-LLM-05 | **就地改写：** 选中正文 → 润色/扩写/翻译/总结为要点，带撤销 | MVP |
| FR-LLM-06 | **行动项抽取：** 从会议/语音转录抽取 TODO 并可转为任务 | P1 |
| FR-LLM-07 | **每日回顾 / 周报：** 基于时间段内的笔记自动生成 | P2 |
| FR-LLM-08 | 所有 LLM 调用需用户可见的"是否上云"开关；含敏感关键词的条目默认仅本地模型 | MVP |
| FR-LLM-09 | 每条 LLM 输出可回滚（保留前序版本于 `.lmnotes/llm/snapshots/`） | P1 |

### 5.5 搜索与连接（域：SEARCH）
| ID | 需求 | 优先级 |
|---|---|---|
| FR-SEARCH-01 | 全局命令面板（⌘/Ctrl+K）：跳转笔记、执行命令、问答 | MVP |
| FR-SEARCH-02 | 混合搜索：关键词（BM25）+ 向量 + 标签/属性过滤，结果可融合排序 | MVP |
| FR-SEARCH-03 | 知识图谱可视化（力导向图）：节点=笔记，边=链接；可过滤/聚焦/折叠 | P1 |
| FR-SEARCH-04 | 反向链接面板（在笔记侧显示谁引用了它） | MVP |
| FR-SEARCH-05 | 时间线 / 每日笔记 / 标签云等派生视图 | P1 |

### 5.6 高级感与高效 UX（域：UX）
| ID | 需求 | 优先级 |
|---|---|---|
| FR-UX-01 | 键盘驱动：所有操作有快捷键，可用命令面板触达一切 | MVP |
| FR-UX-02 | 无模态优先：尽量用内联/抽屉而非弹窗；toast 替代 alert | MVP |
| FR-UX-03 | 毫秒级响应：编辑器输入 < 16ms 延迟；列表/搜索 < 100ms（本地索引） | MVP |
| FR-UX-04 | 动效克制而有意义（链接生长、图谱演化、建议出现有过渡，可关） | P1 |
| FR-UX-05 | 深色/浅色/跟随系统；可自定义主题色 | MVP |
| FR-UX-06 | 无障碍：完整键盘焦点、屏幕阅读器语义、对比度 ≥ AA | P1 |
| FR-UX-07 | 移动端单手优先布局、手势返回、底部速记栏 | P1 |

### 5.7 自定义 LLM（域：MODEL）—— 见 §7 详述
| ID | 需求 | 优先级 |
|---|---|---|
| FR-MODEL-01 | 内置 Provider 抽象：本地（Ollama / llama.cpp / MLX）/ 云端（OpenAI 兼容、GLM、Anthropic 等） | MVP |
| FR-MODEL-02 | 按任务分派不同模型：摘要 / 链接建议 / 向量化 / 对话 / 转录 / 视觉 | MVP |
| FR-MODEL-03 | 连接配置：Base URL / API Key / 模型名 / 上下文窗口 / 速率，可测试连通 | MVP |
| FR-MODEL-04 | 隐私分级：标记每个 Provider 为 local/cloud；条目级开关控制是否允许发往云端 | MVP |
| FR-MODEL-05 | 成本与用量仪表盘（本地次数 / 云端 token 估算） | P2 |

### 5.8 多平台（域：PLATFORM）—— 见 §8、决策 O3
| ID | 需求 | 优先级 |
|---|---|---|
| FR-PLAT-01 | 桌面：Windows / macOS / Linux 原生安装包（Tauri） | MVP |
| FR-PLAT-02 | 跨端同步（见 §6.6） | P1（桌面间先行） |
| FR-PLAT-03 | Web：浏览器可用（含 Vault via OPFS / 远程） | P2（M5+） |
| FR-PLAT-04 | 移动：iOS / Android | P2（M5+） |

---

## 6. 非功能需求（Non-Functional）

### 6.1 性能（基线技术：Tauri + Rust 核心 + Tantivy + sqlite-vec）
- 本地索引 10,000 篇笔记内，冷启动到可交互 < 3s；全文搜索（Tantivy，中文 jieba-rs 分词）P95 < 100ms；混合检索（BM25+向量 RRF）P95 < 200ms。
- 笔记保存后增量索引更新（Tantivy + 向量）延迟 < 1s。
- 编辑器输入延迟 < 16ms（60fps）；粘贴 1MB 文本不卡顿。
- 媒体转录吞吐：本地 CPU 上 1 小时音频转录 < 10 分钟（依模型，作为参考指标不阻塞）。

### 6.2 可靠性与数据完整性
- **永不静默丢失用户数据**：所有写操作先写临时文件再原子重命名；损坏的 frontmatter 进入"只读隔离"而非删除。
- LLM 修改全部可回滚（FR-LLM-09）。
- 索引视为缓存：任何时候损坏都能从 `notes/` 重建。

### 6.3 可移植性与无锁定
- 严格遵守 OKF 合规性（§3.6）。
- 不使用任何"离开应用即不可读"的二进制格式作为唯一真源。

### 6.4 可扩展性
- 媒体处理、LLM 调用、向量计算均为独立 worker，可并行/可队列。
- 插件/扩展点：Provider、视图、导入器、命令。（P2 提供正式 API）

### 6.5 可观测性与可调试性
- 应用内"日志查看器"展示 LLM 调用、索引事件、错误，便于排查。
- 开发模式可导出诊断包（脱敏）。

### 6.6 同步（P1）
- **首选方案：** 文件级同步——用户用任意 sync（iCloud / Syncthing / Git / WebDAV）同步 vault 目录，应用基于文件 mtime + 内容哈希合并。
- **不做** OT 实时协同编辑（单用户假设）。
- 冲突策略：笔记级三向合并（base/ours/theirs），frontmatter 字段冲突进入"冲突视图"人工解决。

### 6.7 国际化与无障碍
- UI 多语言（中/英先行，可扩展）；笔记 `language` 字段驱动分词与模型选择。
- 无障碍符合 WCAG 2.1 AA（FR-UX-06）。

---

## 7. LLM 集成与自定义（详细设计）

### 7.1 Provider 抽象
统一抽象为 `LLMProvider` 接口，能力维度：
- `chat(messages, opts)` → 文本生成（摘要/链接/问答/改写）
- `embed(texts)` → 向量（用于索引）
- `transcribe(audio)` → 转录（可委托给专用引擎）
- `vision(image/video frames)` → 视觉理解
- 能力探测：每个 Provider 声明其支持的能力集合；UI 按任务只显示具备能力的 Provider。

### 7.2 内置 Provider（MVP 至少含）
| Provider | 类型 | 能力 | 备注 |
|---|---|---|---|
| Ollama | 本地 | chat / embed（视模型） | 推荐默认本地 |
| llama.cpp / MLX | 本地 | chat | 高级用户 |
| OpenAI 兼容 | 云端 | chat / embed / vision / 转录(Whisper) | 通用兼容口 |
| GLM | 云端 | chat / embed / vision | 国内可用 |
| Whisper（whisper.cpp / API） | 任一 | 转录 | 独立配置 |

### 7.3 任务→模型分派（Routing）
设置面板按任务列：摘要 / 链接建议 / 向量 / 对话 / 转录 / 视觉，每个下拉选 Provider+模型。
- 默认策略示例：摘要→本地小模型，对话→本地大模型或云端，向量→本地或专用 embed 模型。
- 条目级覆盖：单条笔记可标记"仅本地处理"（含敏感内容时）。

### 7.4 隐私护栏（强制）
- 任何发往云端 Provider 的请求，必须经用户全局授权；首次启用云端时显式同意。
- 可配置"敏感关键词/正则"清单，命中则强制仅本地处理。
- 云端调用日志（脱敏：仅记录 endpoint、token 数、时间，不存内容）。

---

## 8. 跨平台策略

### 8.1 技术取向 ✅ 已决策 O4（见 §15.4）
**选定方案 A：Tauri（Rust core + Web UI）。** 以下对比表保留为决策依据。

| 方案 | 桌面 | 移动 | Web | 本地能力(LLM/媒体) | 体验一致性 | 复杂度 |
|---|---|---|---|---|---|---|
| **A. Tauri (Rust core + Web UI)** ✅ | ✅原生 | ✅(移动实验) | △(可单独 Web) | ✅Rust 调本地 | 高 | 中高 |
| B. Flutter (Dart) | ✅ | ✅ | △(Web 较弱) | △FFI 调本地 | 高 | 中 |
| C. Web (PWA) + 容器化 | △(Electron/PWA) | △(Capacitor) | ✅ | △(WASM/桥接) | 最高 | 低中 |
| D. 原生分栈（Swift/Kotlin/Web） | ✅ | ✅ | ✅ | ✅ | 低（三套代码） | 高 |

**落地架构：** `lmnotes-core`（Rust crate，承载 OKF 解析/校验、Tantivy 全文索引、向量索引、Provider 编排、合并、媒体队列）+ Tauri 壳 + Web UI（前端框架、编辑器、图谱库在 ADR 阶段细化）。移动/Web 后置（见 §15.3）。

### 8.2 共享核心边界（硬约束）
**核心逻辑（OKF 读写、索引、Provider 编排、合并）必须与 UI 解耦**，作为可独立测试的 Rust 库 `lmnotes-core`。UI 仅做视图与交互，通过 Tauri IPC 调核心。这是降低多端成本与保证 OKF 合规的硬约束——未来移动端经 UniFFI/FFI 复用同一核心。

### 8.3 目标平台与最低版本（按 O3：MVP 桌面优先）
- **MVP（桌面）：** Windows 10+ / macOS 12+ / 主流 Linux（Ubuntu 22.04+ / Fedora 38+ / Arch）
- **后置（M5+）：** 移动 iOS 16+ / Android 11+；Web（最近两年的 Chromium / Firefox / Safari）

---

## 9. 用户体验（UX）设计纲要

### 9.1 信息架构
三栏自适应布局（桌面）：
- **左：** Vault 切换 / 搜索 / 收藏 / 标签 / 每日笔记入口（可折叠）。
- **中：** 编辑器或当前视图（图谱/时间线）。
- **右：** 反向链接 / LLM 建议面板 / 大纲（可切换）。
- **底部右：** 状态栏（索引中 / LLM 在跑 / 同步状态）。
- **全局：** ⌘K 命令面板浮层。

### 9.2 高级感设计语言
- **克制：** 大量留白、低饱和中性色 + 单一强调色；线宽与圆角统一。
- **质感：** 细腻阴影分层、亚像素动效（链接生长、节点出现用 spring 动画，可关）。
- **专注：** 编辑器可进入"专注模式"（隐藏所有栏，仅正文与当前句子高亮）。
- **键盘：** Vim-可选模式（P2）、命令面板、所有快捷键可重绑。

### 9.3 关键交互流（与 US 对应）
- 快速捕获 → 转录 → 标签建议 → 一键归档（全键盘可完成）。
- 建议中心：列表 → diff 预览 → J/K 选择 → Enter 接受 / D 拒绝。
- 问答：右侧抽屉 → 输入 → 流式回答 → 引用悬浮卡 → 点击跳转。

---

## 10. 安全与隐私

- **本地优先：** 默认所有处理本地完成；云端为可选增强。
- **密钥管理：** API Key 存于操作系统钥匙串（Keychain / Credential Manager / libsecret），不明文落盘。
- **数据不外泄：** 除用户显式配置的云端 Provider 外，不上传任何内容；遥测默认关闭，开启时仅匿名事件。
- **沙箱：** Web 端用 OPFS / WASM 隔离；桌面端文件访问限定在 vault 目录。
- **供应链：** 锁定依赖版本（lockfile）；CI 跑 SBOM 与漏洞扫描。

---

## 11. 数据规格汇总（OKF 速查）

- **遵循规范：** Google OKF v0.1（官方 SPEC 见 `docs/okf/SPEC.v0.1.md`）
- 顶层布局（bundle 实例）：§3.2
- Concept（笔记）格式：§3.3（OKF 必填字段仅 `type`；LMNotes 扩展字段见 §3.3 表内）
- Concept ID 策略：§3.4（对外=路径，对内=frontmatter `id` 抗改名）
- 多模态资源：§3.5（二进制平铺 + 描述 concept）
- 合规约束：§3.6（含官方 §9 三条 + LMNotes 自加四条）
- **LMNotes 扩展 ID 规则：** 笔记 `nt_YYYYMMDD_HHMM_<4位base32>`；资源描述 concept `at_<slug>_<4位>`；全局唯一、不可变、与文件路径解耦。
- **版本声明：** Vault 根 `index.md` frontmatter 写 `okf_version: "0.1"`（OKF §11）。

---

## 12. 开发路线图（Roadmap，建议）

| 阶段 | 范围 | 关键交付 | 退出标准 |
|---|---|---|---|
| **M0 基础（2 周）** | Rust workspace、CI、`lmnotes-core`：OKF 解析/校验/ID 生成、Validator（对齐官方 §9）、`StorageBackend` trait + FsBackend、`lmnotes-cli` 校验工具 | 能创建 vault、读写 OKF 笔记、CLI `validate` 通过 | OKF round-trip + Validator + §9 合规测试通过 |
| **M1 桌面 MVP（拆为 M1a/M1b/M1c）** | | | |
| ↳ M1a 编辑器+索引层（2 周） | Tauri 壳、SolidJS、CodeMirror 6、快速捕获、图片、三层索引（SQLite/Tantivy+jieba/sqlite-vec）、增量索引、混合检索 | 能写笔记、保存即索引、搜索（纯本地） | §13 B 组前 2 环 |
| ↳ M1b LLM+建议中心（1.5 周） | Provider 抽象（Ollama/OpenAI）、路由、护栏、索引器接 LLM（摘要/标签/链接）、建议中心、就地改写+撤销、向量层填充 | LLM 建议可审阅接受 | §13 B 组 3–5 环 |
| ↳ M1c 图谱问答（0.5 周） | 向量 RAG、Chat with Vault、流式回答+引用 | 问答带可点击引用 | §13 B 组闭环 |
| **M2 多模态（3 周）** | 语音输入、Whisper 转录、音频/视频转录 concept、视觉描述、媒体队列 | US-1/US-4 可用 | §13 C 组 |
| **M3 图谱与问答（3 周）** | 向量 RAG、图谱视图、Chat with Vault、行动项抽取 | US-2/US-3 可用 | §13 D 组 |
| **M4 多 Provider 与云（2 周）** | 多 Provider 路由、隐私护栏、用量仪表盘 | US-6 可用 | §13 E 组 |
| **M5 跨端（4 周，后置）** | 文件级同步合并、Web、移动端速记 | 多端可用 | §13 F 组 |

> 时间为粗估；M1 完成后即可作为桌面预览版发布。技术栈：Tauri + Rust 核心 + Tantivy + sqlite-vec（决策 O4/O5）。MVP 全程桌面单平台（O3）。

---

## 13. 验收标准与成功指标

### 13.1 功能验收（A~F 组对应路线图）
- **A（基础）：** OKF 合规性 + round-trip（写→读→官方字段相等）100%；删 `.lmnotes/` 后基于 OKF bundle 重建索引无数据丢失；内置 Validator 与官方 §9 一致。
- **B（MVP）：** 可完成"写笔记→建链接→搜索→问答→改写→撤销"全链路；混合检索（Tantivy BM25 + sqlite-vec）P95 达 §6.1。
- **C（多模态）：** 1 小时音频转入完整转录+摘要+行动项 < 15 分钟人工校订。
- **D（图谱）：** 1000 篇笔记下图谱渲染 < 2s；问答引用准确率人工抽样 ≥ 80%。
- **E（多 Provider）：** 本地/云端 Provider 按任务分派可用；敏感条目护栏生效；用量仪表盘准确。
- **F（跨端）：** 多端打开同一 vault，文件级同步合并无丢失。

### 13.2 非功能验收
- 性能：§6.1 全部达标。
- 无锁定：用 `git clone` 的 vault 在裸 Python 脚本下能列出所有笔记与链接。

### 13.3 成功指标（产品层面，上线后观测）
- 7 日留存、日均捕获条数、LLM 建议接受率、问答使用频次、本地 vs 云端调用比（衡量隐私默认是否成立）。

---

## 14. 风险与缓解
| 风险 | 影响 | 缓解 |
|---|---|---|
| LLM 链接建议噪声大，用户失去信任 | 核心价值受损 | 建议中心 + 可调激进/保守档 + 反馈学习；优先精确率 |
| 本地 LLM 性能不足导致体验差 | 桌面低端机不可用 | 任务分级，小模型做高频任务，提供"最小可用模型"配置与降级 |
| OKF 版本演进破坏兼容 | 用户数据升级受损 | 跟踪官方 `okf_version` minor/major 升级（§11），提供自动迁移脚本；读取时按官方 §9 向前忽略未知字段 |
| 多平台成本爆炸 | 交付延迟 | 共享核心库 + 单 UI 栈；移动端后置 |
| 云端隐私事件 | 信任危机 | 默认本地；条目级开关；敏感关键词护栏；脱敏日志 |

---

## 15. 开放决策点（需你确认后再进入架构设计）

> 这些是影响后续架构与实现计划的关键岔路。请逐条给出倾向，或在 §15.6 补充。

### 15.1 O1 — OKF 对齐策略 ✅ 已决策（2026-06-21）
**决策：严格遵循 Google 官方 OKF v0.1，通过 producer-defined keys 做合规扩展。**
- 已确认 OKF 是 Google Cloud 2026-06-12 发布的开放规范（`GoogleCloudPlatform/knowledge-catalog`）。
- LMNotes 产出的 Vault 必须是 OKF-conformant bundle（满足官方 §9）。
- 多模态、稳定 ID 等需求通过官方 §4.1 允许的扩展字段实现，不 fork 规范。
- 详见 §3.0 / §3.4 / §3.5；本地存档 `docs/okf/SPEC.v0.1.md`。
- **遗留监控项：** OKF 仍为 v0.1 Draft，需关注官方 minor/major 版本升级（§11 语义），升级时评估对已存 Vault 的迁移影响。

### 15.2 O2 — 反向链接的呈现方式 ✅ 已决策（2026-06-21）
**决策：选 A —— 反向链接仅运行时计算，不写入任何 frontmatter。**
- 依据：OKF §5.3 明确规定链接关系由"扫描正文 markdown link"得出，选 A 与规范天然一致。
- 实现：派生索引器扫描全 bundle 的 markdown link 构建邻接表（存 `.lmnotes/index.sqlite`），UI 反向链接面板从该派生表实时查询。
- 增量可见性补偿：用户若需 git 可见，可手动/定时由应用代写各目录的 `index.md`（OKF §6 允许位置），作为可选导出动作（非默认，避免写放大）。
- **明确不做**：不在 concept frontmatter 写 `backrefs:` 字段（偏离官方最小化精神、增加写放大与同步冲突）。

### 15.3 O3 — 首发平台范围 ✅ 已决策（2026-06-21）
**决策：MVP 聚焦桌面单平台（Windows / macOS / Linux），移动端与 Web 后置。**
- 理由：桌面端本地 LLM、文件系统监听、全局快捷键、媒体处理能力最完整，风险最低，可在 4–6 周内达成 MVP（对应 §12 路线图 M0–M4）。
- 移动端（iOS/Android）与 Web 列为 M5+ 阶段：移动端 LLM 依赖本地桥接（如 MLC-LLM/iOS MLX），Web 端受 OPFS/WASM 沙箱限制，均需独立工程投入。
- **硬约束延续**：§8.2 要求核心逻辑（OKF/索引/Provider）与 UI 解耦，确保后置多端时核心可直接复用。

### 15.4 O4 — 技术栈选型 ✅ 已决策（2026-06-21）
**决策：Tauri（Rust 核心 + Web UI）。**
- **Rust 核心（`lmnotes-core` crate）：** 承担 OKF 解析/校验、Tantivy 全文索引、向量索引、Provider 编排、合并、媒体队列。作为可独立测试的库（§8.2 硬约束），未来移动端可经 FFI/UniFFI 复用。
- **Web UI（前端框架待 ADR 细化，候选 Solid/Svelte/React）：** 仅做视图与交互，通过 Tauri IPC 调核心。高级感动效依赖成熟 Web 生态。
- 理由：包体小（vs Electron ~100MB）、内存低、安全（白名单文件访问）、本地能力完整、跨平台一致。
- **衍生子决策（写入 ADR-004）：** 前端框架、状态管理、Markdown 编辑器（候选 CodeMirror 6 / Tipap on ProseMirror）、图谱可视化（候选 D3 / Cytoscape.js / vis-network）在架构阶段定。

### 15.5 O5 — 索引存储 ✅ 已决策（2026-06-21）
**决策：SQLite（元数据）+ Tantivy（全文）+ sqlite-vec（向量），中文分词用 jieba-rs。**
> 用户关键反馈：SQLite FTS5 对中文支持不佳（`unicode61` 按字切、jieba tokenizer 集成脆弱），故全文检索改用 Rust 原生搜索引擎。

| 子系统 | 选型 | 角色 |
|---|---|---|
| 元数据/关系 | **SQLite** | concept 元信息、邻接表、LLM 建议队列、快照索引；事务安全 |
| 全文检索 | **Tantivy v0.26**（Rust 版 Lucene，1443 万下载，库而非服务器，可嵌入） | 中英混排全文索引，支持自定义 tokenizer |
| 中文分词 | **jieba-rs v0.10** | 作为 Tantivy 的 tokenizer，按词切分中文 |
| 向量检索 | **sqlite-vec**（SQLite 扩展，单文件） | 语义检索、RAG |

- **三层关系：** SQLite 是主存（concept→path→meta）；Tantivy 索引正文文本（写入时增量）；sqlite-vec 存向量。三者均为派生数据，可从 OKF bundle 重建（§3.6 合规要求 1）。
- **检索融合：** 查询时 BM25（Tantivy）+ 向量（sqlite-vec）得分融合（如 RRF），由核心层统一返回。
- **混合检索的实时性：** 增量索引器在笔记保存后 < 1s 内更新 Tantivy + 向量（对应 FR-LLM-01）。

### 15.6 O6 — 其它约束 ✅ 已决策（2026-06-21）
| 约束 | 决策 | 衍生影响 |
|---|---|---|
| **开源** | MIT 或 Apache-2.0（二选一，ADR 定） | 已核查核心依赖许可证均兼容：Tauri（Apache-2.0/MIT）、Tantivy（MIT）、jieba-rs（MIT）、sqlite-vec（MIT）、Rust 主工具链（MIT/Apache）。**注意**：若用 GPL/AGPL 依赖需隔离，选型时显式排除；Ollama/whisper.cpp 等 LLM 引擎以独立进程调用、不静态链接，规避许可传染。 |
| **UI 语言** | 中英双语平等 | 从 M0 起接入 i18n 框架（候选 `react-i18next`/`@lingui`，前端框架定后细化）；所有文案走 key 不硬编码；默认跟随系统语言，可手动切换。笔记 `language` 字段独立驱动分词（jieba-rs）与模型选择。 |
| **本地 LLM 环境** | 不确定，需 fallback | 首次启动检测本地 Ollama/whisper.cpp 可用性：可用→默认本地优先；不可用→引导配置云端 Provider（GLM/OpenAI 兼容）作为 fallback，并明确告知隐私含义。默认禁用云端，需用户显式授权（FR-LLM-08）。 |
| **工期** | 6 个月完整 MVP（M0–M4） | 按 §12 路线图节奏推进；M1 后发桌面预览版，M4 完成全 MVP；M5 跨端为后续阶段。每个里程碑配独立实现计划 + 验收组。 |

> 至此 §15 全部决策完成。下一步：沉淀为 ADR（`docs/adr/`）→ 按 M0 里程碑编写第一份实现计划（`writing-plans`）。

---

## 16. 术语表
- **OKF (Open Knowledge Format)**：Google Cloud 发布的开放知识格式规范 v0.1，"带 YAML frontmatter 的 markdown 文件目录"。LMNotes 遵循它（见 §3），官方原文存档 `docs/okf/SPEC.v0.1.md`。
- **Bundle**：OKF 的分发单元 = 一个目录树。LMNotes 中 **Vault = OKF Bundle**。
- **Concept**：OKF 的最小知识单元 = 一个 `.md` 文件。LMNotes 中一条笔记/一份转录稿都是一个 concept。
- **Concept ID**：OKF §2 定义为"文件路径去 `.md`"。LMNotes 不改动这一定义，并额外用 frontmatter `id` 字段作为内部稳定引用键（§3.4）。
- **Vault**：LMNotes 的一个知识库实例 = 一个本地目录 = 一个 OKF Bundle。
- **笔记（Note）**：LMNotes 中 `type: note` 等 concept 的俗称，聚焦单一概念、有稳定 `id`。
- **Provider**：一个可调用的 LLM/媒体引擎端点（本地或云端）。
- **建议中心**：统一审阅 LLM 产出（链接/摘要/标签）的界面。
- **派生数据**：可从 OKF bundle 真源（`notes/` + `transcripts/` + `assets/`）重建的数据（索引、向量、缓存），存 `.lmnotes/`。
- **Producer-defined keys**：OKF §4.1 允许生产者自加的 frontmatter 字段。LMNotes 的所有扩展字段（`id`/`aliases`/`status`/`language` 等）均属此类，不破坏合规。

---

## 附录 A：与 writing-plans 的衔接
本 PRD 经 §15 决策确认后，下一步将：
1. 由 `brainstorming`/架构设计沉淀为若干 **ADR**（架构决策记录），存于 `docs/adr/`。
2. 按 §12 路线图的每个里程碑产出 **实现计划**（`docs/superpowers/plans/YYYY-MM-DD-<milestone>.md`），由 `writing-plans` 编写，采用 TDD + 任务粒度任务清单。
3. 每个里程碑再由 `subagent-driven-development` 或 `executing-plans` 执行。
