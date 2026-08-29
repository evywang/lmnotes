# LMNotes v0.5.0

> **后台任务、模板、导出、macOS —— 效率与分发。**

v0.5 的主题是**效率与分发**:长媒体处理转入后台任务队列;新建笔记可选模板;一键导出库;macOS 安装包首次随版本分发。

---

## ✨ Highlights / 核心亮点

### ⏳ 媒体任务队列(FR-MEDIA-04,收编 FR-CAP-09)

- 拖入的**长音视频**不再阻塞:自动进入后台队列(单并发 worker),转录完成通过任务中心通知。
- **任务中心**(侧栏「⏳ 任务」):状态徽章(排队/处理中/完成/失败)、打开产物笔记、失败一键**重试**、排队中可取消。
- 应用意外退出后,处理中的任务**重启自动续跑**;网络类失败自动重试一次。
- 视频抽音轨(ffmpeg)放宽到 15 分钟预算。

### 📋 笔记模板(FR-CAP-08)

- vault 下建 `templates/*.md` 即成模板;新建笔记时可选。
- 占位符:`{{title}}` / `{{date}}` / `{{time}}` / `{{datetime}}`(本地时区);未识别占位符原样保留。

### 📦 数据导出(FR-STORE-05)

- **一键导出 ZIP**(设置 → 数据):包含全部笔记与资产,**排除派生数据**(`.lmnotes/`),流式写入不占内存。
- **初始化 git 仓库**:自动写 `.gitignore`(排除派生数据)并完成首次提交(需已配置 git 身份,否则给出指引)。

### 🍎 macOS 安装包

- release 矩阵新增 **macOS(Apple Silicon,dmg)**。
- whisper.cpp 与精简版 ffmpeg 在 CI **从源码构建**(上游无 macOS 预编译)。
- 未配置 Apple 开发者证书时发布**未签名 dmg**(首次打开需右键 → 打开);配置 Secrets 后自动签名公证。

### 随 v0.4 的能力(同分支交付)

- 多 Vault(重启式切换,ADR-0008)、音视频拖拽转录、图片自动描述(VisionCap)、按住说话。

---

## 📥 Download / 下载

| Platform | File | Notes |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.5.0_x64-setup.exe` | NSIS(推荐) |
| 🪟 **Windows** | `LMNotes_0.5.0_x64_en-US.msi` | MSI |
| 🐧 **Linux** | `LMNotes_0.5.0_amd64.deb` / `.rpm` / `.AppImage` | |
| 🍎 **macOS (Apple Silicon)** | `LMNotes_0.5.0_aarch64.dmg` | 未签名:右键 → 打开 |

---

## 🔄 Changelog

### Added
- 媒体任务队列 + 任务中心(FR-MEDIA-04);长音频/视频后台转录(FR-CAP-09)
- 笔记模板系统(FR-CAP-08)
- 库导出 ZIP / git 仓库初始化(FR-STORE-05)
- macOS aarch64 构建与 dmg 产物(可选签名公证)

### Changed
- 版本 0.4.0 → **0.5.0**;release 矩阵 Win + Linux + macOS

### Fixed
- 应用重启后遗留的 running 媒体任务自动恢复为排队

---

## 🧭 Known Issues / 已知限制

- 本地 whisper.cpp 子进程超时仍为 60s(云端无此限):>60s 的长录音请配置云端转录,本地引擎适合速记长度。
- running 中的任务不支持强杀取消(仅排队中可取消)。
- macOS 为未签名 dmg(Gatekeeper 提示需右键打开);签名需 Apple 开发者账号 Secrets。

---

Full docs: [使用手册](../user-manual.md) · [路线图](../roadmap.md) · [ADR-0007](../adr/ADR-0007-local-stt-fallback.md) · [ADR-0008](../adr/ADR-0008-multi-vault-restart-switch.md)
