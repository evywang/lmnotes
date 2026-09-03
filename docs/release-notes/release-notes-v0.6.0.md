# LMNotes v0.6.0

> **任务可控 · 语音打磨 —— 可靠性收口。**

v0.6 的主题是**可控与可靠**:媒体任务补上强杀取消与超时预算(v0.5.1 并入本版);
语音输入修复 Windows 打包可用性问题,并把模型下载、云端 STT 配置打磨到开箱即用。

---

## ✨ Highlights / 核心亮点

### 🛑 媒体任务:可取消 + 超时预算(FR-MEDIA-04 收口)

- **运行中任务可取消**:任务中心对 running 任务点「取消」,子进程(whisper/ffmpeg)随之终止,
  不再只是改状态说谎;完成与取消的竞态由条件 UPDATE 裁决,结果不会被误覆盖。
- **超时预算上移到调用点**:后台队列 15 分钟 / 前台速记 60 秒;**云端转录请求首次获得超时上限**
  (此前可能无限挂起)。
- **长媒体自动后台分流(GAP-A)**:拖拽/粘贴与语音弹窗按实际时长探测,超过阈值(默认 60s,设置页
  可调,也可设为全部后台)自动入队,短媒体保持秒级即时反馈。
- **修复排队视频转录失败(GAP-B)**:此前排队视频被误归档到 `assets/audio/`,跳过抽音轨导致
  whisper 拒读容器;现按 mime 正确归位 `assets/video/`。
- **断网自动重试真的生效(GAP-C)**:重试判定从字符串匹配改为类型化错误分类,云端连接类失败
  现在会兑现"自动重试一次"。
- **审计修复**:ffmpeg 孤儿进程(补 `kill_on_drop`)、内联抽音轨 60s 预算、取消注册表泄漏。

### 🎤 语音输入:Windows 可用性修复(关键)

- 修复 **sidecar 平台配置不被 Tauri 2 自动加载**的问题(`sidecar-*.json` →
  `tauri.{windows,linux,macos}.conf.json`):此前 Windows 安装包里 whisper.cpp/ffmpeg
  从未随包分发,**语音输入在打包版完全不可用**,本版起恢复。

### ⬇️ 模型下载:修复 + 弹窗内直接下载

- 修复模型下载 404(仓库 URL 指向错误的 `ggml-org/whisper.cpp`,正规模型在 `ggerganov` 下);
  官方 HuggingFace 不可达时**自动切换 hf-mirror.com 镜像**(大陆网络友好);30s 响应超时快速失败。
- 语音弹窗打开即探测本地就绪状态:引擎在而无模型 → **弹窗内嵌下载面板**,下载完成自动收起,
  **无需重启应用**(whisper.cpp 转录时动态解析模型路径)。

### 🔊 多 Provider 云端 STT

- 设置页可**添加/删除 OpenAI 兼容 provider**:主 LLM 无转录端点时(如智谱 GLM 不提供
  `/audio/transcriptions`),可专门加一个 STT provider(如硅基流动
  `FunAudioLLM/SenseVoiceSmall`)。
- Transcribe Model 字段仅对 OpenAI 兼容 provider 显示;**转录路由自动派生**到第一个填了
  转录模型的 provider,无需手动配置 routing。

---

## 📥 Download / 下载

| Platform | File | Notes |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.6.0_x64-setup.exe` | NSIS(推荐) |
| 🪟 **Windows** | `LMNotes_0.6.0_x64_en-US.msi` | MSI |
| 🐧 **Linux** | `LMNotes_0.6.0_amd64.deb` / `.rpm` / `.AppImage` | |
| 🍎 **macOS (Apple Silicon)** | `LMNotes_0.6.0_aarch64.dmg` | 未签名:右键 → 打开 |

---

## 🔄 Changelog

### Added
- 媒体任务强杀取消 + 长媒体后台分流 + 阈值设置(FR-MEDIA-04 收口,v0.5.1 并入)
- 语音弹窗内联模型下载(免重启);多 Provider 云端 STT 配置与自动路由

### Changed
- 版本 0.5.0 → **0.6.0**;sidecar 平台配置更名为 `tauri.*.conf.json`(Tauri 2 自动加载)
- 转录超时预算上移调用点:队列 15min / 内联 60s;云端请求首次有超时上限

### Fixed
- Windows 打包版语音输入不可用(sidecar 未随包分发)
- 模型下载 404(仓库 URL 错误);新增 hf-mirror 镜像回退与 30s 响应超时
- 排队视频误归 `assets/audio/` 导致转录失败(GAP-B)
- 云端断网失败未自动重试(GAP-C:类型化错误分类)
- ffmpeg 孤儿进程、内联抽音轨无超时、取消注册表泄漏(审计)

---

## 🧭 Known Issues / 已知限制

- 云端 STT 路由在启动期构建:新增/修改 provider 后需**重启应用**生效。
- 媒体任务并发固定为 1(whisper.cpp 单进程吃满 CPU,串行即最优)。
- 流式转录仍未实现(速记场景批量转录已够用,见 roadmap 刻意押后)。
- macOS 为未签名 dmg(Gatekeeper 提示需右键打开);签名需 Apple 开发者账号 Secrets。

---

Full docs: [使用手册](../user-manual.md) · [路线图](../roadmap.md) · [ADR-0006](../adr/ADR-0006-voice-transcription.md) · [ADR-0007](../adr/ADR-0007-local-stt-fallback.md) · [v0.5.1 设计](../superpowers/plans/2026-08-29-v0.5.1-task-cancel-and-timeouts.md)
