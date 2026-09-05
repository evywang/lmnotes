# LMNotes v0.7.0

> **导航与 MVP 收口 —— 命令面板、全局速记、时间线/标签。**

v0.7 的主题是**导航与收口**：Ctrl+K 命令面板与全局快捷键浮窗补齐 PRD 最后两笔
MVP 欠账，时间线/每日笔记/标签云让"找笔记"多三个入口。至此 PRD §5 全部 MVP
级需求达成。

---

## ✨ Highlights / 核心亮点

### ⌨️ 命令面板（FR-SEARCH-01）

- **Ctrl/Cmd+K** 唤起：空查询显示全部命令 + **最近打开**（自动记录，上限 8）；
  输入即搜笔记（标题/别名/路径，复用双链补全过滤内核）+ 命令名匹配。
- 全键盘操作：`↑↓` 环绕选择、`Enter` 执行、`Esc` 关闭。

### 🌐 全局快捷键浮窗（FR-CAP-01）

- **Ctrl/Cmd+Shift+L** 在系统任何地方唤起置顶无边框速记小窗（应用运行中即可，
  可拖动、不占任务栏）；`Ctrl+Enter` 存入当日日记并自动隐藏，`Esc` 直接隐藏。
- 保存后主窗口文件树自动刷新。热键被其它程序占用时打日志降级，不影响应用内
  `Ctrl+N` 捕获。

### 🕘 时间线 / 📅 今日笔记 / 标签（FR-SEARCH-05）

- **时间线**：按最近变更倒序、按日分组（今天/昨天/日期），点击直达笔记。
- **今日笔记**：一键打开/创建当日 `notes/daily/YYYY-MM-DD.md`（幂等，绝不覆盖）。
- **标签云**：侧栏展示全库标签及计数，点击查看带该标签的笔记列表
  （concepts 索引新增 tags 列，老库自动迁移）。

### 随本版一并交付（v0.6.0 未正式发布的内容）

- 媒体任务强杀取消 + 超时预算（队列 15min / 内联 60s）；长媒体自动后台分流。
- 语音修复包：Windows 打包版语音恢复、模型下载 404 修复 + hf-mirror 镜像回退、
  弹窗内联下载免重启、多 Provider 云端 STT。

---

## 📥 Download / 下载

| Platform | File | Notes |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.7.0_x64-setup.exe` | NSIS（推荐） |
| 🪟 **Windows** | `LMNotes_0.7.0_x64_en-US.msi` | MSI |
| 🐧 **Linux** | `LMNotes_0.7.0_amd64.deb` / `.rpm` / `.AppImage` | |
| 🍎 **macOS (Apple Silicon)** | `LMNotes_0.7.0_aarch64.dmg` | 未签名：右键 → 打开 |

---

## 🔄 Changelog

### Added
- Ctrl+K 命令面板（命令 + 笔记检索 + 最近打开）（FR-SEARCH-01）
- 全局快捷键 + 快速捕获浮窗（FR-CAP-01）
- 时间线 / 每日笔记入口 / 标签云（FR-SEARCH-05，concepts.tags 列 + 老库迁移）

### Changed
- 版本 0.6.0 → **0.7.0**

---

## 🧭 Known Issues / 已知限制

- 云端 STT 路由启动期构建：新增/修改 provider 后需重启生效。
- 媒体任务并发固定为 1；流式转录未实现（见 roadmap 刻意押后）。
- macOS 为未签名 dmg（右键打开）；签名需 Apple 开发者账号 Secrets。

---

Full docs: [使用手册](../user-manual.md) · [路线图](../roadmap.md) · [v0.7 计划](../superpowers/plans/2026-09-04-v0.7.0-navigation-mvp-closure.md)
