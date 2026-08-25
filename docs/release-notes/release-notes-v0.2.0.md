# LMNotes v0.2.0

> **Voice input lands: speak, and it becomes a note. — 语音输入来了:说一句话,自动变成笔记。**

v0.2 的主题是**语音**:麦克风录音 → 转录 → 自动生成符合 OKF §3.5 的转录笔记。云端优先、本地兜底。

---

## ✨ Highlights / 核心亮点

### 🎤 语音输入(FR-CAP-05 / FR-MEDIA-01 / FR-MEDIA-05)

- **一键录音转笔记** — `Ctrl+Shift+V` 或侧栏「🎤 语音」,点按开始/停止,说一句话自动生成 `type: transcript` 笔记并在编辑器打开。
- **双引擎:云端优先,本地自动降级(ADR-0006 / ADR-0007)** —
  - 云端:OpenAI 兼容 Whisper 端点(OpenAI / Groq / GLM);
  - 本地:**whisper.cpp 随安装包分发**(含 ffmpeg 转码),云端不可达(断网/超时/5xx)时**运行时自动切换本地**,用户无感;`transcribed_by` 字段标注本次来源。
- **音频归档** — 原始录音按 SHA-256 去重存 `assets/audio/`,转录笔记经 `resource` 字段反向引用,可回溯可复核。
- **模型按需下载** — 首次离线使用时在设置里选 base(142MB)/ small(466MB)/ medium(1.5GB)下载,带进度条。
- **隐私护栏不变** — 云端转录仍受 `cloud_allowed` 门控(默认关,本地优先);本地引擎音频不出本机;401 等配置错误**不**静默降级,让问题可见。

### 🖥️ 桌面体验修复

- **对话框标题统一为应用名** — 替换浏览器原生 `prompt/alert/confirm`(标题曾是页面地址),输入框/确认框/错误提示现在都显示 **LMNotes**,支持 Enter/Esc 与默认值。
- **文件树点击切换修复** — 此前会话内只有第一个打开的文件能显示,点其他文件编辑器不换内容(非 keyed `<Show>` 未重挂载);现在每次点击正确加载。
- 打包管线修复 — whisper.cpp/ffmpeg sidecar 正确进安装包(Windows: whisper.exe + ffmpeg.exe + ggml CPU 变体 DLL)。

---

## 📥 Download / 下载

| Platform | File | Notes |
|---|---|---|
| 🪟 **Windows** | `LMNotes_0.2.0_x64-setup.exe` | NSIS 安装程序(推荐,内置语音引擎) |
| 🪟 **Windows** | `LMNotes_0.2.0_x64_en-US.msi` | MSI 安装包(企业部署) |
| 🐧 **Linux (Debian/Ubuntu)** | `LMNotes_0.2.0_amd64.deb` | `sudo dpkg -i` 安装 |
| 🐧 **Linux (Fedora/RHEL)** | `LMNotes-0.2.0-1.x86_64.rpm` | `sudo rpm -i` 安装 |
| 🐧 **Linux (通用)** | `LMNotes_0.2.0_amd64.AppImage` | 免安装,双击运行 |

---

## 🚀 Quick Start with Voice / 语音快速开始

1. 安装后按 `Ctrl+Shift+V` 打开语音浮窗(首次需授权麦克风)。
2. **云端路径**(可选):设置(`Ctrl+,`)里给 OpenAI 兼容 provider 填 Transcribe Model(如 `whisper-1`)并勾选"允许云端"。
3. **离线路径**:设置 → 「🎙️ 本地 STT」下载一个模型(推荐 small),断网也能转。
4. 说完点停止 → 笔记自动生成,正文即转录文字,可被搜索命中并进入建议中心。

---

## 🔄 Changelog

### Added
- 语音输入:录音 → 转录 → transcript 笔记(FR-CAP-05、FR-MEDIA-01)
- 本地 whisper.cpp 引擎 + 运行时云端→本地自动降级(FR-MEDIA-05、ADR-0007)
- 模型按需下载 UI(base/small/medium,进度条)
- `insert_audio` / `list_whisper_models` / `download_whisper_model` / `get_local_stt_status` 命令
- 错误分类 `classify_transcribe_error`(网络/配置/其他)与降级单测(共 105+ 用例)

### Fixed
- 文件树点击不切换编辑器内容(非 keyed Show 不重挂载)
- 原生 alert/prompt/confirm 标题显示页面地址 → 应用名标题对话框
- 云端 5xx 不触发本地降级(Conformance 包装丢失状态码)
- CI sidecar 下载脚本:仓库迁移(ggerganov → ggml-org)、CLI 改名 whisper-cli、DLL 伴生打包

### Packaging
- 安装包内置 whisper.cpp + ffmpeg sidecar(Windows 46MB / Linux 对应)
- 版本 0.1.0 → **0.2.0**

---

## 🧭 Known Issues / 已知限制

- 流式转录、按住说话、macOS sidecar 留待后续(ADR-0006/0007 P1/P2)。
- 模型下载暂无断点续传,中断需重下。
- 长录音(>60s)本地转录会超时保护,后台队列属 FR-CAP-09(P2)。
- 401 等鉴权错误不降级(设计取舍:让配置问题可见)。

---

Full docs: [使用手册](../user-manual.md) · [语音测试计划](../testing/voice-input.md) · [ADR-0006](../adr/ADR-0006-voice-transcription.md) · [ADR-0007](../adr/ADR-0007-local-stt-fallback.md)
