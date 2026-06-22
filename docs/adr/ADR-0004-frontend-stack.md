# ADR-0004: 前端框架 / 编辑器 / 图谱库选型

| 状态 | Accepted (rev2: M1a 改用 CodeMirror 6) |
| 日期 | 2026-06-21（rev2: 2026-06-22） |
| 关联 | PRD §9, §15.4（O4）; ADR-0002 |
| 决策者 | 调研推荐（GitHub 数据核查 2026-06-21），待用户最终确认 |

> 本 ADR 在 PRD §15.4 标记的"前端框架/编辑器/图谱库在 ADR 阶段细化"处落地。选型基于 GitHub 仓库实时数据（stars / 最近推送 / 许可证），数据抓取于 2026-06-21。

## 背景（Context）

Tauri 架构（ADR-0002）下，前端是独立 Web 工程。需选定四类技术：
1. **UI 框架**：组件化、状态管理、Tauri IPC 调用便利。
2. **Markdown 编辑器**：所见即所写、双向链接补全、高性能（§6.1 < 16ms）、可扩展（自定义 LLM 改写菜单）。
3. **图谱可视化**：力导向图、节点可上千（§13.1 D 组 < 2s 渲染）、可交互。
4. **i18n 框架**：中英双语平等（O6b），M0 起接入。

约束：许可证 MIT/Apache（O6a）；与 Tauri + Rust 核心（ADR-0002）契合；高级感动效易实现（§9.2）。

## 决策（Decision）

| 维度 | 选型 | 理由 |
|---|---|---|
| **UI 框架** | **SolidJS** | 性能（细粒度响应式，无 vdom diff）、JSX 生态、bundle 小；与 Tauri 契合 |
| **状态管理** | **Solid 内置 stores + signals** | 框架原生，无需额外库；服务端状态经 Tauri IPC 拉取/订阅 |
| **Markdown 编辑器** | **Tiptap**（基于 ProseMirror） | 插件生态成熟、可定制节点/Mark（双向链接补全、LLM 内联改写）、所见即所写 |
| **图谱可视化** | **Cytoscape.js** | 性能稳定（上千节点）、API 完善、布局算法齐全、MIT |
| **i18n** | **@solid-primitives/i18n** | 与 Solid 原生契合，轻量；中英文案走 key 不硬编码 |

### 关键设计

1. **编辑器与 OKF 的桥**：Tiptap 文档模型 ↔ markdown 双向转换用 **markmap/marked 或 remark** 管道；保存时序列化为 OKF concept（含 frontmatter）。双向链接节点是自定义 Tiptap 节点，键入 `[](` 触发补全，补全项由核心层 `resolve_path` 提供（ADR-0001 决策 6）。
2. **图谱性能**：Cytoscape.js 用 `cola` 或 `cose-bilinear` 布局；> 500 节点启用 WebGL 渲染器；增量更新经 IPC 事件。
3. **i18n 接入**：所有文案 `t('key')`；默认语言跟随系统，可手动切换（O6b）；笔记 `language` 字段独立驱动分词（ADR-0003）。
4. **样式系统**：CSS 变量 + 主题 token，深/浅色/跟随系统（FR-UX-05）；动效用原生 CSS transition / Motion One，克制（§9.2）。

### 集成风险点（评审 F6）

- **Tiptap + SolidJS 无官方封装**：Tiptap 官方只提供 React/Vue 绑定，Solid 需**手动接线**——通过 `createEffect` 绑定 Tiptap `Editor` 实例到 DOM 节点，用 Solid signals 订阅 Tiptap 事务更新。这是已知额外工作（约 1 个 UI 模块的量），但 Tiptap 的 headless 设计（纯逻辑核心 `@tiptap/core` + `ProseMirror`）使手动集成可行，社区有参考实现。
  - **缓解**：M0 起把"Solid-Tiptap 绑定层"作为独立模块 `editor/`，文档化生命周期（mount/update/unmount）；若集成阻力过大，**fallback 为 CodeMirror 6**（ADR-0004 替代方案已列为源码模式备选）。

