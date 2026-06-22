# M1a: 编辑器 + 三层索引层 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 M0 核心库之上，搭建 Tauri 桌面应用骨架与 SolidJS 前端，实现 Markdown 编辑器（CodeMirror 6）、快速捕获、图片资源管理，以及 ADR-0003 定义的三层派生索引（SQLite 元数据 + Tantivy/jieba-rs 全文 + sqlite-vec 向量预留）与增量索引器、混合检索命令。**M1a 全程纯本地、无 LLM**（LLM 留给 M1b）。

**Architecture:**
- 新增 `crates/lmnotes-tauri`：Tauri 2 IPC 壳，`#[tauri::command]` 暴露核心层能力。
- 新增 `apps/desktop`：Tauri 2 应用 + SolidJS 前端（Vite）。
- `lmnotes-core` 扩展 `index` 模块：`IndexBackend` trait + `SqliteIndex` 实现（元数据 + sqlite-vec）+ `TantivyIndex` 实现（全文）。
- 新增 `crates/lmnotes-core/src/indexer`：增量索引器，监听 concept 保存事件，更新三层索引（文本层；向量层在 M1b 接 embed 后填充）。

**Tech Stack:** Tauri 2.11；Rust：`rusqlite` 0.40（bundled）+ `sqlite-vec` 0.1.9 + `tantivy` 0.22 + `jieba-rs` 0.7 + `notify` 6（文件监听）；前端：`solid-js` 1.9 + `@codemirror/*` 6.x（state/view/lang-markdown/commands）+ `vite` 5 + `@tauri-apps/api` 2.x。

> **编辑器选型说明（ADR-0004 修订）：** ADR-0004 原选 Tiptap，但评审 F6 标注 Solid+Tiptap 无官方封装、需手动接线有风险。本计划改用 **CodeMirror 6**（纯文本 markdown 模式，与 SolidJS 集成简单、性能优异）。Tiptap 推迟到 M2/M3 视情况再评估。**需在计划完成后同步更新 ADR-0004**（见本计划末"后续动作"）。

---

## File Structure

执行本计划新增/修改的文件：

```
lmnotes/
├── Cargo.toml                                  # [T1] workspace + tauri 依赖
├── crates/
│   ├── lmnotes-core/
│   │   ├── Cargo.toml                          # [T3] 加 rusqlite/sqlite-vec/tantivy/jieba-rs/notify
│   │   └── src/
│   │       ├── lib.rs                          # [T3] 加 pub mod index/indexer/search
│   │       ├── backend/mod.rs                  # [T2] 加 IndexBackend trait
│   │       ├── search/                         # [T7] 混合检索
│   │       │   ├── mod.rs
│   │       │   └── rrf.rs                      # Reciprocal Rank Fusion
│   │       ├── index/                          # [T3-T6] 三层索引实现
│   │       │   ├── mod.rs                      # [T3] IndexBackend trait 重导出
│   │       │   ├── sqlite.rs                   # [T4] SQLite 元数据 + sqlite-vec
│   │       │   ├── tantivy.rs                  # [T5] Tantivy + jieba tokenizer
│   │       │   └── schema.rs                   # [T3] 共享 schema 常量
│   │       └── indexer/                        # [T6] 增量索引器
│   │           └── mod.rs
│   └── lmnotes-tauri/                          # [T2] IPC 壳
│       ├── Cargo.toml
│       ├── build.rs
│       └── src/
│           ├── lib.rs                          # [T2] tauri::Builder + 命令注册
│           └── commands.rs                     # [T2][T7] #[tauri::command] 定义
├── apps/desktop/                               # [T1] Tauri 应用
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   ├── tauri.conf.json                         # [T1] Tauri 配置（窗口/权限）
│   ├── src/
│   │   ├── main.tsx                            # [T8] Solid 入口
│   │   ├── App.tsx                             # [T8] 三栏布局
│   │   ├── editor/Editor.tsx                   # [T9] CodeMirror 封装
│   │   ├── editor/markdown.ts                  # [T9] markdown 扩展组装
│   │   ├── capture/Capture.tsx                 # [T10] 快速捕获浮窗
│   │   ├── store/vault.ts                      # [T8] vault 状态（createSignal + IPC）
│   │   └── styles.css                          # [T8] 基础样式 + 主题 token
│   └── capabilities/default.json               # [T1] Tauri 2 权限清单
└── docs/okf/SPEC.v0.1.md                       # 已存在
```

**职责边界：**
- `index/sqlite.rs`：SQLite 元数据（concepts/edges 表）+ sqlite-vec 向量虚拟表
- `index/tantivy.rs`：Tantivy 全文索引（jieba tokenizer），delete-by-term + add 更新语义
- `indexer/`：协调三层，监听 concept 变更，事务化更新
- `search/`：跨 SQLite + Tantivy + sqlite-vec 的混合检索与 RRF 融合
- `lmnotes-tauri/commands.rs`：前端唯一入口，DTO 转换
- 前端 `editor/`、`capture/`：纯视图，所有数据经 IPC

---

## Task 1: Tauri 2 应用骨架 + SolidJS 前端

**Files:**
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/src/main.tsx`（空壳，T8 填充）
- Create: `apps/desktop/src/styles.css`
- Create: `apps/desktop/tauri.conf.json`
- Create: `apps/desktop/capabilities/default.json`
- Modify: `Cargo.toml`（workspace members + tauri deps）

**目标：** `npm install && npm run tauri dev` 能启动一个显示"LMNotes"的桌面窗口。Tauri 2 用 `npm create tauri-app` 的结构但手工搭建以保证可控。

- [ ] **Step 1: workspace 根 Cargo.toml 加 tauri 依赖与 lmnotes-tauri 成员**

修改 `Cargo.toml` 的 `[workspace]`：
```toml
[workspace]
members = ["crates/lmnotes-core", "crates/lmnotes-cli", "crates/lmnotes-tauri"]
resolver = "2"
```

加 `[workspace.dependencies]`（追加到现有段）：
```toml
tauri = { version = "2", features = [] }
rusqlite = { version = "0.40", features = ["bundled"] }
sqlite-vec = "0.1"
tantivy = "0.22"
jieba-rs = "0.7"
notify = "6"
serde_json = "1"
```

- [ ] **Step 2: 创建 `crates/lmnotes-tauri/Cargo.toml`**

```toml
[package]
name = "lmnotes-tauri"
version.workspace = true
edition.workspace = true
license.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
lmnotes-core = { path = "../lmnotes-core" }
tauri = { workspace = true, features = ["protocol-asset"] }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync"] }
```

- [ ] **Step 3: `crates/lmnotes-tauri/build.rs`**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 4: `crates/lmnotes-tauri/src/lib.rs`（最小可启动）**

```rust
//! LMNotes Tauri 2 IPC 壳。命令注册在 commands.rs，T2 起填充。

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: `crates/lmnotes-tauri/src/commands.rs`（占位命令）**

```rust
//! Tauri 命令定义。M1a 逐步填充。

#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}
```

- [ ] **Step 6: `crates/lmnotes-tauri/src/main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    lmnotes_tauri::run();
}
```

- [ ] **Step 7: `apps/desktop/package.json`**

```json
{
  "name": "lmnotes-desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "solid-js": "^1.9"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "typescript": "^5.6",
    "vite": "^5",
    "vite-plugin-solid": "^2.11"
  }
}
```

- [ ] **Step 8: `apps/desktop/vite.config.ts`**

```ts
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "esnext" },
});
```

- [ ] **Step 9: `apps/desktop/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "preserve",
    "jsxImportSource": "solid-js",
    "strict": true,
    "noEmit": true,
    "types": ["vite/client"]
  },
  "include": ["src"]
}
```

- [ ] **Step 10: `apps/desktop/index.html`**

```html
<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>LMNotes</title>
  </head>
  <body>
    <div id="root"></div>
    <script src="/src/main.tsx" type="module"></script>
  </body>
</html>
```

- [ ] **Step 11: `apps/desktop/src/main.tsx`（空壳）**

```tsx
import { render } from "solid-js/web";
import "./styles.css";

function App() {
  return <h1>LMNotes</h1>;
}

render(() => <App />, document.getElementById("root")!);
```

- [ ] **Step 12: `apps/desktop/src/styles.css`（主题 token 基础）**

```css
:root {
  --bg: #1e1e2e;
  --fg: #cdd6f4;
  --accent: #89b4fa;
  --border: #45475a;
}
@media (prefers-color-scheme: light) {
  :root { --bg: #eff1f5; --fg: #4c4f69; --accent: #1e66f5; --border: #bcc0cc; }
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg); font-family: system-ui, sans-serif; }
h1 { padding: 1rem; }
```

- [ ] **Step 13: `apps/desktop/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "LMNotes",
  "version": "0.1.0",
  "identifier": "com.lmnotes.desktop",
  "build": {
    "frontendDist": "../..",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [{ "title": "LMNotes", "width": 1280, "height": 800 }],
    "security": { "csp": null }
  },
  "bundle": { "active": true, "targets": "all" }
}
```

