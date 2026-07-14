# ADR-0006: 语音转录（Voice Transcription）

| 状态 | Accepted |
| 日期 | 2026-07-12 |
| 关联 | PRD §3.5（多模态资源）、§5.2 FR-CAP-05、§5.3 FR-MEDIA-01/FR-MEDIA-05；ADR-0005（护栏）、ADR-0001（ID 策略） |
| 决策者 | LMNotes 维护者 |

## 背景（Context）

PRD §5.2 把"语音输入：按住说话 / 流式转录，可选用本地 Whisper 或云端"列为 **MVP**（FR-CAP-05），
§5.3 把"音频自动转录，转录稿写入 `type: transcript` 的描述 concept（§3.5）"也列为 **MVP**（FR-MEDIA-01），
§5.3 FR-MEDIA-05 要求"处理引擎可插拔（本地 whisper.cpp / 云端 API）"。

ADR-0005 已为转录预留了能力位与 trait（`TranscribeCap`）、`Task::Transcribe` 路由项、以及 provider 表
（云端 OpenAI 兼容 / 本地 whisper.cpp 子进程）。截至本 ADR 前，这些均为**未实现的占位设计**：
`Capabilities` 仅有 `CHAT|EMBED`、`Task` 枚举无 `Transcribe`、无任何音频/Whisper 代码、`reqwest` 未启用 multipart。

实现语音→笔记需要拍板的关键 forces：
1. **MVP 速度 vs 完整性**：本地 whisper.cpp 需用户自装二进制 + 模型权重（数百 MB）、子进程管线复杂；
   云端 Whisper API（OpenAI 兼容 `/audio/transcriptions`）用已有 `reqwest` 即可，几小时落地。
2. **隐私**：录音天然敏感；ADR-0005 的 `cloud_allowed` 默认 OFF、local-first 默认须保留。
3. **OKF 合规**：§3.5 规定音频本体平铺 `assets/audio/<hash>`，转录稿作为独立 `type: transcript` concept 存 `transcripts/`，
   其 `resource` 字段指向音频本体，扩展字段（`duration_ms`/`mime`/`transcribed_by`）经 `Frontmatter.extra` 透传。
4. **架构边界**：`lmnotes-core` 禁止 `std::fs`（ADR-0002）；音频归档与 concept 落盘在 Tauri 壳层用 `tokio::fs`，
   核心层只做 provider 抽象与转录调用。

## 决策（Decision）

**MVP 范围**：点按开始/停止录音 → 麦克风采集 → 云端 Whisper 转录 → 音频归档 + 生成 transcript concept → 打开。

具体决策：

1. **引擎：云端 Whisper API 优先**。新增 `WhisperProvider`（`crates/lmnotes-core/src/llm/whisper.rs`），
   仿 `OpenAiProvider` 结构，multipart POST `/audio/transcriptions`，兼容 OpenAI / Groq / GLM 等端点。
   `reqwest` 在 `lmnotes-core` 启用 `multipart` feature。本地 whisper.cpp 子进程留作 **P1**（不静态链接权重，遵 ADR-0005 §2）。

2. **能力抽象落地**（实现 ADR-0005 预留位）：
   - `Capabilities::TRANSCRIBE = 1 << 2`。
   - `TranscribeCap` trait + `AudioInput { bytes, mime, filename }` + `Transcript { text }`。
   - `Task::Transcribe` + `Registry::{transcribes, register_transcribe, register_transcribe_arc, transcribe_for}`
     （首选→降级，与 `chat_for`/`embed_for` 完全对称）。

3. **交互形态：toggle（点按开始/停止）而非 push-to-talk（按住说话）**。
   理由：浏览器 `getUserMedia` + `MediaRecorder` API 天然是 start/stop 语义，push-to-talk 需额外全局键监听与长按手势，
   属体验优化，留 P1。前端用 `MediaRecorder`（默认 audio/webm;codecs=opus，Safari 退化到 mp4），音频 bytes 经 Tauri IPC 传入命令。

4. **批量转录而非流式**。Whisper `/audio/transcriptions` 是一次性 batch 端点；真流式需 Deepgram/Groq streaming WebSocket，
   复杂度与 MVP 不匹配，留 P1。录制结束后整体上传。