### 修订（rev2, 2026-06-22）：M1a 采用 CodeMirror 6

**M1a 实际采用 CodeMirror 6 替代 Tiptap**，理由：
- 评审 F6 的集成风险（Solid+Tiptap 手动接线）在 M1a 紧迫时间线下不值得冒。
- CodeMirror 6 的 markdown 模式成熟、纯文本性能极佳、与 SolidJS 集成简单（无框架绑定需求）。
- 双向链接补全、就地改写等富文本特性可用 CodeMirror 的扩展机制实现（装饰/插件）。

**Tiptap 不放弃，推迟**：若 M2/M3 需要更强的所见即所得富文本（表格编辑、嵌入块等），届时重新评估迁移到 Tiptap。届时 Solid-Tiptap 绑定层作为独立模块开发。当前 M1a 的 CodeMirror 选型记录于 `docs/superpowers/plans/2026-06-22-m1a-editor-index.md`。

**markdown round-trip**：Tiptap 是富文本模型，脚注/数学/表格等 GFM 元素需自定义节点，round-trip 可能丢格式。
  - **缓解**：核心层做 markdown 规范化校验（ADR-0001 Validator 扩展）；不支持的元素降级为代码块并提示用户。

### 调研数据（2026-06-21 核查）

| 库 | Stars | 最近推送 | 许可证 |
|---|---|---|---|
| SolidJS | 35,638 | 2026-06-17 | MIT |
| Tiptap | 37,314 | 2026-06-19 | MIT |
| Cytoscape.js | 11,057 | 2026-06-18 | MIT |

三者均活跃维护、MIT 许可、生态成熟。

## 后果（Consequences）

**正面：**
- SolidJS 细粒度响应式天然达成 < 16ms 输入延迟（§6.1）。
- Tiptap 插件机制让 LLM 内联改写、双向链接、自定义节点扩展成本低。
- Cytoscape.js 满足千节点图谱渲染（§13.1 D 组）。
- 全 MIT，兼容开源（O6a）。

**负面：**
- SolidJS 生态体积小于 React，部分 UI 组件库需自建或适配。缓解：用 Tailwind/未UI 等 headless 方案；Tauri 桌面端组件库需求本就有限。
- Tiptap 是富文本模型，markdown round-trip 需小心（如脚注/数学）。缓解：核心层做 markdown 规范化校验，不支持的元素降级为代码块。
- ProseMirror 学习曲线陡。缓解：编辑器相关代码集中在一个模块，文档化扩展点。

**缓解措施汇总：** headless 组件库、markdown 规范化、编辑器模块文档化。

## 考虑过的替代方案（Alternatives）

- **UI 框架**
  - React：生态最大但 vdom diff 在长列表/编辑器场景不如 Solid；运行时更大。
  - Svelte/SvelteKit：优秀但 Tauri 集成案例少于 Solid/Rust 侧；编译模式与某些库兼容性偶有问题。
  - Vue：可行但响应式粒度不如 Solid 精细。
- **编辑器**
  - CodeMirror 6：性能极佳、纯文本向，但"所见即所写"富文本体验弱于 Tiptap；适合代码向笔记。**保留为"vim 模式/源码模式"的备选**。
  - Monaco：过重（VSCode 内核），不适合笔记。
  - Lexical（Meta）：优秀但生态/文档不如 Tiptap 成熟，中文社区资源少。
- **图谱库**
  - vis-network：易用但大规模性能不如 Cytoscape。
  - D3 force：灵活但需大量手写，开发成本高。
  - Sigma.js / graphology：WebGL 性能强，但 API 偏底层，交互开发量大。**保留为超大图谱（万节点）的升级路径**。
- **i18n**
  - react-i18next：React 生态，不适用 Solid。
  - Lingui：优秀但宏观编译模式与 Solid 契合不如 primitives。
