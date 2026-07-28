# ADR-0007: 本地 STT 降级（whisper.cpp sidecar）

| 状态 | Accepted |
| 日期 | 2026-07-21 |
| 关联 | PRD §5.3 FR-MEDIA-05（引擎可插拔）；ADR-0005（护栏）、ADR-0006（语音 MVP，标 P1）、ADR-0002（核心层边界） |
| 决策者 | LMNotes 维护者 |

## 背景（Context）

ADR-0006 把语音转录 MVP 落到云端 Whisper API，并把"本地 whisper.cpp 作为云端之外的降级 provider"显式留作 P1。
当云端不可用（断网、API key 失效、端点宕机、首启无配置）时，MVP 直接报错、功能不可用——对 local-first 产品是硬伤。
本 ADR 决策本地 STT 的获取、打包、触发与降级方式，兑现 FR-MEDIA-05"引擎可插拔（本地 whisper.cpp / 云端 API）"。

关键 forces：
1. **零依赖体验** vs **本地可用**：README 承诺"装包即用、无需自装"。本地引擎要么随包分发，要么首启引导下载——不能要求用户编译 whisper.cpp。
2. **体积**：whisper.cpp 二进制 ~5MB、ffmpeg ~20MB 可接受随包；但 .bin 模型 140MB~1.5GB 不能进安装包。
3. **降级语义**：用户期望"云端不可用时自动用本地"，而非"启动时探活"或"手动切换"——要求运行时失败检测 + 自动重试。
4. **架构边界**：whisper.cpp 调子进程 + 写临时 wav 是系统资源操作，触 ADR-0002 的 `lmnotes-core` `std::fs` 禁令。
5. **音频格式差**：前端 `MediaRecorder` 出 webm/opus；whisper.cpp 要 16kHz mono WAV——必须转码。

现有抽象已就绪：`TranscribeCap` trait（ADR-0005 预留）、`Task::Transcribe` 路由、`Registry::transcribe_for`（首选→降级）、
`create_voice_note` 编排器。但 `transcribe_for` 只按注册选、不调 `health()`、不重试；`Routing.map` 的 fallback 槽位恒空。

## 决策（Decision）

**核心**：云端 `transcribe()` 返回网络类错误时，运行时自动切本地 whisper.cpp 重试，用户无感。

1. **whisper.cpp 二进制：Tauri sidecar 打包**。`tauri.conf.json` 的 `externalBin` 在 release 时经 `TAURI_CONFIG` 注入
   （dev 不注入，避免本地构建报缺二进制）；CI（`.github/scripts/fetch-sidecars.sh`）从 `ggml-org/whisper.cpp` releases
   下载预编译包，按 target-triple 命名（`whisper-x86_64-pc-windows-msvc.exe` 等）。**不静态链接权重**（遵 ADR-0006 §决策1）。
   运行时路径解析：env 覆盖 > `~/.lmnotes/bin/` > PATH（Unix `which`）。Capabilities 加 scoped `shell:allow-execute`。

2. **模型权重：首次按需下载**。`~/.lmnotes/models/ggml-<name>.bin`，来源 HuggingFace `ggml-org/whisper.cpp`。
   `download_whisper_model` 命令流式下载 + `whisper-model-progress` 事件（250ms 节流）。安装包不增重。`.gitignore` 加 `ggml-*.bin`/`*.gguf`。

3. **`WhisperCppProvider` 置 Tauri 壳层**（`apps/desktop/src-tauri/src/whisper_cpp.rs`），非 `lmnotes-core`。
   理由：它要 `tokio::process`/`tokio::fs`（触核心层 `std::fs` 禁令），且不被核心层其他模块复用。
   实现 `lmnotes_core::llm::TranscribeCap`——核心层零改动即接入。`kind()=Local`、`health()=binary+model 存在`。

4. **音频转码：ffmpeg sidecar**（同 whisper.cpp 打包机制）。webm/opus → 16kHz mono PCM WAV。
   留 symphonia 纯 Rust 解码作 P2（去 ffmpeg 依赖，体积优化）。