> **注意：** Tauri 2 的 `frontendDist` 路径相对于 `src-tauri` 位置。此配置假设 `tauri.conf.json` 放在 `apps/desktop/`，但 Tauri 默认在 `src-tauri/` 找它。**修正：把 tauri.conf.json 放到 `apps/desktop/src-tauri/tauri.conf.json`**，并将 `main.rs`/`Cargo.toml`(lmnotes-tauri) 也移到 `src-tauri/`。重新组织 Step 13–14：

**Step 13（修正）: 目录结构**——Tauri 2 约定：前端工程根放 `package.json`，`src-tauri/` 子目录放 Rust。所以：
- `apps/desktop/package.json`、`vite.config.ts`、`index.html`、`src/main.tsx` —— 前端根
- `apps/desktop/src-tauri/Cargo.toml` —— **Tauri Rust crate（即原 lmnotes-tauri）**
- `apps/desktop/src-tauri/tauri.conf.json`
- `apps/desktop/src-tauri/main.rs`
- `apps/desktop/src-tauri/build.rs`

把 Step 2–6 创建的 `crates/lmnotes-tauri/` 内容移到 `apps/desktop/src-tauri/`，并从根 workspace members 移除 `crates/lmnotes-tauri`，加入 `apps/desktop/src-tauri`。

更新根 `Cargo.toml`：
```toml
[workspace]
members = ["crates/lmnotes-core", "crates/lmnotes-cli", "apps/desktop/src-tauri"]
resolver = "2"
```

更新 `apps/desktop/src-tauri/Cargo.toml` 的包名为 `lmnotes-desktop`（避免与 cli 混淆）。

- [ ] **Step 14: `apps/desktop/src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "LMNotes",
  "version": "0.1.0",
  "identifier": "com.lmnotes.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [{ "title": "LMNotes", "width": 1280, "height": 800 }],
    "security": { "csp": null }
  },
  "bundle": { "active": true, "targets": "all" }
}
```

- [ ] **Step 15: `apps/desktop/src-tauri/capabilities/default.json`（Tauri 2 权限）**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "默认权限：核心事件 + 对话框",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:default"]
}
```

- [ ] **Step 16: 验证 Rust 侧编译**

Run: `cargo check --workspace`
Expected: 编译通过（lmnotes-desktop crate 加入，ping 命令注册）。

- [ ] **Step 17: 验证前端构建**

```bash
cd apps/desktop
npm install
npm run build
```
Expected: `dist/` 生成，无 TS 错误。

- [ ] **Step 18: 手动启动验证（桌面窗口出现）**

```bash
cd apps/desktop
npm run tauri dev
```
Expected: 桌面窗口弹出显示 "LMNotes"，开发服务器在 1420 端口。**手动确认后关闭窗口结束 dev。**

- [ ] **Step 19: Commit**

```bash
git add apps/ crates/ Cargo.toml
git commit -m "feat(tauri): scaffold Tauri 2 + SolidJS desktop shell"
```

---

## Task 2: IndexBackend trait + 核心层搜索/索引模块骨架

**Files:**
- Modify: `crates/lmnotes-core/src/lib.rs`（加 `pub mod index; pub mod indexer; pub mod search;`）
- Modify: `crates/lmnotes-core/src/backend/mod.rs`（加 IndexBackend trait）
- Create: `crates/lmnotes-core/src/index/mod.rs`
- Create: `crates/lmnotes-core/src/index/schema.rs`
- Create: `crates/lmnotes-core/src/indexer/mod.rs`
- Create: `crates/lmnotes-core/src/search/mod.rs`
- Create: `crates/lmnotes-core/src/search/rrf.rs`

**目标：** 定义 `IndexBackend` trait（ADR-0002），搭建 index/indexer/search 模块骨架，T3–T7 填充实现。

- [ ] **Step 1: 写失败测试（trait 与 schema 先行）**

创建 `crates/lmnotes-core/src/index/schema.rs`：

```rust
//! 三层索引共享的 schema 常量与数据结构。

/// SQLite concepts 表：concept 元数据。
pub const CREATE_CONCEPTS: &str = "
CREATE TABLE IF NOT EXISTS concepts (
    id          TEXT PRIMARY KEY,        -- frontmatter id（nt_...）
    path        TEXT NOT NULL UNIQUE,    -- bundle 内相对路径
    type_       TEXT NOT NULL,
    title       TEXT,
    mtime       INTEGER NOT NULL,        -- unix 秒
    content_hash TEXT NOT NULL           -- 正文 sha256（变更检测）
);
CREATE INDEX IF NOT EXISTS idx_concepts_path ON concepts(path);
";

/// SQLite edges 表：图谱邻接（增量，见 ADR-0003 F5）。
pub const CREATE_EDGES: &str = "
CREATE TABLE IF NOT EXISTS edges (
    src_id  TEXT NOT NULL,
    dst_id  TEXT,                        -- 可空：链接目标暂不存在（OKF §5.3 容忍断链）
    dst_path TEXT NOT NULL,              -- 链接原始路径
    link_text TEXT,
    PRIMARY KEY (src_id, dst_path)
);
CREATE INDEX IF NOT EXISTS idx_edges_src ON edges(src_id);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst_id);
";

/// sqlite-vec 向量虚拟表（M1b 接 embed 后填充）。
pub const CREATE_VEC: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS vec_concepts USING vec0(
    id TEXT PRIMARY KEY,
    embedding float[768]
);
";

#[derive(Debug, Clone)]
pub struct ConceptRow {
    pub id: String,
    pub path: String,
    pub type_: String,
    pub title: Option<String>,
    pub mtime: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct EdgeRow {
    pub src_id: String,
    pub dst_id: Option<String>,
    pub dst_path: String,
    pub link_text: Option<String>,
}
```

- [ ] **Step 2: IndexBackend trait（追加到 backend/mod.rs）**

在 `crates/lmnotes-core/src/backend/mod.rs` 末尾追加：

```rust
use crate::index::schema::{ConceptRow, EdgeRow};

/// 索引后端抽象（ADR-0002）。SQLite 元数据层 + 向量层。
/// Tantivy 全文层由独立类型实现（不在此 trait，因其 API 差异大）。
#[async_trait]
pub trait IndexBackend: Send + Sync {
    /// 初始化 schema（幂等）。
    async fn init_schema(&self) -> Result<()>;

    /// UPSERT concept 元数据。
    async fn upsert_concept(&self, row: ConceptRow) -> Result<()>;

    /// 删除 concept（含其出边）。
    async fn delete_concept(&self, id: &str) -> Result<()>;

    /// 替换 concept 的出边（先删后插，增量，见 ADR-0003 F5）。
    async fn replace_edges(&self, src_id: &str, edges: Vec<EdgeRow>) -> Result<()>;

    /// 按 id 查 concept。
    async fn get_concept(&self, id: &str) -> Result<Option<ConceptRow>>;

    /// 按 path 查 concept（改名检测用）。
    async fn get_concept_by_path(&self, path: &str) -> Result<Option<ConceptRow>>;

