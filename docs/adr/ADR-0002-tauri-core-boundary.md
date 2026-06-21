# ADR-0002: Tauri 架构与 lmnotes-core 边界

| 状态 | Accepted |
| 日期 | 2026-06-21 |
| 关联 | PRD §8, §15.3（O3）, §15.4（O4）; ADR-0001, ADR-0003, ADR-0005 |
| 决策者 | 用户（确认） |

## 背景（Context）

LMNotes 需要跨平台、本地优先、LLM 原生（PRD §1.1, §8）。约束 forces：

1. **性能敏感**：OKF 解析、Tantivy 索引、向量计算、Provider 编排需低延迟（§6.1 编辑器 < 16ms、搜索 P95 < 100ms）。
2. **本地能力完整**：直接调用 Ollama、whisper.cpp、文件系统监听、OS 钥匙串。
3. **数据安全**：文件访问需限定在 vault 目录（沙箱化，§10）。
4. **包体与资源**：本地优先应用不应臃肿（Electron 基线 ~100MB+）。
5. **多端演进**：MVP 桌面（O3），但移动/Web 后置——核心逻辑必须可复用（§8.2 硬约束）。
6. **开源（O6a）**：依赖许可证需 MIT/Apache 兼容。

候选技术栈见 PRD §8.1 对比表：Tauri / Flutter / Web+Capacitor / 原生分栈。

## 决策（Decision）

**采用 Tauri：Rust 核心 `lmnotes-core` + Web UI + Tauri 壳。**

### 架构分层（硬边界）

```
┌─────────────────────────────────────────────┐
│  Web UI（前端，独立构建）                     │  ← 视图与交互，无业务逻辑
│  （前端框架/编辑器/图谱见 ADR-0004）          │
├─────────────────────────────────────────────┤
│  Tauri IPC 层（薄）                          │  ← 命令注册、事件、权限白名单
├─────────────────────────────────────────────┤
│  lmnotes-core（Rust crate，可独立测试）       │  ← 所有业务逻辑
│   ├─ okf        OKF 解析/校验/ID/Validator   │
│   ├─ store      Vault/文件 IO/资源去重        │
│   ├─ index      Tantivy/sqlite-vec/SQLite    │  （ADR-0003）
│   ├─ llm        Provider 抽象/路由/护栏      │  （ADR-0005）
│   ├─ media      转录/OCR/视觉队列            │
│   └─ sync       三向合并                     │
├─────────────────────────────────────────────┤
│  Tauri Runtime + 平台原生能力                │
└─────────────────────────────────────────────┘
```

### 边界规则（写进 CI/lint）

1. **核心库零 UI 依赖**：`lmnotes-core` 的 `Cargo.toml` 不得依赖任何 Tauri 类型或 web 框架；可编译为纯 `cargo test`。
2. **IPC 是唯一桥**：前端只通过 `#[tauri::command]` 暴露的命令集访问核心；核心层定义独立的 **DTO 类型**（`dto` 模块，专为 IPC 设计的扁平结构），与内部领域模型分离——内部类型不直接 Serialize 给前端，避免泄漏内部实现结构。
3. **文件访问白名单**：Tauri `fs` scope 限定为当前 vault 路径；核心层所有文件操作走受控的 `store` 模块，不直接 `std::fs` 越界（§10 沙箱）。
4. **存储后端抽象（关键，对应 Web 复用）**：核心层定义 `StorageBackend` trait，所有文件/索引 IO 经它：
   ```rust
   #[async_trait]
   pub trait StorageBackend: Send + Sync {
       async fn read_file(&self, rel_path: &Path) -> Result<Vec<u8>>;
       async fn write_file(&self, rel_path: &Path, data: &[u8]) -> Result<()>;
       async fn list_dir(&self, rel_path: &Path) -> Result<Vec<DirEntry>>;
       async fn watch(&self, rel_path: &Path) -> Result<WatchStream>;
       async fn rename(&self, from: &Path, to: &Path) -> Result<()>;
       // ... 索引后端同理：trait IndexBackend { sqlite, tantivy, vec }
   }
   ```
   - **桌面/Tauri**：`FsBackend`（`std::fs` + notify 监听）+ `NativeIndexBackend`（SQLite/Tantivy/sqlite-vec 文件）。
   - **Web（M5）**：`OpfsBackend`（OPFS 文件）+ `WasmIndexBackend`（IndexedDB/内存索引，Tantivy WASM 构建）。**同一套核心逻辑，只换后端实现**。
   - **移动（M5）**：UniFFI 暴露核心，`FsBackend` 走平台文件系统。
   - **约束**：核心层任何模块**禁止直接 `use std::fs`**，必须经 `StorageBackend`——CI lint 强制（clippy 自定义规则或简单的 `cfg` + 模块边界检查）。这是"M5 跨端核心零改动"承诺的技术保证。
