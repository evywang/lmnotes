# 本地 STT 降级方案（whisper.cpp）—— FR-MEDIA-05 实现

## 目标与定位

云端 Whisper 不可用时（网络断、API key 失效、端点宕），**运行时自动降级到本地 whisper.cpp 子进程转录**，用户无感。这是 ADR-0006 标记为 P1 的"本地 whisper.cpp 作为云端之外的降级 provider"的落地，也是 FR-MEDIA-05（引擎可插拔）的另一半。

用户已确认三个方向：
1. **whisper.cpp 可执行文件**：打包预编译二进制（Tauri sidecar，安装包 +5~10MB）。
2. **模型权重 .gguf**：首次用语音时按需下载到 `~/.lmnotes/models/`，安装包不增重。
3. **降级触发**：运行时——云端 `transcribe()` 返回连接/超时错误 → 自动切本地重试。

## 关键现状（已核实）

- `TranscribeCap` trait（`crates/lmnotes-core/src/llm/provider.rs:91`）是干净的扩展点，`WhisperProvider`（云）已实现，照抄结构即可。
- **`transcribe_for` 只按注册选 provider，不调 `health()`**（`routing.rs:138-157`）；`Routing.map` 的 `Vec<ProviderRef>` fallback 槽位存在但 `build_routing()` 恒填 `vec![]`。
- **`create_voice_note` 是单发**（`commands.rs:544-575`）：调一次 `transcribe_for` → guard → `.transcribe()?`，无重试/降级。
- 无 sidecar 基础设施：`tauri.conf.json` 无 `externalBin`/`resources`；`capabilities/default.json` 仅 `core:default`+`dialog:default`；无 `tauri-plugin-shell`。
- 音频格式差：前端出 webm/opus；whisper.cpp 要 16kHz mono WAV。需 Rust 侧转码。
- `tokio` 的 `process` feature 未启用；现有子进程是 `std::process::Command::spawn`（explorer/xdg-open，`commands.rs:754-791`）。
- ADR-0006 §决策1 明确"不静态链接权重"。

---

## 任务分解（T1–T10）

### T1 · `WhisperCppProvider`（新文件 `crates/lmnotes-core/src/llm/whisper_cpp.rs`）

照 `whisper.rs` 结构。**核心差异**：kind=Local、`transcribe()` 走子进程而非 HTTP。

```rust
pub struct WhisperCppProvider {
    id: String,                 // "whisper-cpp"
    binary_path: PathBuf,       // sidecar 解析后的路径（由 Tauri 壳注入）
    model_path: PathBuf,        // ~/.lmnotes/models/ggml-xxx.bin
}
```

- `LlmProvider` impl：`kind()=Local`、`capabilities()=TRANSCRIBE`、`health()` = 探测 `binary_path` 与 `model_path` 都存在 + `--help` 退出码 0（Local provider，guard 恒放行）。
- `TranscribeCap::transcribe()`：
  1. 把 `audio.bytes`（webm/opus）写临时文件 `<tmp>.<ext>`（在 Tauri 壳层的 tempdir）。
  2. 调 `ffmpeg`（或 symphonia，见 T2）转 16kHz mono WAV `<tmp>.wav`。
  3. `tokio::process::Command::new(&binary_path)`
     `.args(["-m", model_path, "-f", wav_path, "-otxt", "-l", language_or_auto, "-of", out_prefix])`
     `.output().await`（读 stdout/stderr，超时 60s）。
  4. 读 `<out_prefix>.txt`，返回 `Transcript { text }`。
  5. 清理临时文件（`defer`-style，用 RAII guard 或显式 drop）。
- **注意 ADR-0002 边界**：`lmnotes-core` 禁止 `std::fs`。所以 `whisper_cpp.rs` 不能在核心层直接写临时文件/调子进程。**解法**：核心层只定义 trait + 一个 **trait-based 的宿主能力接口** `WhisperRunner`（注入：写临时文件、跑命令、读结果），由 Tauri 壳层实现。或者更简单——**把 `WhisperCppProvider` 整个放到 Tauri 壳层**（`apps/desktop/src-tauri/src/whisper_cpp.rs`），因为它本来就要用 `tokio::fs`/`tokio::process`，且它不被核心层其他模块复用。

  → **决定：放 Tauri 壳层**（`apps/desktop/src-tauri/src/whisper_cpp.rs`），实现 `lmnotes_core::llm::TranscribeCap`。核心层零改动即支持。这与现有 `OllamaProvider`/`OpenAiProvider` 在核心层、`WhisperCppProvider` 在壳层的分工一致（壳层负责系统资源：子进程、临时文件、模型路径）。