    /// 反向链接查询：谁链接到了 dst_id。
    async fn backrefs(&self, dst_id: &str) -> Result<Vec<EdgeRow>>;
}
```

更新 `crates/lmnotes-core/src/lib.rs`：
```rust
pub mod backend;
pub mod error;
pub mod id;
pub mod index;
pub mod indexer;
pub mod okf;
pub mod search;
pub mod vault;

pub use error::{CoreError, Result};
```

创建占位模块：
- `crates/lmnotes-core/src/index/mod.rs`: `pub mod schema;`
- `crates/lmnotes-core/src/indexer/mod.rs`: `// T6 实现`
- `crates/lmnotes-core/src/search/mod.rs`: `pub mod rrf;`
- `crates/lmnotes-core/src/search/rrf.rs`: `// T7 实现`

- [ ] **Step 3: 写失败测试（trait 契约 + RRF）**

创建 `crates/lmnotes-core/src/search/rrf.rs`：

```rust
//! Reciprocal Rank Fusion：融合多路检索结果（ADR-0003）。

/// 融合两路排名（rank 从 1 开始）。RRF 公式：score = Σ 1/(k + rank_i)。
pub fn fuse_scores(rank_a: usize, rank_b: usize, k: usize) -> f64 {
    1.0 / (k as f64 + rank_a as f64) + 1.0 / (k as f64 + rank_b as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_in_both_highest() {
        let both_top = fuse_scores(1, 1, 60);
        let only_a_top = fuse_scores(1, 100, 60);
        assert!(both_top > only_a_top, "出现在两路 top1 应高于仅一路 top1");
    }

    #[test]
    fn k_60_is_standard() {
        // 标准 RRF k=60；验证常数合理（结果在 0~0.04 区间）
        let s = fuse_scores(1, 2, 60);
        assert!(s > 0.0 && s < 0.05);
    }

    #[test]
    fn higher_rank_lower_score() {
        assert!(fuse_scores(1, 1, 60) > fuse_scores(5, 5, 60));
    }
}
```

- [ ] **Step 4: 验证 rrf 测试通过**

Run: `cargo test -p lmnotes-core search::rrf`
Expected: 3 个测试 PASS（这是纯函数，先实现以验证算法正确）。

- [ ] **Step 5: Commit**

```bash
git add crates/lmnotes-core/src/
git commit -m "feat(core): IndexBackend trait + index schema + RRF fusion"
```

---

## Task 3: SqliteIndex 实现（元数据 + edges）

**Files:**
- Modify: `crates/lmnotes-core/Cargo.toml`（加 rusqlite/sqlite-vec 依赖）
- Create: `crates/lmnotes-core/src/index/sqlite.rs`
- Modify: `crates/lmnotes-core/src/index/mod.rs`（`pub mod sqlite;`）

**目标：** `SqliteIndex` 实现 `IndexBackend`，含 concepts/edges 表 + sqlite-vec 虚拟表初始化。向量写入留 M1b（trait 只要求 init_schema 建 vec 表）。

- [ ] **Step 1: 加依赖**

修改 `crates/lmnotes-core/Cargo.toml` `[dependencies]`：
```toml
rusqlite = { workspace = true }
sqlite-vec = { workspace = true }
```

> `rusqlite` 的 `bundled` feature 已在 workspace 声明（T1 Step 1）。

- [ ] **Step 2: 写失败测试**

创建 `crates/lmnotes-core/src/index/sqlite.rs`（先测试 + 实现，因 rusqlite 是同步 API，测试用 tokio 的 block_on 简化）：

```rust
//! SQLite 元数据索引 + sqlite-vec 向量表实现。

use super::schema::{CREATE_CONCEPTS, CREATE_EDGES, CREATE_VEC, ConceptRow, EdgeRow};
use crate::backend::IndexBackend;
use crate::Result;
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct SqliteIndex {
    conn: Mutex<Connection>,
}

impl SqliteIndex {
    /// 打开/创建索引文件。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let conn = Connection::open(path.into())?;
        // 加载 sqlite-vec 扩展
        sqlite_vec::load(&conn)
            .map_err(|e| crate::CoreError::Conformance(format!("sqlite-vec load: {e}")))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 内存库（测试用）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        sqlite_vec::load(&conn)
            .map_err(|e| crate::CoreError::Conformance(format!("sqlite-vec load: {e}")))?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

#[async_trait]
impl IndexBackend for SqliteIndex {
    async fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(&format!("{CREATE_CONCEPTS}\n{CREATE_EDGES}\n{CREATE_VEC}"))?;
        Ok(())
    }

    async fn upsert_concept(&self, row: ConceptRow) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO concepts (id, path, type_, title, mtime, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                row.id, row.path, row.type_, row.title, row.mtime, row.content_hash
            ],
        )?;
        Ok(())
    }

    async fn delete_concept(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM concepts WHERE id = ?1", [id])?;
        conn.execute("DELETE FROM edges WHERE src_id = ?1", [id])?;
        Ok(())
    }

    async fn replace_edges(&self, src_id: &str, edges: Vec<EdgeRow>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM edges WHERE src_id = ?1", [src_id])?;
        let mut stmt = conn.prepare(
            "INSERT INTO edges (src_id, dst_id, dst_path, link_text) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for e in &edges {
            stmt.execute(rusqlite::params![e.src_id, e.dst_id, e.dst_path, e.link_text])?;
        }
        Ok(())
    }

    async fn get_concept(&self, id: &str) -> Result<Option<ConceptRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, type_, title, mtime, content_hash FROM concepts WHERE id = ?1",
        )?;
        let row = stmt.query_row([id], |r| {
            Ok(ConceptRow {
                id: r.get(0)?,
                path: r.get(1)?,
                type_: r.get(2)?,
                title: r.get(3)?,
                mtime: r.get(4)?,
                content_hash: r.get(5)?,
            })
        });
        match row {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_concept_by_path(&self, path: &str) -> Result<Option<ConceptRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, type_, title, mtime, content_hash FROM concepts WHERE path = ?1",
        )?;
        let row = stmt.query_row([path], |r| {
            Ok(ConceptRow {
                id: r.get(0)?,
                path: r.get(1)?,
                type_: r.get(2)?,
                title: r.get(3)?,
                mtime: r.get(4)?,
                content_hash: r.get(5)?,
            })
        });
        match row {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn backrefs(&self, dst_id: &str) -> Result<Vec<EdgeRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT src_id, dst_id, dst_path, link_text FROM edges WHERE dst_id = ?1",
        )?;
        let rows = stmt.query_map([dst_id], |r| {
            Ok(EdgeRow {
                src_id: r.get(0)?,
                dst_id: r.get(1)?,
                dst_path: r.get(2)?,
                link_text: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, path: &str) -> ConceptRow {
        ConceptRow {
            id: id.into(),
            path: path.into(),
            type_: "note".into(),
            title: Some("T".into()),
            mtime: 1000,
            content_hash: "abc".into(),
        }
    }

    #[tokio::test]
    async fn init_then_upsert_get() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "notes/a.md")).await.unwrap();
        let got = idx.get_concept("nt_1").await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().path, "notes/a.md");
    }

    #[tokio::test]
    async fn upsert_replaces() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "notes/a.md")).await.unwrap();
        let mut r = row("nt_1", "notes/a.md");
        r.title = Some("Updated".into());
        idx.upsert_concept(r).await.unwrap();
        assert_eq!(idx.get_concept("nt_1").await.unwrap().unwrap().title, Some("Updated".into()));
    }

    #[tokio::test]
    async fn delete_cascades_edges() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        idx.upsert_concept(row("nt_2", "b.md")).await.unwrap();
        idx.replace_edges(
            "nt_1",
            vec![EdgeRow {
                src_id: "nt_1".into(),
                dst_id: Some("nt_2".into()),
                dst_path: "/b.md".into(),
                link_text: Some("b".into()),
            }],
        )
        .await
        .unwrap();
        assert_eq!(idx.backrefs("nt_2").await.unwrap().len(), 1);
        idx.delete_concept("nt_1").await.unwrap();
        assert!(idx.backrefs("nt_2").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn replace_edges_is_incremental() {
        // ADR-0003 F5：replace_edges 先删后插，仅影响 src_id 的出边
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        idx.upsert_concept(row("nt_2", "b.md")).await.unwrap();
        idx.upsert_concept(row("nt_3", "c.md")).await.unwrap();
        idx.replace_edges(
            "nt_1",
            vec![EdgeRow {
                src_id: "nt_1".into(),
                dst_id: Some("nt_2".into()),
                dst_path: "/b.md".into(),
                link_text: None,
            }],
        )
        .await
        .unwrap();
        idx.replace_edges(
            "nt_3",
            vec![EdgeRow {
                src_id: "nt_3".into(),
                dst_id: Some("nt_2".into()),
                dst_path: "/b.md".into(),
                link_text: None,
            }],
        )
        .await
        .unwrap();
        // 替换 nt_1 出边，不应影响 nt_3 的出边
        idx.replace_edges("nt_1", vec![]).await.unwrap();
        assert_eq!(idx.backrefs("nt_2").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_by_path_works() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "notes/a.md")).await.unwrap();
        let got = idx.get_concept_by_path("notes/a.md").await.unwrap();
        assert_eq!(got.unwrap().id, "nt_1");
    }
}
```

更新 `crates/lmnotes-core/src/index/mod.rs`：
```rust
pub mod schema;
pub mod sqlite;

pub use sqlite::SqliteIndex;
```

- [ ] **Step 3: 跑测试**

Run: `cargo test -p lmnotes-core index::sqlite`
Expected: 5 个测试 PASS。若 sqlite-vec 加载失败（平台问题），T3 阻塞，需排查 sqlite-vec 在该平台的构建（其是纯 Rust FFI 绑定，bundled rusqlite 已含 SQLite）。

- [ ] **Step 4: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(index): SqliteIndex implementing IndexBackend (concepts + edges + vec table)"
```

---

## Task 4: TantivyIndex 实现（全文 + jieba 分词）

**Files:**
- Modify: `crates/lmnotes-core/Cargo.toml`（加 tantivy/jieba-rs）
- Create: `crates/lmnotes-core/src/index/tantivy.rs`
- Modify: `crates/lmnotes-core/src/index/mod.rs`

**目标：** `TantivyIndex` 全文检索，中文用 jieba-rs tokenizer。更新语义 = `delete_by_term(id) + add`（ADR-0003 F4）。

- [ ] **Step 1: 加依赖**

`crates/lmnotes-core/Cargo.toml` `[dependencies]`：
```toml
tantivy = { workspace = true }
jieba-rs = { workspace = true }
```

- [ ] **Step 2: 写实现 + 测试**

创建 `crates/lmnotes-core/src/index/tantivy.rs`：

```rust
//! Tantivy 全文索引，中文用 jieba-rs tokenizer。
//! 更新语义（ADR-0003 F4）：delete_by_term(id) + add。

use crate::Result;
use std::path::PathBuf;
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, STORED, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, Term};