5. **重计算在核心**：解析、索引、向量、合并等 CPU 密集任务在 Rust 侧（核心内）执行，前端只渲染结果——避免 IPC 序列化大对象。
6. **流式接口**：LLM 流式输出、转录进度通过 Tauri 事件（`emit`）推前端，避免长查询阻塞。
7. **核心复用承诺（修正后）**：`lmnotes-core` 经 **`StorageBackend`/`IndexBackend` trait 抽象**实现跨端复用——桌面用 fs 后端，Web 用 OPFS+WASM 后端，移动经 UniFFI 用 fs 后端。**注意**：trait 是承诺的前提；若核心直接 `std::fs`，则 Web 端无法复用（这正是 F2 评审发现的风险，已通过决策 4 规避）。

### 包结构

```
lmnotes/
├── Cargo.toml                    # workspace
├── crates/
│   ├── lmnotes-core/             # 业务核心（无 UI 依赖）
│   │   └── src/
│   │       ├─ okf/               # OKF 解析/校验/ID/Validator
│   │       ├─ store/             # 文件/资源去重（经 StorageBackend）
│   │       ├─ index/             # Tantivy/sqlite-vec/SQLite（经 IndexBackend）
│   │       ├─ llm/               # Provider 抽象/路由/护栏
│   │       ├─ media/             # 转录/OCR/视觉队列
│   │       ├─ sync/              # 三向合并
│   │       ├─ backend/           # StorageBackend/IndexBackend trait + FsBackend/NativeIndexBackend
│   │       └─ dto/               # IPC 专用扁平 DTO（与领域模型分离）
│   ├── lmnotes-cli/              # 可选 CLI（调试、Validator 命令行）
│   └── lmnotes-tauri/            # Tauri 壳 + IPC 命令
├── apps/
│   └── desktop/                  # Tauri 桌面应用 + 前端（ADR-0004）
└── docs/
```

## 后果（Consequences）

**正面：**
- 包体小（Tauri 基线 ~10MB vs Electron ~100MB）、内存低。
- Rust 核心保证 §6.1 性能指标；安全（类型/所有权/白名单）。
- 核心可独立测试，UI 与逻辑解耦，M5 跨端核心零改动。
- 全栈依赖 MIT/Apache 兼容（Tauri = MIT/Apache，§15.6 O6a 已核查）。

**负面：**
- Rust + Web 双语言栈，团队需具备两者能力；前端通过 IPC 无法直接调 Rust 库（需包 DTO）。
- Tauri Mobile 仍较新，移动端 M5 阶段可能踩坑。缓解：M0–M4 桌面期持续关注 Tauri Mobile 稳定性，移动端必要时用 UniFFI 直出原生壳而非 Tauri Mobile。
- IPC 序列化成本：大列表需分页/游标化。缓解：核心层提供分页 API。

**缓解措施汇总：** DTO 规范、流式事件、分页 API、移动端双轨预案。

## 考虑过的替代方案（Alternatives）

- **Electron + Web**：拒绝。包体/内存过高（违背"极简"与本地优先的资源观）；Node 后端做 OKF/Tantivy 性能不如 Rust。
- **Flutter**：拒绝。单一语言栈一致性诱人，但本地 LLM/whisper 需 FFI/Dart bindings（生态弱），Web 端较弱；且与 Tantivy/sqlite-vec 等 Rust 生态契合度差。
- **Web PWA + Capacitor**：拒绝。本地能力最弱（OPFS/WASM 沙箱限制文件监听、全局快捷键、whisper），违背"本地优先 LLM 原生"核心定位。
- **原生分栈**（Swift/Kotlin/Web）：拒绝。三套代码维护成本高，核心无法复用，违背 §8.2 硬约束。