5. **产物形态：音频归档 + transcript concept（PRD §3.5 完整形态）**。
   - 音频 SHA-256 去重存 `assets/audio/<hash[..2]>/<hash>.<ext>`（复用图片归档逻辑，抽出 `archive_binary(data, ext, kind)`）。
   - 转录稿写成 `transcripts/<slug>-<YYYYMMDD>.md`，`type: transcript`，frontmatter 用 `resource`（OKF 官方字段）指向音频，
     `duration_ms`/`mime`/`transcribed_by`/`language` 经 `Frontmatter.extra` 透传（round-trip 安全，有单测）。
   - 资源 ID 用新增的 `new_resource_id(slug) -> "at_<slug>_<4位Crockford>"`（补齐 ADR-0001 §3.4 中 `at_` 前缀的未实现部分）。

6. **护栏对音频的简化**：`guard::check()` 当前按 `content: &str` 扫敏感关键词，音频 bytes 不可字符串扫描。
   转录路径传空串，仅依赖 `cloud_allowed` + `local_only` 两道闸；此路径无源 concept，`local_only` 恒 false，
   故云端默认被 `cloud_allowed=false` 拒（符合 FR-LLM-08 / ADR-0005 默认本地优先）。**遗留风险见后果段。**

7. **配置**：`ProviderConfig::OpenAi` 增 `transcribe_model: Option<String>`（Ollama 不支持，不加）；
   `RoutingConfig` 增 `transcribe: Option<ProviderRefSer>`；默认全本地 Ollama → `transcribe: None`（不启用）。
   用户需显式配云端 provider + 填 Transcribe Model + 勾选允许云端。

8. **编排器命令** `create_voice_note`：归档 → 取 provider → 护栏 → 转录 → 构建 `Frontmatter`+`Concept`（用结构体而非手写 yaml）→
   落盘 → `indexer.index_concept` + spawn `generate_suggestions`（复刻 watcher 分支）。返回 transcript concept 路径。
   另抽 `insert_audio` 作为原子能力（FR-CAP-04 拖拽音频复用）。

## 后果（Consequences）

- **正面**：
  - 云端路径几小时落地，无需分发模型权重，安装包体积零增长。
  - 能力抽象对称、可插拔：P1 加本地 whisper.cpp 只需再写一个 `impl TranscribeCap for WhisperCppProvider` 并在配置注册。
  - 产物严格符合 OKF §3.5，音频与转录稿分离、可独立检索/引用，无锁定。
  - 默认本地优先：开箱无云端依赖，用户显式 opt-in 才走云端。

- **负面**：
  - 依赖网络与外部 API（无网 / 无 API key 时语音功能不可用）。
  - `MediaRecorder` 默认 webm/opus；少数非 OpenAI 实现可能只收 wav/mp3（留 P1 前端转码）。
  - 护栏对音频内容不做敏感关键词扫描（仅 cloud_allowed + local_only）——见缓解。
  - `Vec<u8>` 经 Tauri IPC 传输，对 <1min 短录音无碍；长音频需流式落盘（留 FR-CAP-09 P2）。

- **缓解**：
  - **隐私**：默认 `cloud_allowed=false`；首启探测无云端 provider 时语音入口降级提示；设置页明确标注"转录需上云"。
    未来若引入本地 whisper.cpp，敏感录音可强制走本地（与 ADR-0005 `local_only` 一致）。
  - **格式兼容**：`WhisperProvider::transcribe` 同时解析裸文本与 JSON `{"text":...}` 两种响应，覆盖多数兼容端点。
  - **可演进**：所有决策点（引擎、交互、形态）均为独立可替换项；P1 引入本地引擎或流式时不改产物格式与护栏接口。

## 考虑过的替代方案（Alternatives）

1. **本地 whisper.cpp 优先**：local-first 最纯粹、音频不出本机。未选作 MVP 因：需用户自装 whisper.cpp + 权重、
   子进程管线与临时 wav 转码复杂、首启体验差。留 P1，届时作为云端之外的降级 provider。
2. **两者同时做**：工作量翻倍，且 MVP 验证"语音→笔记"核心闭环只需一条可用路径。先云端验证流程，再补本地。
3. **流式转录**：体验更好（边说边出字），但需 WebSocket provider（Deepgram/Groq streaming），与 MVP 的 batch Whisper 不兼容，
   且前端管线复杂。留 P1。
4. **push-to-talk（按住说话）**：移动端更自然，但桌面 webview 需全局键监听 + 长按手势，复杂度高于 toggle。留 P1。
5. **音频用完即弃（只留文字）**：实现最简，但丢失原始录音、无法复核转录准确性、不符合 §3.5 的"资源归档"理念。未选。
6. **静态链接 whisper.cpp 权重进二进制**：ADR-0005 Alternatives 已明确拒绝（license 污染 + 体积膨胀）。不重审。