/// 单条检索命中。
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
}

pub struct TantivyIndex {
    index: Index,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    id_field: tantivy::schema::Field,
    text_field: tantivy::schema::Field,
    title_field: tantivy::schema::Field,
}

impl TantivyIndex {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        std::fs::create_dir_all(&path)
            .map_err(crate::CoreError::Io)?;
        let schema = Self::build_schema();
        let text_field = schema.get_field("text").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let title_field = schema.get_field("title").unwrap();

        let index = Index::open_or_create_in_dir(&path, schema)?;
        // 注册 jieba tokenizer 到 text field
        let tokenizer = jieba_rs::JiebaTokenizer {};
        index
            .tokenizers()
            .register("jieba", tokenizer);

        let writer = index.writer(15_000_000)?; // 15MB heap
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(Self { index, writer: Mutex::new(writer), reader, id_field, text_field, title_field })
    }

    /// 内存索引（测试用）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let schema = Self::build_schema();
        let text_field = schema.get_field("text").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let title_field = schema.get_field("title").unwrap();
        let index = Index::create_in_ram(schema);
        index
            .tokenizers()
            .register("jieba", jieba_rs::JiebaTokenizer {});
        let writer = index.writer(15_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(Self { index, writer: Mutex::new(writer), reader, id_field, text_field, title_field })
    }

    fn build_schema() -> Schema {
        let mut schema = Schema::builder();
        // id field 不分词，用于 delete_by_term
        schema.add_text_field("id", tantivy::schema::INDEXED);
        schema.add_text_field("title", TEXT);
        // text 用 jieba 分词 + STORED（M1c RAG 需取回 body snippet，前瞻性加 STORED 避免回改）
        let text_opts =
            tantivy::schema::TextOptions::default()
                .set_stored()
                .set_indexing_options(
                    tantivy::schema::TextFieldIndexingOptions::default()
                        .set_tokenizer("jieba")
                        .set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions),
                );
        schema.add_text_field("text", text_opts);
        // title 也用 jieba（中文标题）
        schema.finish()
    }

    /// 新增/更新文档（更新 = 先删后增，ADR-0003 F4）。
    pub fn upsert(&self, id: &str, title: &str, text: &str) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        // 删除旧文档
        writer.delete_term(Term::from_field_text(self.id_field, id));
        writer.add_document(doc!(
            self.id_field => id,
            self.title_field => title,
            self.text_field => text
        ))?;
        writer.commit()?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.delete_term(Term::from_field_text(self.id_field, id));
        writer.commit()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.text_field, self.title_field]);
        let parsed = parser
            .parse_query(query)
            .map_err(|e| crate::CoreError::Conformance(format!("query parse: {e}")))?;
        let hits = searcher.search(&parsed, &TopDocs::with_limit(limit))?;
        let out = hits
            .into_iter()
            .map(|(score, doc_addr)| {
                let doc: tantivy::TantivyDocument = searcher.doc(doc_addr).unwrap();
                let id = doc
                    .get_first(self.id_field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                SearchHit { id, score }
            })
            .collect();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_then_search_chinese() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "注意力机制", "注意力机制是 Transformer 的核心").unwrap();
        idx.upsert("nt_2", "Transformer", "Transformer 用了自注意力").unwrap();
        let hits = idx.search("注意力", 10).unwrap();
        assert!(hits.iter().any(|h| h.id == "nt_1"));
        assert!(hits.iter().any(|h| h.id == "nt_2"));
    }

    #[test]
    fn update_is_delete_then_add() {
        // ADR-0003 F4：同一 id 二次 upsert 不应产生重复
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "T", "内容一").unwrap();
        idx.upsert("nt_1", "T", "内容二 完全不同").unwrap();
        let hits = idx.search("内容", 10).unwrap();
        let count = hits.iter().filter(|h| h.id == "nt_1").count();
        assert_eq!(count, 1, "update should not duplicate");
    }

    #[test]
    fn delete_removes_doc() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "T", "唯一关键词 独角兽").unwrap();
        assert!(!idx.search("独角兽", 10).unwrap().is_empty());
        idx.delete("nt_1").unwrap();
        assert!(idx.search("独角兽", 10).unwrap().is_empty());
    }

    #[test]
    fn jieba_segments_chinese_words() {
        // jieba 把"知识图谱"切成"知识"+"图谱"，搜"知识"能命中
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "T", "知识图谱是结构化的知识库").unwrap();
        let hits = idx.search("知识", 10).unwrap();
        assert!(hits.iter().any(|h| h.id == "nt_1"));
    }
}
```

更新 `crates/lmnotes-core/src/index/mod.rs`：
```rust
pub mod schema;
pub mod sqlite;
pub mod tantivy;

pub use sqlite::SqliteIndex;
pub use tantivy::{SearchHit as TantivyHit, TantivyIndex};
```

- [ ] **Step 3: 跑测试**

Run: `cargo test -p lmnotes-core index::tantivy`
Expected: 4 个测试 PASS（含 jieba 中文分词验证）。

> 若 `jieba_rs::JiebaTokenizer` 的 trait 路径或方法签名与版本不符（jieba-rs API 偶有变动），按编译器提示调整 import。jieba-rs 0.7 的 tokenizer 实现见其文档。

- [ ] **Step 4: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(index): TantivyIndex with jieba tokenizer (delete-by-term update semantics)"
```

---

## Task 5: 增量索引器（indexer）

**Files:**
- Create: `crates/lmnotes-core/src/indexer/mod.rs`

**目标：** 协调三层索引。concept 保存时：解析 frontmatter + body，抽取出边（markdown link），更新 SQLite（concepts + edges）与 Tantivy（全文）。向量层留 M1b。增量检测：比较 content_hash 跳过未变更。

- [ ] **Step 1: 写实现 + 测试**

替换 `crates/lmnotes-core/src/indexer/mod.rs`：

