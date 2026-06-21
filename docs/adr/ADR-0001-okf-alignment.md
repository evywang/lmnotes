# ADR-0001: OKF 对齐与合规扩展边界

| 状态 | Accepted |
| 日期 | 2026-06-21 |
| 关联 | PRD §3, §15.1（决策 O1）; ADR-0003 |
| 决策者 | 用户（确认）+ 调研结论 |

## 背景（Context）

LMNotes 需要"无锁定"承诺：用户的笔记必须能脱离应用本身被人类阅读、被其他工具处理、被任意 LLM 理解（PRD §1.2 P3、§3.5）。这要求一个**开放、有权威背书、跨工具可互操作**的知识格式。

格式候选有三类：
1. **Google OKF v0.1**（2026-06-12 发布）：官方定义为"human- and agent-friendly format"，markdown 文件目录 + YAML frontmatter，明确面向"enrichment agents 写入 / consumption agents 读取"。本地存档见 `docs/okf/SPEC.v0.1.md`。
2. **社区事实标准**（Obsidian Flavor / Foam / Logseq）：流行但有 fork 差异，无权威 SPEC，`[[wikilink]]` 等语法不互通。
3. **自创格式**：自由度最大，但生态冷启动、重复造轮子、无互操作性。

LMNotes 还有两个规范未直接覆盖的硬需求：
- **稳定 ID 抗改名/移动**：用户重命名笔记时，所有引用不应断裂。
- **多模态**（图片/音频/视频）作为一等公民：规范 v0.1 只涉及 markdown。

## 决策（Decision）

**严格遵循 Google 官方 OKF v0.1，通过规范明确允许的 producer-defined frontmatter keys 做合规扩展。**

具体边界：
1. **产出的每个 Vault 必须满足 OKF §9 Conformance**（官方三条硬约束：每个 `.md` 有 frontmatter、有 `type`、`index.md`/`log.md` 符合 §6/§7）。
2. **必填 frontmatter 只有 `type`**（官方）；链接用标准 markdown link（官方 §5）；Concept ID = 文件路径（官方 §2）；版本声明 `okf_version` 在根 `index.md`（官方 §11）。
3. **LMNotes 扩展字段**（`id` / `aliases` / `status` / `language` / `created` 等）归类为 OKF §4.1 允许的 *producer-defined keys*，规范明文："Producers MAY include any additional keys. Consumers SHOULD preserve unknown keys when round-tripping."
4. **合规性自检规则**：任何 LMNotes concept 删掉所有扩展字段后，仍是 100% OKF-conformant。这是核心库的回归测试断言。
5. **多模态扩展**：二进制资源平铺存 `assets/`（规范不约束），其语义（转录/OCR/描述）写成独立 OKF concept（`type: transcript` 等），用官方 `resource` 字段指向二进制 URI。不引入专有 sidecar 格式。
6. **ID 策略（对外路径、对内 id）**：
   - **Concept ID 对外仍是文件路径**（不动官方 §2 定义）。
   - **链接里存的是路径**（官方 §5 markdown link 形式，如 `[文本](/notes/ai/llm-wiki.md)`），符合规范，链接内容不含 id。
   - **frontmatter 扩展字段 `id`**（`nt_<date>_<rand>`，全局唯一不可变）作为应用内部主键，**不进链接**。
   - **改名/移动不断链机制**：应用维护"id → 当前路径"的派生映射（存 `.lmnotes/index.sqlite`，见 ADR-0003）。当用户重命名/移动笔记 X：
     1. 索引器用 X 的 `id` 反查所有正文链接指向 X 旧路径的笔记（基于邻接表，见 ADR-0003）；
     2. 把这些链接里的旧路径改写为新路径；
     3. 更新"id → 路径"映射。
   - 这样**链接始终是合规的 markdown 路径链接**，又因 id 派生映射而能自动跟随改名。用户在 Tiptap 里键入 `[](` 触发补全时，核心层提供 `resolve_path(title|alias|id) → 当前路径`。
7. **内置 Validator** 与官方 §9 + 社区 `openknowledgeformat.com/validator` 规则一致，可在应用内一键校验。

## 后果（Consequences）

**正面：**
- 真正的无锁定：用户的 Vault 任何 OKF 消费者（包括 Google 自家 knowledge-catalog 工具）都能读。
- 享受官方生态：Validator、文档、未来 minor/major 升级路径。
- 扩展字段命名空间清晰，升级 OKF 版本时不易冲突。
- 多模态、稳定 ID 等需求满足且不偏离规范。

**负面：**
- 受规范演进约束：OKF 仍为 v0.1 Draft，官方若做 breaking change（major bump）需迁移。缓解：跟踪 `okf_version`，按官方 §11 语义处理；读取时按 §9 容错忽略未知字段。
- 链接是标准 markdown link（不是 `[[wikilink]]`），编辑器需自建补全逻辑。缓解：核心库提供 `resolve_path(title|alias|id) → 当前路径` API（见决策 6）。
- Obsidian/Foam 导入需把 `[[x]]` 转成 `[x](/path/x.md)`（已计入 FR-STORE-06）。

**缓解措施汇总：** ID 解析由核心库承担；版本迁移提供脚本；Validator 内置。

## 考虑过的替代方案（Alternatives）

- **自创 OKF 规范并发布**：拒绝。自由度收益 < 生态与互操作性损失；且"OKF"这个命名已被 Google 占用，自创会混淆。
- **对齐 Obsidian Flavor 超集**：拒绝。无权威 SPEC、各工具 fork 差异、`[[wikilink]]` 非标准 markdown 链接、不利于"agent-readable"定位。Obsidian 仅作为**导入源**（FR-STORE-06）。
- **Concept ID 直接用路径、不引入 `id` 字段**：拒绝。无法满足"改名/移动不断链"需求。当前方案"路径对外 + id 对内"两全。
- **多模态用 sidecar `.okf.md`**：拒绝。引入专有格式偏离规范，且文件名耦合。当前"描述性 concept + resource URI"更符合规范精神。