- **单测**（壳层 lib test，用 `tempfile`）：mock 一个假 binary（写个临时脚本 echo 固定文本），验证参数拼装 + 输出读取。或更稳：把"命令构造"抽成纯函数 `fn build_cmd(binary, model, wav, lang) -> Command`，单测它（不实际 spawn）。

### T2 · 音频转码 webm/opus → 16kHz mono WAV

三选一：
- **A（推荐）**：依赖 `ffmpeg` sidecar——把 ffmpeg 也作为 sidecar 打包（+~20MB，但转码质量/兼容性最好，且 whisper.cpp 本就常配 ffmpeg）。**用户选了"打包预编译二进制"，ffmpeg 同模式打包即可**。
- **B**：Rust 纯解 `symphonia`（+依赖，解 opus）+ 手写 WAV header（无外部二进制，但 opus 解码器依赖较重，且 symphonia 的 opus 支持需 feature gate）。
- **C**：前端 `getUserMedia` 改采 WAV——浏览器原生不产 WAV，需加 npm 编码库（`wav-encoder`/`lamejs`），且 WAV 体积大。

→ **本计划选 A**（ffmpeg sidecar），理由：whisper.cpp 工作流标配 ffmpeg、转码最稳、与 whisper.cpp sidecar 同一套 Tauri sidecar 机制复用。**留 B 作 P2**（若体积敏感再换 symphonia）。

转码命令：`ffmpeg -i <in> -ar 16000 -ac 1 -c:a pcm_s16le <out>.wav`

### T3 · Tauri sidecar 打包（whisper + ffmpeg）

**`apps/desktop/src-tauri/tauri.conf.json`**（`bundle` 块加）：
```json
"externalBin": [
  "binaries/whisper",
  "binaries/ffmpeg"
]
```

**二进制文件**（不入 git，CI 下载）放 `apps/desktop/src-tauri/binaries/`，按 target-triple 命名：
- `binaries/whisper-x86_64-pc-windows-msvc.exe`
- `binaries/whisper-x86_64-unknown-linux-gnu`
- `binaries/ffmpeg-x86_64-pc-windows-msvc.exe`、`binaries/ffmpeg-x86_64-unknown-linux-gnu`

macOS 留作后续（当前 release 矩阵只有 Win+Linux）。

**`.gitignore`** 加：
```
apps/desktop/src-tauri/binaries/
*.gguf
*.bin
```
（防止误提交大文件）

**`apps/desktop/src-tauri/Cargo.toml`**：
- `tokio` features 加 `"process"`（异步子进程）。
- 加 `tauri-plugin-shell = "2"`（sidecar API）。

**`apps/desktop/src-tauri/src/lib.rs`**：`.plugin(tauri_plugin_shell::init())`。

**`apps/desktop/src-tauri/capabilities/default.json`**：加 sidecar 执行权限（scoped）：
```json
"permissions": [
  "core:default",
  "dialog:default",
  "shell:allow-execute",
  { "identifier": "shell:allow-execute", "allow": [
    { "name": "whisper", "sidecar": true },
    { "name": "ffmpeg", "sidecar": true }
  ]}
]
```

### T4 · CI 下载预编译二进制（`.github/workflows/release.yml`）

在 `tauri-action` 步骤**之前**加一步（每平台 matrix 各跑），从官方 release 下载 whisper.cpp 与 ffmpeg 预编译包，解压到 `apps/desktop/src-tauri/binaries/` 并按 target-triple 重命名：

```yaml
- name: Fetch whisper.cpp + ffmpeg sidecar binaries
  shell: bash
  run: |
    .github/scripts/fetch-sidecars.sh ${{ matrix.target }}
```