```rust
//! 增量索引器：协调 SQLite 元数据 + Tantivy 全文（向量层 M1b 补）。
//! 监听 concept 变更，事务化更新三层。增量：按 content_hash 跳过未变。

use crate::backend::IndexBackend;
use crate::index::schema::{ConceptRow, EdgeRow};
use crate::index::tantivy::TantivyIndex;
use crate::okf::concept::Concept;
use crate::Result;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct Indexer {
    pub meta: Arc<dyn IndexBackend>,
    pub fulltext: Arc<TantivyIndex>,
}

impl Indexer {
    pub fn new(meta: Arc<dyn IndexBackend>, fulltext: Arc<TantivyIndex>) -> Self {
        Self { meta, fulltext }
    }

    /// 索引一个 concept（增量：hash 未变则跳过）。
    pub async fn index_concept(&self, rel_path: &str, text: &str, concept: &Concept) -> Result<bool> {
        let id = concept.frontmatter.id.clone().unwrap_or_else(|| rel_path.to_string());
        let content_hash = hex_hash(text);
        // 增量检查
        if let Some(existing) = self.meta.get_concept(&id).await? {
            if existing.content_hash == content_hash && existing.path == rel_path {
                return Ok(false); // 未变更
            }
        }
        // 抽取 body 中的 markdown link 作为出边
        let edges = extract_edges(&concept.body, rel_path);
        let row = ConceptRow {
            id: id.clone(),
            path: rel_path.to_string(),
            type_: concept.frontmatter.type_.clone(),
            title: concept.frontmatter.title.clone(),
            mtime: now_secs(),
            content_hash: content_hash.clone(),
        };
        // 更新 SQLite
        self.meta.upsert_concept(row).await?;
        // 解析出边中的 dst_id：尝试按 path 反查
        let mut resolved: Vec<EdgeRow> = Vec::with_capacity(edges.len());
        for e in edges {
            let dst_id = self.resolve_dst_id(&e.dst_path).await?;
            resolved.push(EdgeRow {
                src_id: id.clone(),
                dst_id,
                dst_path: e.dst_path,
                link_text: e.link_text,
            });
        }
        self.meta.replace_edges(&id, resolved).await?;
        // 更新 Tantivy 全文
        let title = concept.frontmatter.title.as_deref().unwrap_or("");
        self.fulltext.upsert(&id, title, &concept.body)?;
        Ok(true)
    }

    /// 删除一个 concept 的全部索引数据。
    pub async fn unindex(&self, id: &str) -> Result<()> {
        self.meta.delete_concept(id).await?;
        self.fulltext.delete(id)?;
        Ok(())
    }

    async fn resolve_dst_id(&self, dst_path: &str) -> Result<Option<String>> {
        // bundle-relative 路径去 .md 后缀对齐 concept.path
        let normalized = dst_path
            .trim_start_matches('/')
            .trim_end_matches(".md");
        Ok(self.meta.get_concept_by_path(normalized).await?.map(|r| r.id))
    }
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn hex_hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

struct RawEdge {
    dst_path: String,
    link_text: Option<String>,
}

/// 抽取 body 中的 markdown link（OKF §5）。
fn extract_edges(body: &str, _self_path: &str) -> Vec<RawEdge> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let parser = Parser::new(body);
    let mut edges = Vec::new();
    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                let dest = dest_url.into_string();
                // 仅 bundle-relative（/开头）算内部链接（OKF §5.1）
                if dest.starts_with('/') {
                    edges.push(RawEdge { dst_path: dest, link_text: None });
                }
            }
            Event::Text(t) => {
                if let Some(last) = edges.last_mut() {
                    if last.link_text.is_none() {
                        last.link_text = Some(t.into_string());
                    }
                }
            }
            Event::End(TagEnd::Link) => {}
            _ => {}
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::sqlite::SqliteIndex;

    async fn setup() -> Indexer {
        let meta = Arc::new(SqliteIndex::in_memory().unwrap());
        meta.init_schema().await.unwrap();
        let ft = Arc::new(TantivyIndex::in_memory().unwrap());
        Indexer::new(meta, ft)
    }

    #[tokio::test]
    async fn index_then_search_finds_it() {
        let idx = setup().await;
        let c = Concept::parse(
            "---\ntype: note\ntitle: 注意力\nid: nt_1\n---\n\n# 注意力\n\n这是关于注意力的内容。\n",
        )
        .unwrap();
        let changed = idx.index_concept("notes/a.md", "raw", &c).await.unwrap();
        assert!(changed);
        let hits = idx.fulltext.search("注意力", 10).unwrap();
        assert!(hits.iter().any(|h| h.id == "nt_1"));
    }

    #[tokio::test]
    async fn incremental_skips_unchanged() {
        let idx = setup().await;
        let c = Concept::parse("---\ntype: note\nid: nt_1\n---\n\nbody\n").unwrap();
        idx.index_concept("a.md", "raw", &c).await.unwrap();
        let changed = idx.index_concept("a.md", "raw", &c).await.unwrap();
        assert!(!changed, "re-index same content should be no-op");
    }

    #[tokio::test]
    async fn links_become_edges() {
        let idx = setup().await;
        // 先索引目标
        let target = Concept::parse("---\ntype: note\nid: nt_2\n---\n\n目标\n").unwrap();
        idx.index_concept("notes/b.md", "raw", &target).await.unwrap();
        // 索引含链接的源
        let src = Concept::parse(
            "---\ntype: note\nid: nt_1\n---\n\n见 [/notes/b.md](/notes/b.md)\n",
        )
        .unwrap();
        idx.index_concept("notes/a.md", "raw", &src).await.unwrap();
        let backrefs = idx.meta.backrefs("nt_2").await.unwrap();
        assert_eq!(backrefs.len(), 1);
        assert_eq!(backrefs[0].src_id, "nt_1");
    }

    #[tokio::test]
    async fn unindex_removes_everywhere() {
        let idx = setup().await;
        let c = Concept::parse("---\ntype: note\nid: nt_1\ntitle: 唯一\n---\n\n独角兽\n").unwrap();
        idx.index_concept("a.md", "raw", &c).await.unwrap();
        idx.unindex("nt_1").await.unwrap();
        assert!(idx.fulltext.search("独角兽", 10).unwrap().is_empty());
        assert!(idx.meta.get_concept("nt_1").await.unwrap().is_none());
    }
}
```

> 需加 `hex` 依赖。在 `crates/lmnotes-core/Cargo.toml` `[dependencies]` 加 `hex = "0.4"`。`sha2` 已在 M0 加过。

- [ ] **Step 2: 加 hex 依赖并跑测试**

修改 Cargo.toml 加 `hex = "0.4"`，然后：
Run: `cargo test -p lmnotes-core indexer::`
Expected: 4 个测试 PASS。

> **若 `pulldown-cmark` 0.13 的 `Tag::Link` / `TagEnd` 枚举结构与上方代码不符**（0.13 改了 link 变体为 struct），按编译器调整。0.13 的 `Tag::Link { dest_url, link_type, title, id }` —— 上面代码用 `dest_url.into_string()`，0.13 的 `dest_url` 是 `CowStr`，`into_string()` 可用。

- [ ] **Step 3: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(indexer): incremental concept indexer with edge extraction (markdown links)"
```

---

## Task 6: 混合检索命令 + IPC

**Files:**
- Create: `crates/lmnotes-core/src/search/mod.rs`（替换占位）
- Modify: `apps/desktop/src-tauri/src/commands.rs`（加 search 命令）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（注册命令 + 状态管理）

**目标：** 跨 SQLite + Tantivy 的混合检索（向量留 M1c，M1a 暂只全文 + 元数据过滤），暴露 `#[tauri::command] search`。

- [ ] **Step 1: 写 search 模块**

替换 `crates/lmnotes-core/src/search/mod.rs`：

```rust
pub mod rrf;

use crate::backend::IndexBackend;
use crate::index::tantivy::{SearchHit as TantivyHit, TantivyIndex};
use crate::Result;
use std::sync::Arc;

/// 一条混合检索命中（DTO 前身，M1c 接向量后含 source 标记）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub id: String,
    pub path: String,
    pub title: Option<String>,
    pub score: f64,
}

pub struct SearchEngine {
    pub meta: Arc<dyn IndexBackend>,
    pub fulltext: Arc<TantivyIndex>,
}

impl SearchEngine {
    pub fn new(meta: Arc<dyn IndexBackend>, fulltext: Arc<TantivyIndex>) -> Self {
        Self { meta, fulltext }
    }

    /// 全文检索 + 元数据富化（向量层 M1c 补）。
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let tantivy_hits: Vec<TantivyHit> = self.fulltext.search(query, limit)?;
        let mut out = Vec::with_capacity(tantivy_hits.len());
        for h in tantivy_hits {
            if let Some(row) = self.meta.get_concept(&h.id).wait()?? {
                out.push(SearchHit {
                    id: row.id,
                    path: row.path,
                    title: row.title,
                    score: h.score as f64,
                });
            }
        }
        Ok(out)
    }
}

/// 阻塞等待异步 future（search 是同步入口，元数据查询异步）。
trait WaitExt {
    type Item;
    fn wait(self) -> std::thread::Result<Self::Item>;
}

// 简化：search 内部用 block_on。生产可改全异步。
```

> **注意**：上方 `wait()??` 是伪代码占位——同步 `search` 调异步 `get_concept` 需 `tokio::runtime::Handle::block_on` 或把 `IndexBackend` 改同步。**修正方案：** `IndexBackend` 的查询方法（get_concept/backrefs）应为同步（rusqlite 本就同步），只写入方法（upsert/replace/delete）异步。**这是一个 trait 设计修正**：

修正 `crates/lmnotes-core/src/backend/mod.rs` 的 `IndexBackend`：把 `get_concept`/`get_concept_by_path`/`backrefs` 改为**同步方法**（去掉 `async`），保留 upsert/delete/replace_edges/init_schema 为 async。相应修改 SqliteIndex 实现与测试（去掉这几个方法的 `.await`）。

**修正后的 search/mod.rs**：

```rust
pub mod rrf;

use crate::backend::IndexBackend;
use crate::index::tantivy::{SearchHit as TantivyHit, TantivyIndex};
use crate::Result;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub id: String,
    pub path: String,
    pub title: Option<String>,
    pub score: f64,
}

pub struct SearchEngine {
    pub meta: Arc<dyn IndexBackend>,
    pub fulltext: Arc<TantivyIndex>,
}

impl SearchEngine {
    pub fn new(meta: Arc<dyn IndexBackend>, fulltext: Arc<TantivyIndex>) -> Self {
        Self { meta, fulltext }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let hits: Vec<TantivyHit> = self.fulltext.search(query, limit)?;
        let mut out = Vec::with_capacity(hits.len());
        for h in hits {
            if let Some(row) = self.meta.get_concept(&h.id)? {
                out.push(SearchHit {
                    id: row.id,
                    path: row.path,
                    title: row.title,
                    score: h.score as f64,
                });
            }
        }
        Ok(out)
    }
}
```

- [ ] **Step 2: 应用 trait 修正（同步查询方法）**

修改 `IndexBackend` trait：`get_concept`/`get_concept_by_path`/`backrefs` 去 `async`。同步修改 SqliteIndex 实现与 T5 indexer 中对这几个方法的调用（去 `.await`）。

Run: `cargo test -p lmnotes-core`
Expected: 全部测试仍 PASS。

- [ ] **Step 3: search 测试**

