# Architecture Decision Records (ADR)

> ADR 记录 LMNotes 项目中具有长期影响的架构决策：背景、决策、后果、替代方案。
> 一旦写入即历史档案，**不被修改**；如决策变更，新增 ADR 标记"Supersedes ADR-0NNN"，原 ADR 标记状态为 Superseded。

## 模板

每个 ADR 文件名：`ADR-0NNN-<short-kebab-title>.md`，结构：

```markdown
# ADR-0NNN: 标题

| 状态 | Proposed / Accepted / Superseded by ADR-0MMM |
| 日期 | YYYY-MM-DD |
| 关联 | PRD §X.Y; 决策 O-z; 关联 ADR-0NNN |
| 决策者 | （记录拍板来源） |

## 背景（Context）
触发该决策的问题、约束、 forces。

## 决策（Decision）
做了什么，以及为什么。

## 后果（Consequences）
- 正面：
- 负面：
- 缓解：

## 考虑过的替代方案（Alternatives）
列出并说明为何未选。
```

## 索引

| # | 标题 | 状态 | 日期 |
|---|---|---|---|
| [0001](ADR-0001-okf-alignment.md) | OKF 对齐与合规扩展边界 | Accepted (rev1: F1 ID策略澄清) | 2026-06-21 |
| [0002](ADR-0002-tauri-core-boundary.md) | Tauri 架构与 lmnotes-core 边界 | Accepted (rev1: F2 StorageBackend trait + F3 DTO) | 2026-06-21 |
| [0003](ADR-0003-index-three-layer.md) | 索引三层架构（SQLite/Tantivy/sqlite-vec） | Accepted (rev1: F4 Tantivy更新语义 + F5 邻接增量) | 2026-06-21 |
| [0004](ADR-0004-frontend-stack.md) | 前端框架/编辑器/图谱库选型 | Accepted (rev1: F6 Tiptap+Solid风险; rev2: M1a改用CodeMirror 6) | 2026-06-21 |
| [0005](ADR-0005-llm-provider-guardrails.md) | LLM Provider 抽象与隐私护栏 | Accepted (rev1: F7 能力门控trait) | 2026-06-21 |

## 与 PRD 的关系
- PRD `docs/specs/PRD.md` §15 决策（O1–O6）是这些 ADR 的输入。
- ADR 把决策落到可执行的架构约束与边界定义，作为实现计划（`docs/superpowers/plans/`）的依据。
- 决策冲突时，**PRD 描述"做什么"，ADR 描述"怎么做"**；ADR 优先级更高（它更新更细）。