新建 `.github/scripts/fetch-sidecars.sh`：
- 按 target 选 GHR URL：whisper.cpp 从 `ggml-org/whisper.cpp` releases（含预编译 release zip），ffmpeg 从 `BtbN/FFmpeg-Builds`（Win）/ `johnvansickle/ffmpeg`（Linux）。
- 下载、解压、rename 到 `binaries/<name>-<target-triple>[.exe]`、`chmod +x`。
- 失败则 fail-fast（release 不应静默缺 sidecar）。

CI（`ci.yml`）也要加一步保证 dev/本地构建不因缺 sidecar 报错——dev 模式 sidecar 缺失时降级为"本地 STT 不可用，仅云端"，**不阻塞编译**（externalBin 缺失只影响 bundle，不影响 `cargo build`/`tauri dev`）。

### T5 · 模型按需下载（首次语音时）

**后端命令**（`commands.rs` 新增）：
- `list_whisper_models() -> Vec<WhisperModel>`：返回可选模型清单（base/small/medium，含大小、中英文推荐）。硬编码元数据（名称、URL、大小、推荐语）。
- `download_whisper_model(name) -> String`：从 HuggingFace `ggml-org/whisper.cpp` repo 下载 `ggml-<name>.bin` 到 `~/.lmnotes/models/`，返回本地路径。用 `reqwest` 流式 + 进度事件（`app.emit("whisper-model-progress", {name, downloaded, total})`），前端可显示进度条。
- `get_local_stt_status() -> LocalSttStatus { binary_available: bool, model_path: Option<String>, models_dir_free_mb: u64 }`：探测 sidecar 是否就绪、已下哪个模型。

**模型来源**：`https://huggingface.co/ggml-org/whisper.cpp/resolve/main/ggml-base.bin` 等（HF 直链，稳定、可断点续传）。

### T6 · 配置层（`llm_config.rs`）

加 `ProviderConfig::WhisperCpp` 变体（用户可显式启用/调参）：
```rust
#[serde(rename = "whisper_cpp")]
WhisperCpp {
    /// 模型名（ggml-<name>.bin 的 <name>，如 "small"）。None 则用默认 "base"。
    #[serde(default)]
    model: Option<String>,
    /// 覆盖默认 binary 路径（默认走 sidecar）。高级用户用。
    #[serde(default)]
    binary_path: Option<String>,
    /// 线程数（默认 4）。
    #[serde(default = "default_threads")]
    threads: usize,
}
```

**`Config::build()`**：
- 遍历 providers，遇到 `WhisperCpp` 变体 → 解析 sidecar 路径（`app_handle` 注入 or 默认 `~/.lmnotes/`）+ 模型路径（`~/.lmnotes/models/ggml-<name>.bin`）→ 构造 `WhisperCppProvider` → `reg.register_transcribe(...)`。
- **默认启用**：即使用户 config.json 没写 `WhisperCpp`，`build()` 仍**自动注册一个**（用 sidecar + base 模型，若存在），使其天然作为 fallback 可用。符合"打包了就该开箱可用"。

**`build_routing()`**：Transcribe 路由的 **fallback 槽位填本地**：
```rust
// 若注册了 whisper-cpp，把它作为任何云端 transcribe primary 的 fallback
let local_fallback = if reg_has_whisper_cpp { 
    vec![ProviderRef { provider_id: "whisper-cpp".into(), model: "base".into() }] 
} else { vec![] };
map.insert(Task::Transcribe, (primary, local_fallback));
```
（routing 是 build 完才有的，这里需要在 build_routing 拿到 registry 信息——可能要把"是否注册了 whisper-cpp"作为参数传入，或在 build() 里 post-process routing。）

**`probe_providers`**：加 `WhisperCpp` 分支（否则 match 不 exhaustive 编译失败），探测 binary+model 存在性。

**`Default` 配置**：仍保持本地 Ollama 主导；transcribe 不显式配（依赖运行时自动注册的 whisper-cpp + 用户配置的云端）。

### T7 · 运行时降级逻辑（`create_voice_note`）

**关键改动**（`commands.rs:544-575` 区域）：把单发改成"先云端、失败降本地"。