在 `search/mod.rs` 末尾加：
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::sqlite::SqliteIndex;
    use crate::indexer::Indexer;
    use crate::okf::concept::Concept;

    #[tokio::test]
    async fn search_returns_enriched_hits() {
        let meta = Arc::new(SqliteIndex::in_memory().unwrap());
        meta.init_schema().await.unwrap();
        let ft = Arc::new(TantivyIndex::in_memory().unwrap());
        let indexer = Indexer::new(meta.clone(), ft.clone());
        let c = Concept::parse("---\ntype: note\nid: nt_1\ntitle: 知识图谱\n---\n\n知识图谱连接概念\n").unwrap();
        indexer.index_concept("notes/kg.md", "raw", &c).await.unwrap();
        let engine = SearchEngine::new(meta, ft);
        let hits = engine.search("知识", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "notes/kg.md");
        assert_eq!(hits[0].title.as_deref(), Some("知识图谱"));
    }
}
```

Run: `cargo test -p lmnotes-core search::`
Expected: PASS。

- [ ] **Step 4: Tauri search 命令**

修改 `apps/desktop/src-tauri/src/commands.rs`：
```rust
use lmnotes_core::search::{SearchEngine, SearchHit};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
pub async fn search(
    query: String,
    limit: Option<usize>,
    engine: State<'_, Arc<SearchEngine>>,
) -> Result<Vec<SearchHit>, String> {
    engine.search(&query, limit.unwrap_or(20)).map_err(|e| e.to_string())
}
```

修改 `apps/desktop/src-tauri/src/lib.rs`：注册 search 命令 + 启动时构建索引（M1a 暂用固定路径 `~/.lmnotes/default`，UI 选择器 M1b）：

```rust
mod commands;

use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::search::SearchEngine;
use std::path::PathBuf;
use std::sync::Arc;

