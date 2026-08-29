# ADR-0008: 多 Vault 与重启式切换

| 状态 | Accepted |
| 日期 | 2026-08-29 |
| 关联 | PRD §5.1 FR-STORE-01（多 Vault，MVP）；ADR-0002（Tauri 壳层职责）、ADR-0003（索引随 vault 绑定） |
| 决策者 | LMNotes 维护者 |

## 背景（Context）

v0.3 及之前 vault 硬编码 `~/.lmnotes/default`（PRD FR-STORE-01 是 MVP，一直欠账）。
路径派生面已收口为两处函数（`commands::vault_root` / `lib::vault_dir`，21 处调用），
但**启动期绑定的资源远不止路径**：`SqliteIndex`/`TantivyIndex`/`Indexer`/`SearchEngine`
四个 Arc + `notify` watcher + 只读 MCP server（端口/token/发现文件）都在 `run()` 里
以具体 vault 实例化并 `manage()` 进 Tauri 状态。

实现"切换 vault"因此有两个数量级不同的选项：
热切换（进程内重建上述全部资源）vs 重启式切换（写配置后 `AppHandle::restart()`）。

## 决策（Decision）

1. **配置**：`config.json` 增 `vaults: Vec<String>`（登记清单，绝对路径）与
   `last_vault: Option<String>`（启动选择）。serde default 使老配置无痛升级；
   `resolve_vault` 纯函数在 last_vault 缺失/失效时回退默认库（有单测）。

2. **路径收口**：`vault_root()`/`vault_dir()` 委托 `llm_config::current_vault()`
   （`OnceLock` 进程内缓存）。索引、快照、导出等全部经它派生，无绝对路径残留。

3. **切换 = 重启**：`switch_vault` 校验登记与目录有效性 → 写 `last_vault` →
   `app.restart()`（`-> !`）。重启后索引/watcher/MCP 天然以新库重建，语义干净。

4. **UI**：设置页「📚 库」小节（添加=原生目录选择器；切换/移除均 confirm；
   移除只出清单不删文件）；侧栏 vault 徽标显示当前库名并直达设置。

## 后果（Consequences）

- **正面**：实现小（3 行核心逻辑 + 命令/前端）；零悬挂引用风险；MCP/索引/监听
  的重建逻辑与首启完全同一代码路径（少一条维护线）。
- **负面**：切换有 2-3s 重启等待；重启瞬间丢失未防抖完的编辑（800ms 防抖 +
  切换前 confirm 提示缓解）。
- **缓解**：切换 confirm 文案明示自动保存；未来若做热切换，本 ADR 的 Alternatives
  记录了前提条件。

## 考虑过的替代方案（Alternatives）

1. **热切换**：进程内替换 5 组 Arc + 重启 watcher + MCP server rebind（含端口/token
   与发现文件重写）。被否：Tauri `manage()` 无官方卸载语义、悬挂 `State<'_>` 引用
   风险高、MCP 已对外发布的发现信息需原子迁移——复杂度与收益不成比例。若未来
   需求强烈（如移动端多库同屏），以"独立 Workspace 句柄对象"重构为前提再评估。
2. **每库一个进程/窗口**：多窗口对应多库。被否：与"单实例 + 全局快捷键捕获"的
   现有交互模型冲突，资源翻倍。
3. **symlink 切库**（切 `default` 指向）：被否：平台差异（Windows junction 权限）、
   对用户不可见易踩坑、且 MCP 发现文件里的绝对路径会失真。