抽一个内部函数 `transcribe_with_fallback`：
```rust
async fn transcribe_with_fallback(
    registry: &Registry, routing: &Routing, guard_cfg: &GuardConfig,
    audio: AudioInput, language: Option<&str>,
) -> Result<(Transcript, String /*provider_id*/), String> {
    let (primary, model) = registry.transcribe_for(routing, Task::Transcribe)?;
    // 1) 过护栏（云端需 cloud_allowed；本地恒放行）
    match check(guard_cfg, primary.kind(), "", false) {
        GuardDecision::Deny(r) => return Err(r),  // 云端被拒：不降级，直接报错（用户主动关了云）
        GuardDecision::Allow => {}
    }
    // 2) 试首选
    match primary.transcribe(audio.clone(), &model, language).await {
        Ok(t) => return Ok((t, primary.id().to_string())),
        Err(e) if primary.kind() == Cloud => {
            eprintln!("cloud transcribe failed ({e:?}), falling back to local");
            // 3) 降级到本地 whisper-cpp
        }
        Err(e) => return Err(e.to_string()),  // 本地失败不降级
    }
    // 4) 找 fallback 链里的 Local provider
    for fb in fallbacks {
        if fb.kind() == Local {
            return fb.transcribe(audio, model, language).await
                .map(|t| (t, fb.id().to_string()))
                .map_err(|e| format!("local fallback also failed: {e}"));
        }
    }
    Err("cloud transcribe failed and no local fallback available".into())
}
```

**降级触发条件**（核心）：只对 `ProviderKind::Cloud` 的**网络类错误**降级（连接拒绝、超时、DNS、HTTP 5xx）。需要 `CoreError` 能区分网络错误——看现有 `CoreError::Http(reqwest::Error)`，`reqwest::Error::is_connect()`/`is_timeout()` 可判。**本地错误（如模型缺失）不降级**，避免无谓重试。

`create_voice_note` 用 `transcribe_with_fallback` 替换原 544-575 行，返回的 `provider_id` 写进 frontmatter 的 `transcribed_by`（这样能看出这次是云还是本地转录的）。

### T8 · 前端：模型下载 UI + 状态提示

**新组件 `apps/desktop/src/voice/LocalSttSetup.tsx`**（在 VoiceCapture 里条件渲染）：
- 开语音浮窗前/首次录音后，调 `get_local_stt_status`。
- 若云端可达 → 直接录（不烦用户）。
- 若云端不可达 + 本地未就绪（无模型）→ 弹"云端不可用，本地需下载模型"对话框：
  - 列模型（base ~140MB 推荐 / small ~460MB / medium ~1.5GB），中英质量说明。
  - 点下载 → `invoke("download_whisper_model")` + 监听 `whisper-model-progress` 事件显示进度条。
  - 下载完成 → 自动继续转录流程。
- 若云端不可达 + 本地就绪 → 直接录（静默降级，`transcribed_by` 会标 `whisper-cpp`）。

**i18n**：`localStt.*` 系列 key（中英双语）：标题、模型选择、下载中、下载失败、云端不可用提示等。

**ProviderSettings**：加"本地 STT"小节，显示 sidecar 状态、已下模型、删除/换模型按钮、默认线程数。

### T9 · ADR-0007 + 文档同步

- **`docs/adr/ADR-0007-local-stt-fallback.md`**：记录决策——whisper.cpp sidecar 打包（非静态链接）、模型按需下载、运行时网络错误自动降级、ffmpeg sidecar 转码、`WhisperCppProvider` 置壳层。Alternatives：symphonia 纯 Rust 解码（P2）、启动时探活（被否，每次延迟）、用户手选（被否，非自动降级）。
- **更新 ADR-0006**：状态加注"Superseded in part by ADR-0007（本地引擎已落地）"，或仅在 ADR-0007 引用即可（不改 ADR-0006 正文——ADR 不回改）。
- **`docs/adr/README.md`**：索引加 ADR-0007。
- **`README.md`**：功能表"语音输入"行更新为"麦克风录音→（云端不可用时自动降级本地 whisper.cpp）→transcript 笔记"；前置要求加一行 whisper.cpp/ffmpeg 已随包分发（用户无需自装）。
- **`docs/user-manual.md` §2.9**：加"本地降级"说明 + 首次模型下载流程。
- **`docs/testing/voice-input.md`**：加本地降级测试用例（TC-M10~M13：断网降级、模型缺失、下载中断恢复、本地转录质量）。
- **PRD §5.3 FR-MEDIA-05**：标注"✅ MVP/P1 已实现（本地 whisper.cpp 可插拔 + 自动降级）"。