fn vault_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".lmnotes").join("default")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let dir = vault_dir();
    let meta = Arc::new(SqliteIndex::open(dir.join(".lmnotes/index.sqlite")).expect("open sqlite"));
    // 异步 init 在 spawn 里做
    let meta_init = meta.clone();
    tauri::async_runtime::spawn(async move {
        let _ = meta_init.init_schema().await;
    });
    let fulltext = Arc::new(
        TantivyIndex::open(dir.join(".lmnotes/tantivy")).expect("open tantivy"),
    );
    let engine = Arc::new(SearchEngine::new(meta, fulltext));

    tauri::Builder::default()
        .manage(engine)
        .invoke_handler(tauri::generate_handler![commands::ping, commands::search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

> 需在 `apps/desktop/src-tauri/Cargo.toml` 加 `dirs = "5"`。

- [ ] **Step 5: 验证编译**

Run: `cargo check --workspace`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
git add crates/lmnotes-core/ apps/desktop/src-tauri/
git commit -m "feat(search): hybrid search engine + tauri search command"
```

---

## Task 7: SolidJS 三栏布局 + Vault 状态

**Files:**
- Create: `apps/desktop/src/App.tsx`
- Create: `apps/desktop/src/store/vault.ts`
- Modify: `apps/desktop/src/main.tsx`

**目标：** 三栏布局骨架（左导航 / 中编辑器 / 右反链面板占位），vault 状态管理（createSignal + IPC 调用）。

- [ ] **Step 1: `apps/desktop/src/store/vault.ts`**

```ts
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface SearchHit {
  id: string;
  path: string;
  title: string | null;
  score: number;
}

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchHit[]>([]);
const [searching, setSearching] = createSignal(false);

export function useSearch() {
  return { query, setQuery, results, searching };
}

export async function runSearch(q: string) {
  setSearching(true);
  try {
    const r = await invoke<SearchHit[]>("search", { query: q, limit: 50 });
    setResults(r);
  } finally {
    setSearching(false);
  }
}
```

- [ ] **Step 2: `apps/desktop/src/App.tsx`**

```tsx
import { createSignal, For, Show } from "solid-js";
import { runSearch, useSearch } from "./store/vault";
import { Editor } from "./editor/Editor";

export function App() {
  const { query, setQuery, results, searching } = useSearch();
  const [activePath, setActivePath] = createSignal<string | null>(null);

  return (
    <div class="layout">
      <aside class="sidebar">
        <input
          placeholder="搜索…"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && runSearch(query())}
        />
        <ul>
          <For each={results()}>
            {(r) => (
              <li>
                <button onClick={() => setActivePath(r.path)}>{r.title || r.path}</button>
              </li>
            )}
          </For>
        </ul>
        <Show when={searching()}><span>搜索中…</span></Show>
      </aside>
      <main class="content">
        <Show when={activePath()} fallback={<p>选择左侧笔记或搜索</p>}>
          <Editor path={activePath()!} />
        </Show>
      </main>
      <aside class="backrefs">
        <h3>反向链接</h3>
        <p class="muted">（M1b 接入）</p>
      </aside>
    </div>
  );
}
```

- [ ] **Step 3: 更新 styles.css 三栏样式**

追加到 `apps/desktop/src/styles.css`：
```css
.layout { display: grid; grid-template-columns: 240px 1fr 240px; height: 100vh; }
.sidebar, .backrefs { border-right: 1px solid var(--border); padding: 0.5rem; overflow: auto; }
.backrefs { border-right: none; border-left: 1px solid var(--border); }
.content { padding: 1rem; overflow: auto; }
.sidebar input { width: 100%; padding: 0.4rem; background: var(--bg); color: var(--fg); border: 1px solid var(--border); }
.sidebar ul { list-style: none; padding: 0; }
.sidebar button { background: none; border: none; color: var(--fg); cursor: pointer; text-align: left; padding: 0.3rem; width: 100%; }
.sidebar button:hover { background: var(--border); }
.muted { color: var(--border); font-size: 0.85rem; }
```

- [ ] **Step 4: 更新 main.tsx**

```tsx
import { render } from "solid-js/web";
import { App } from "./App";
import "./styles.css";

render(() => <App />, document.getElementById("root")!);
```

- [ ] **Step 5: 验证构建**

```bash
cd apps/desktop && npm run build
```
Expected: dist 生成，无 TS 错误。

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/
git commit -m "feat(ui): three-pane layout + search sidebar"
```

---

## Task 8: CodeMirror 6 编辑器

**Files:**
- Modify: `apps/desktop/package.json`（加 @codemirror/* 依赖）
- Create: `apps/desktop/src/editor/Editor.tsx`
- Create: `apps/desktop/src/editor/markdown.ts`
- Create: `apps/desktop/src/editor/solid-cm.ts`（Solid 封装）

**目标：** CodeMirror 6 + markdown 语言，加载/保存 OKF concept 文件。双向链接补全推 M1b（需 IPC resolve_path）。

- [ ] **Step 1: 加依赖**

```bash
cd apps/desktop
npm install @codemirror/state @codemirror/view @codemirror/lang-markdown @codemirror/commands @codemirror/language
```

- [ ] **Step 2: `apps/desktop/src/editor/solid-cm.ts`（Solid 封装）**

```ts
import { onMount, onCleanup } from "solid-js";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { defaultKeymap } from "@codemirror/commands";

export function useCodeMirror(
  container: () => HTMLElement | undefined,
  initial: string,
  onChange: (doc: string) => void,
) {
  let view: EditorView | undefined;
  onMount(() => {
    const el = container();
    if (!el) return;
    view = new EditorView({
      state: EditorState.create({
        doc: initial,
        extensions: [
          keymap.of(defaultKeymap),
          EditorView.lineWrapping,
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChange(u.state.doc.toString());
          }),
        ],
      }),
      parent: el,
    });
  });
  onCleanup(() => view?.destroy());
  return () => view;
}
```

- [ ] **Step 3: `apps/desktop/src/editor/markdown.ts`**

```ts
import { markdown } from "@codemirror/lang-markdown";

export const markdownExtension = () => markdown({ defaultCodeLanguage: false });
```

- [ ] **Step 4: `apps/desktop/src/editor/Editor.tsx`**

```tsx
import { createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { useCodeMirror } from "./solid-cm";
import { markdownExtension } from "./markdown";

interface ConceptFile {
  text: string;
}

export function Editor(props: { path: string }) {
  let host: HTMLDivElement | undefined;
  const [content, setContent] = createSignal("");
  const [dirty, setDirty] = createSignal(false);

  onMount(async () => {
    const file = await invoke<ConceptFile>("read_concept", { path: props.path });
    setContent(file.text);
    // CodeMirror 初始化在 content 就绪后
  });

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const onChange = (doc: string) => {
    setContent(doc);
    setDirty(true);
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      invoke("save_concept", { path: props.path, text: doc })
        .then(() => setDirty(false))
        .catch((e) => console.error("save failed", e));
    }, 800); // 防抖保存
  };

  useCodeMirror(() => host, content(), onChange);

  return (
    <div class="editor-wrap">
      <div class="editor-toolbar">
        <span>{props.path}</span>
        {dirty() && <span class="dirty">●</span>}
      </div>
      <div class="cm-host" ref={host} />
    </div>
  );
}
```

> 需 T9 提供 `read_concept`/`save_concept` Tauri 命令（见 Task 9）。先在本任务把命令加到 commands.rs。

- [ ] **Step 5: 加 read_concept/save_concept 命令**

`apps/desktop/src-tauri/src/commands.rs` 追加：
```rust
use lmnotes_core::okf::concept::Concept;
use std::path::PathBuf;

fn vault_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lmnotes/default")
}

#[tauri::command]
pub async fn read_concept(path: String) -> Result<ConceptDto, String> {
    let full = vault_root().join(&path);
    let text = tokio::fs::read_to_string(&full).await.map_err(|e| e.to_string())?;
    Ok(ConceptDto { text })
}

#[tauri::command]
pub async fn save_concept(path: String, text: String) -> Result<(), String> {
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &text).await.map_err(|e| e.to_string())?;
    // 触发增量索引（T10 索引器接保存事件）
    // M1a: 此处先直接调 indexer，T10 改为事件订阅
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ConceptDto {
    pub text: String,
}
```

并在 `lib.rs` 的 `generate_handler!` 注册 `commands::read_concept, commands::save_concept`。

- [ ] **Step 6: 加 deps**

`apps/desktop/src-tauri/Cargo.toml` 加 `dirs = "5"`（lib.rs 已用到）。并在 commands.rs 顶部 `use` 调整。

- [ ] **Step 7: 验证构建**

```bash
cargo check --workspace
cd apps/desktop && npm run build
```
Expected: 两边都通过。

- [ ] **Step 8: Commit**

```bash
git add apps/desktop/
git commit -m "feat(editor): CodeMirror 6 markdown editor with debounced save"
```

---

## Task 9: 快速捕获浮窗

**Files:**
- Create: `apps/desktop/src/capture/Capture.tsx`
- Modify: `apps/desktop/src/App.tsx`（全局快捷键唤起）
- Modify: `apps/desktop/src-tauri/src/commands.rs`（加 capture 命令）

**目标：** 全局快捷键（桌面端 Ctrl/Cmd+N）唤起浮窗，输入文本即写入当日 daily note。语音推 M2。

- [ ] **Step 1: `apps/desktop/src-tauri/src/commands.rs` 加 capture 命令**

```rust
use chrono::Utc;

#[tauri::command]
pub async fn quick_capture(text: String) -> Result<String, String> {
    let root = vault_root();
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let daily_path = format!("notes/daily/{date}.md");
    let full = root.join(&daily_path);
    // 若不存在，创建带 frontmatter 的 daily note
    if !full.exists() {
        let id = lmnotes_core::id::new_note_id(Utc::now().naive_utc());
        let header = format!(
            "---\ntype: daily\nid: {id}\ntitle: {date}\n---\n\n# {date}\n\n",
            id = id, date = date
        );
        tokio::fs::create_dir_all(full.parent().unwrap()).await.map_err(|e| e.to_string())?;
        tokio::fs::write(&full, header).await.map_err(|e| e.to_string())?;
    }
    // 追加捕获内容（带时间戳）
    let time = Utc::now().format("%H:%M").to_string();
    let entry = format!("\n## {time}\n\n{text}\n");
    let mut existing = tokio::fs::read_to_string(&full).await.map_err(|e| e.to_string())?;
    existing.push_str(&entry);
    tokio::fs::write(&full, existing).await.map_err(|e| e.to_string())?;
    Ok(daily_path)
}
```

注册到 `generate_handler!`。

- [ ] **Step 2: `apps/desktop/src/capture/Capture.tsx`**

```tsx
import { createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export function Capture(props: { onClose: () => void }) {
  const [text, setText] = createSignal("");
  const [saving, setSaving] = createSignal(false);

  const submit = async () => {
    if (!text().trim()) return props.onClose();
    setSaving(true);
    try {
      await invoke("quick_capture", { text: text() });
      props.onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="capture-overlay" onClick={props.onClose}>
      <div class="capture-box" onClick={(e) => e.stopPropagation()}>
        <textarea
          autofocus
          placeholder="快速记一条…（Esc 关闭，Ctrl+Enter 保存）"
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onClose();
            if (e.key === "Enter" && e.ctrlKey) submit();
          }}
        />
        <Show when={saving()}><span>保存中…</span></Show>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: App.tsx 集成浮窗 + 全局快捷键**

修改 `App.tsx`：加 `captureOpen` signal，键盘监听 `Ctrl/Cmd+N`，渲染 `<Capture>`。追加：

```tsx
const [captureOpen, setCaptureOpen] = createSignal(false);

window.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
    e.preventDefault();
    setCaptureOpen(true);
  }
});
```

并在 JSX 末尾：`<Show when={captureOpen()}><Capture onClose={() => setCaptureOpen(false)} /></Show>`

- [ ] **Step 4: 样式**

追加到 styles.css：
```css
.capture-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: flex-start; justify-content: center; padding-top: 15vh; z-index: 100; }
.capture-box { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 1rem; width: 500px; }
.capture-box textarea { width: 100%; min-height: 100px; background: var(--bg); color: var(--fg); border: none; resize: vertical; font-size: 1rem; }
.editor-wrap { display: flex; flex-direction: column; height: 100%; }
.cm-host { flex: 1; overflow: auto; }
.editor-toolbar { padding: 0.3rem; border-bottom: 1px solid var(--border); font-size: 0.85rem; }
.dirty { color: var(--accent); margin-left: 0.5rem; }
```

- [ ] **Step 5: 端到端验证**

```bash
cd apps/desktop && npm run tauri dev
```
手动：Ctrl+N → 输入文本 → Ctrl+Enter → 检查 `~/.lmnotes/default/notes/daily/<今日>.md` 存在且含内容。

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/
git commit -m "feat(capture): quick capture overlay with Ctrl+N shortcut"
```

---

## Task 10: 保存即索引（事件接线）

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`（启动时全量重建 + indexer 注入 State）
- Modify: `apps/desktop/src-tauri/src/commands.rs`（save_concept 触发 indexer）

**目标：** 保存 concept 后自动增量索引；启动时若索引为空则全量重建。

- [ ] **Step 1: lib.rs 注入 indexer**

```rust
use lmnotes_core::indexer::Indexer;
// ... 在 run() 里构建 indexer 并 manage
let indexer = Arc::new(Indexer::new(meta.clone(), fulltext.clone()));
// ...
.manage(indexer.clone())
.manage(engine)
```

> 注意：`engine` 和 `indexer` 共享同一对 meta/fulltext（Arc 克隆）。

- [ ] **Step 2: save_concept 触发索引**

修改 `save_concept` 命令：
```rust
#[tauri::command]
pub async fn save_concept(
    path: String,
    text: String,
    indexer: State<'_, Arc<Indexer>>,
) -> Result<(), String> {
    let full = vault_root().join(&path);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &text).await.map_err(|e| e.to_string())?;
    // 解析并增量索引
    match Concept::parse(&text) {
        Ok(c) => {
            indexer.index_concept(&path, &text, &c).await.map_err(|e| e.to_string())?;
        }
        Err(e) => {
            // frontmatter 损坏：不阻塞保存，记录日志，索引跳过（Vault::validate 会报告）
            eprintln!("index skip (parse fail): {e}");
        }
    }
    Ok(())
}
```

- [ ] **Step 3: 启动时全量重建（若空）**

lib.rs 的 `run()` 中，spawn 一个后台任务：
```rust
let root = vault_dir();
let indexer_init = indexer.clone();
let root_init = root.clone();
tauri::async_runtime::spawn(async move {
    // 若 concepts 表为空，遍历 vault 全量索引
    if let Ok(entries) = std::fs::read_dir(&root_init) {
        let count = /* 简单：检查 index.sqlite 是否存在且非空 */;
        if !count {
            walk_and_index(&root_init, &indexer_init).await;
        }
    }
});
```

`walk_and_index` 递归遍历 `.md` 文件，逐个读、解析、`index_concept`。错误文件跳过并 log。

- [ ] **Step 4: 端到端验证（搜索闭环）**

```bash
cd apps/desktop && npm run tauri dev
```
手动流程（对应 §13 B 组前 2 环）：
1. 新建笔记（编辑器输入 markdown 保存）→ 等待 ~1s
2. 搜索栏输入笔记中的词 → 结果出现该笔记
3. 点击结果 → 编辑器加载该笔记

Expected: 全链路通。

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/
git commit -m "feat(index): save-triggered incremental indexing + startup rebuild"
```

---

## Task 11: 图片资源管理

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`（加 insert_image 命令）
- Modify: `apps/desktop/src/editor/Editor.tsx`（拖拽/粘贴图片）

**目标：** 拖拽/粘贴图片 → 按 SHA-256 哈希存 `assets/img/<前2位>/<hash>.png` → 在光标处插入 markdown 图片链接（ADR-0001 §3.5）。

- [ ] **Step 1: insert_image 命令**

`commands.rs` 追加：
```rust
use sha2::{Digest, Sha256};