5. **运行时降级**（关键）：新增 `transcribe_with_fallback`（commands.rs）+ `Registry::transcribe_candidates`（routing.rs）。
   - `build_routing()` 把 whisper-cpp 作为任何云端 transcribe primary 的 fallback（`Routing.map[Transcribe].1`）。
   - `Config::build()` 自动注册一个 whisper-cpp provider（即便用户 config.json 没声明）——sidecar 可达即"开箱可用"。
   - 降级触发：仅 `ProviderKind::Cloud` 的网络类错误（`reqwest::Error::is_connect/is_timeout`、HTTP 5xx）；
     **4xx（401/403 鉴权）不降级**（用户配置问题应可见）；本地错误不降级（避免无谓重试）。
   - 每个候选独立过护栏：云端被 `cloud_allowed=false` 拒时**降级**到本地（而非直接报错），让本地能接手。

6. **配置**：`ProviderConfig` 加 `WhisperCpp { model, binary_path, ffmpeg_path, threads }` 变体（用户可显式调参）；
   默认 `threads=4`、`model=base`。`probe_providers` 加 WhisperCpp 分支，探测 binary+model 存在性。

7. **前端**：`LocalSttSetup.tsx`（挂在 ProviderSettings）显示引擎状态 + 模型列表 + 下载进度。
   `VoiceCapture` 在云端不可达且本地未就绪时引导用户下载（错误文案 `voice.cloudDownNoLocal`）。

## 后果（Consequences）

- **正面**：
  - 离线/断网/云端宕机时语音功能仍可用，符合 local-first 产品定位。
  - 降级完全透明：`frontmatter.transcribed_by` 标 `whisper-cpp` 让用户知晓本次来源，但无需手动切换。
  - 引擎可插拔兑现 FR-MEDIA-05：再加一个引擎只需 `impl TranscribeCap` + 注册。
  - 二进制随包、模型按需——安装包仅 +25MB，首启零配置（云端可用时不烦用户下载模型）。

- **负面**：
  - 安装包 +25~30MB（whisper ~5MB + ffmpeg ~20MB）。
  - 首次离线用：需先下载模型（base 140MB 起），首次有等待。
  - 子进程转录有开销（启动 + 60s 超时），短录音体验不如云端。
  - `fetch-sidecars.sh` 依赖上游 release 命名稳定，需定期维护。
  - macOS 暂不支持（release 矩阵只有 Win+Linux）。

- **缓解**：
  - 模型下载断点续传（HTTP Range）+ 进度可见 + 失败可重试。
  - 子进程 60s 超时 + kill，长音频留 FR-CAP-09 后台队列。
  - 设置页"本地 STT"面板透明展示引擎/模型状态，用户可手动测试。
  - CI fetch 脚本失败 fail-fast，不静默缺 sidecar。

## 考虑过的替代方案（Alternatives）

1. **symphonia 纯 Rust 转码**（去 ffmpeg 依赖）：体积最优，但 opus 解码器依赖重、且 symphonia opus 需 feature gate。
   未选作 MVP——ffmpeg 是 whisper.cpp 工作流标配、转码最稳。留 P2。
2. **启动时探活决定走云还是本地**：每次转录前多一次网络往返，且云端"可达但慢"时仍会卡。
   未选——运行时失败降级更符合"云端优先、本地兜底"语义。
3. **用户在设置里显式选云/本地/仅本地**：最可预测但非"自动降级"，违背用户原始需求"web service 不可用时作为默认方案"。
   作为补充（设置面板透传状态），但不作为主路径。
4. **模型随安装包打包**（base ~140MB）：开箱即用但安装包过重，不符合 ~10MB 轻量定位。未选。
5. **静态链接 whisper.cpp 权重**：ADR-0006 Alternatives 已拒绝（license + 体积）。不重审。
6. **`WhisperCppProvider` 置核心层 + trait 注入宿主能力**：过度设计——核心层无其他调用方，壳层实现更简单且不破边界。未选。

## 状态字段说明

本 ADR 不 supersede ADR-0006；ADR-0006 描述 MVP（云端优先）仍有效，本 ADR 在其基础上**补齐 P1 本地降级**。
二者并存：云优先（ADR-0006）+ 本地兜底（本 ADR）。