### T10 · 测试

**自动化（cargo test）**：
- `whisper_cpp.rs`：`build_cmd()` 纯函数单测（参数拼装正确性）+ 一个 mock binary 的集成测试（tempfile 写个 echo 脚本当 whisper，验证输出读取 + 清理）。
- `llm_config.rs`：`WhisperCpp` 变体的 serde round-trip + `build()` 自动注册 whisper-cpp 的逻辑（需 stub sidecar 路径存在性——可能要把"探测存在性"抽成可注入闭包）。
- `routing.rs`：加 `transcribe_for` 跨 Cloud→Local fallback 的解析测试（构造一个 fake Cloud + fake Local transcriber）。
- `transcribe_with_fallback`：抽到核心层或壳层独立函数，单测"云失败→本地成功"、"云成功不调本地"、"本地失败不降级"——用 fake providers（Cloud 报 `CoreError::Http`，Local 返回成功）。

**手动（写入 testing/voice-input.md）**：
- TC-M10：拔网 → 录音 → 应静默降级本地，frontmatter `transcribed_by: whisper-cpp@local`。
- TC-M11：配错 API key（云端 401）→ 应降级本地（401 属"云端失败"）。
- TC-M12：本地模型未下载 + 云端不可达 → 弹下载框；下载中断重试。
- TC-M13：本地 binary 缺失（删 sidecar）→ 仅报错不降级，引导用户。
- TC-M14：长音频（>60s）→ 本地转录 + 60s 超时保护。

---

## 不做（留后续）

- **macOS 支持**：当前 release 矩阵无 macOS，sidecar 暂只 Win+Linux；macOS 需加 target-triple + code-signing。
- **GPU 加速**（CUDA/CoreML/Metal）：whisper.cpp 预编译版默认 CPU；GPU 加速需平台特化 build，留 P2。
- **symphonia 纯 Rust 转码**（去 ffmpeg 依赖）：留 P2，体积优化。
- **流式本地转录**：whisper.cpp 是 batch；流式留 P2。
- **VAD/语音活动检测**降噪：留 P2。

---

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| 安装包体积 +25~30MB（whisper+ffmpeg） | 可接受（仍远小于模型）；后续 P2 换 symphonia 去 ffmpeg |
| whisper.cpp/ffmpeg 预编译 release 平台覆盖不全 | fetch 脚本兜底：缺则从源码 CI 编译（加 cmake 步骤） |
| 模型下载失败/中断 | 断点续传（HTTP Range）+ 重试 + 明确错误提示 |
| 子进程超时/卡死 | 60s 超时 + kill；错误透传前端 |
| 首次体验：云端可用时用户永远不知道本地能降级 | 设置页"本地 STT"小节透明展示 + 手动"测试本地"按钮 |
| ADR-0002 边界：子进程属系统资源 | `WhisperCppProvider` 置 Tauri 壳层，核心层零 fs/process 改动 |

---

## 实现顺序

T1（壳层 WhisperCppProvider）→ T2（ffmpeg 转码）→ T3（sidecar 打包配置）→ T4（CI 下载脚本）→ T5（模型下载命令）→ T6（配置层）→ T7（运行时降级，核心逻辑）→ T8（前端 UI）→ T9（文档）→ T10（测试，贯穿）。

建议拆提交：`feat(stt): whisper.cpp local provider + sidecar packaging` / `feat(stt): runtime cloud→local fallback` / `feat(stt): model download UI` / `docs(adr): ADR-0007 local STT fallback` / `test(stt): local fallback + provider tests`。

Sources:
- [Tauri v2 Sidecar 官方文档](https://v2.tauri.app/develop/sidecar/)
- [whisper.cpp 仓库（CLI flags）](https://github.com/ggml-org/whisper.cpp/blob/master/include/whisper.h)