#[tauri::command]
pub async fn insert_image(data: Vec<u8>, ext: String) -> Result<String, String> {
    let mut h = Sha256::new();
    h.update(&data);
    let hash = hex::encode(h.finalize());
    let prefix = &hash[..2];
    let rel = format!("assets/img/{prefix}/{hash}.{ext}");
    let full = vault_root().join(&rel);
    if !full.exists() {
        tokio::fs::create_dir_all(full.parent().unwrap()).await.map_err(|e| e.to_string())?;
        tokio::fs::write(&full, &data).await.map_err(|e| e.to_string())?;
    }
    Ok(format!("/{rel}"))
}
```

> Cargo.toml 加 `hex = "0.4"`、`sha2 = "0.10"`（workspace 已有 sha2，hex 需加）。

- [ ] **Step 2: Editor.tsx 粘贴/拖拽处理**

Editor.tsx 的 cm-host div 加事件：
```tsx
const handleFiles = async (files: FileList) => {
  for (const f of Array.from(files)) {
    if (!f.type.startsWith("image/")) continue;
    const buf = new Uint8Array(await f.arrayBuffer());
    const ext = f.name.split(".").pop() || "png";
    const rel = await invoke<string>("insert_image", { data: Array.from(buf), ext });
    // 在 CodeMirror 光标处插入 ![f.name](rel)
    // （需暴露 view 引用，T8 的 useCodeMirror 返回 view getter）
  }
};
```

```tsx
<div
  class="cm-host"
  ref={host}
  onPaste={(e) => e.clipboardData.files.length && handleFiles(e.clipboardData.files)}
  onDrop={(e) => { e.preventDefault(); handleFiles(e.dataTransfer.files); }}
  onDragOver={(e) => e.preventDefault()}
/>
```

> `useCodeMirror` 需返回 view 引用以便插入文本。调整 solid-cm.ts 返回 `() => view`，Editor 用 `view()?.dispatch({ changes: { from: pos, insert: `![](${rel})` } })`。

- [ ] **Step 3: 端到端验证**

手动：粘贴截图 → 检查 `assets/img/` 下生成哈希文件 → 编辑器出现 `![](/assets/img/...)` → 预览模式（M2 加）显示图片。

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/
git commit -m "feat(media): image paste/drop with content-hash dedup storage"
```

---

## Task 12: 文件系统监听（外部编辑感知）

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`（notify 监听 vault 目录）

**目标：** vault 目录被外部编辑（git pull / 其他编辑器）时，自动增量重索引对应文件（FR-STORE-04）。

- [ ] **Step 1: lib.rs 启动 notify watcher**

```rust
use notify::{Watcher, RecursiveMode, EventKind};
use std::sync::mpsc::channel;

let (tx, rx) = channel();
let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
    if let Ok(e) = res {
        if matches!(e.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            for p in &e.paths {
                if p.extension().map(|x| x == "md").unwrap_or(false) {
                    let _ = tx.send(p.clone());
                }
            }
        }
    }
})
.map_err(|e| eprintln!("watcher: {e}"));
if let Ok(mut w) = watcher {
    let _ = w.watch(&root, RecursiveMode::Recursive);
    let indexer = indexer.clone();
    let root = root.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(p) = rx.recv() {
            if let Ok(rel) = p.strip_prefix(&root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if let Ok(text) = std::fs::read_to_string(&p) {
                    if let Ok(c) = Concept::parse(&text) {
                        let _ = indexer.index_concept(&rel, &text, &c).await;
                    }
                }
            }
        }
    });
    // watcher 需保活：存入 tauri State 或 leak
    std::mem::forget(watcher);
}
```

> `std::mem::forget` 保活 watcher 是简化做法；更稳妥是存入 `tauri::State`。计划采用 State 方式（封装 struct HoldWatcher(Watcher)）。

- [ ] **Step 2: 端到端验证**

手动：应用运行中，用外部编辑器改一个 .md 文件保存 → 应用内搜索该笔记新内容 → 命中。

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src-tauri/
git commit -m "feat(store): notify-based file watcher for external edit awareness"
```

---

## Task 13: CI 扩展（前端构建 + tauri check）

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: 加 frontend job**

```yaml
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: "20" }
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy }
      - run: cd apps/desktop && npm ci
      - run: cd apps/desktop && npm run build
      - run: cd apps/desktop && npx tsc --noEmit
      - name: cargo check (tauri crate)
        run: cargo check -p lmnotes-desktop
```

- [ ] **Step 2: 验证本地**

```bash
cd apps/desktop && npm run build && npx tsc --noEmit && cd ../.. && cargo check -p lmnotes-desktop
```

- [ ] **Step 3: Commit**

```bash
git add .github/
git commit -m "ci: add frontend build + tauri crate check"
```

---

## M1a 退出标准（Definition of Done）

对照 PRD §13 B 组（前 2 环，LLM 部分留 M1b）：

- [ ] `cargo test --workspace` 全绿（M0 的 39 + M1a 新增 ~20 测试）
- [ ] `cd apps/desktop && npm run build` + `tsc --noEmit` 通过
- [ ] `npm run tauri dev` 启动桌面窗口
- [ ] 能创建/打开 vault（默认 `~/.lmnotes/default`）
- [ ] 编辑器（CodeMirror 6）可写 markdown 笔记，防抖保存
- [ ] **搜索闭环**：保存笔记 → 1s 内可搜索到（中文用 jieba 分词）
- [ ] 快速捕获（Ctrl+N）写入当日 daily note
- [ ] 图片粘贴/拖拽按哈希去重存储，插入链接
- [ ] 外部编辑文件能被感知并重索引
- [ ] **ADR-0003 三层结构**：SQLite（concepts+edges+vec表）+ Tantivy（jieba）+ sqlite-vec 表已建（向量填充留 M1b）
- [ ] **ADR-0003 F4**：Tantivy 更新语义 = delete_by_term + add（有测试）
- [ ] **ADR-0003 F5**：邻接表增量（replace_edges 仅影响 src_id，有测试）
- [ ] CI 多平台 Rust + 前端构建全绿

---

## Self-Review

**1. Spec coverage（PRD §12 M1 中 M1a 范围 + ADR 可执行约束）**
- FR-STORE-01/02/03/04（vault/读写/去重/监听）→ T1/T8/T11/T12 ✓
- FR-CAP-01/02/03/04（快捷键/编辑器/双链补全/拖拽）→ T9/T8/T11 ✓（双链补全推 M1b，需 resolve_path）
- FR-SEARCH-01/02/04（命令面板/混合搜索/反链）→ T7/T6 ✓（命令面板 Cmd+K 推 M1b，反链面板占位）
- ADR-0003 三层 + F4 + F5 → T3/T4/T5 + 测试 ✓
- ADR-0002 后端抽象 → IndexBackend trait（T2）✓
- ADR-0004 编辑器 → CodeMirror 6（**偏离 ADR-0004 原选 Tiptap，需更新 ADR**）⚠️

**2. Placeholder scan**
- T6 Step 1 有明确的"伪代码占位 → 修正方案"说明，最终代码完整。这是有意的 trait 设计修正记录，非遗留占位。
- T10 Step 3 的 `/* 简单检查 */` 是逻辑描述，执行时需补全为具体 SQL count 查询——**标记为执行时细化点**。
- 所有其余代码块完整。

**3. Type consistency**
- `ConceptRow`/`EdgeRow` 在 schema.rs 定义，T3/T5 跨模块一致 ✓
- `SearchHit`（search 模块）vs `TantivyHit`（tantivy 模块）——已显式区分命名（避免歧义）✓
- `IndexBackend` trait 方法签名在 T2 定义，T3 实现、T5 调用、T6 修正（查询方法改同步）一致 ✓
- DTO `ConceptDto { text }` 在 commands.rs 定义，前端 store/vault.ts 未直接用（read_concept 返回值前端 inline 定义）——一致 ✓

**4. 发现的需要修正项**
- **trait 设计修正（已内联在 T6）**：IndexBackend 查询方法应同步（rusqlite 本同步），避免同步 search 调异步 get_concept 的尴尬。这是计划编写中发现的设计改进，已在 T6 Step 1–2 显式说明。
- **Tauri 目录结构修正（已内联在 T1 Step 13）**：Tauri 2 约定 src-tauri 子目录，原计划的 crates/lmnotes-tauri 布局需调整。已在 Step 13 说明。
- **ADR-0004 同步（后续动作）**：编辑器从 Tiptap 改 CodeMirror，需更新 ADR-0004。

---

## 后续动作（计划完成后立即做）

1. **更新 ADR-0004**：编辑器选型从 Tiptap 改为 CodeMirror 6（M1a），Tiptap 推迟。新增 ADR-0006 或在 ADR-0004 加修订记录。
2. **更新 PRD §12 M1 行**：措辞对齐 M1a/M1b/M1c 拆分。

---

## Execution Handoff

计划已保存至 `docs/superpowers/plans/2026-06-22-m1a-editor-index.md`。建议执行方式：
- **Subagent-Driven**（若环境提供实现型 subagent）或 **Inline Execution**（同 M0 模式）。
- 注意：M1a 含前端 TypeScript，TDD 纪律适用于 Rust 核心（前端以端到端手动验证为主，因 SolidJS 单测投入产出比低）。
