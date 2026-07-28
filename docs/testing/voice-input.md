# 语音输入功能测试文档（FR-CAP-05 + FR-MEDIA-01）

> 麦克风录音 → 云端 Whisper 转录 → 音频归档 + 生成 `type: transcript` concept。
> 本文覆盖**自动化测试**与**手动测试**两部分，附审查中发现并已修复的问题清单。

- 关联规格：[PRD §5.2 FR-CAP-05](../specs/PRD.md)、[§5.3 FR-MEDIA-01](../specs/PRD.md)、[§3.5 多模态资源](../specs/PRD.md)
- 关联决策：[ADR-0005 护栏](../adr/ADR-0005-llm-provider-guardrails.md)、[ADR-0006 语音转录](../adr/ADR-0006-voice-transcription.md)
- 实现位置：
  - 核心层 `crates/lmnotes-core/src/llm/{provider,routing,whisper,guard}.rs`、`id.rs`
  - 配置/命令 `apps/desktop/src-tauri/src/{llm_config.rs,commands.rs,lib.rs}`
  - 前端 `apps/desktop/src/{voice/VoiceCapture.tsx,App.tsx,settings/ProviderSettings.tsx,i18n/locales/*}`

---

## 1. 测试金字塔

```
                    ┌───────────────┐
                    │  手动 E2E (§3) │  ← 麦克风 + 真实/模拟 Whisper 端点
                    └───────────────┘
                ┌───────────────────────┐
                │  集成测试（暂缺，§5）  │  ← Tauri 命令级，需 mock 状态
                └───────────────────────┘
        ┌───────────────────────────────────┐
        │  单元测试（已建，§2）              │  ← 纯 cargo test，CI 强制
        └───────────────────────────────────┘
```

自动化覆盖核心层与配置层（可纯 `cargo test`，无外网/无麦克风）；
端到端流程（录音 → 上传 → 落盘 → 打开）依赖麦克风权限与网络，归入手动测试。

---

## 2. 自动化测试（CI 强制，`cargo test --workspace --all-targets`）

> 全部为纯单元测试，无网络、无麦克风、无文件系统副作用（whisper 用 wiremock 模拟 HTTP）。

### 2.1 Whisper Provider（`crates/lmnotes-core/src/llm/whisper.rs`）

| 用例 | 验证点 | 预期 |
|---|---|---|
| `transcribe_parses_plain_text_body` | OpenAI 标准 `response_format=text` 返回裸文本 | `t.text == "你好世界"` |
| `transcribe_parses_json_fallback` | 兼容端点忽略 `response_format` 返回 `{"text":"..."}` | `t.text == "hello world"` |
| `transcribe_surfaces_http_error` | 401/5xx 等非 2xx | `Result::is_err()` |
| `is_cloud_kind` | `kind()`=Cloud、`capabilities()`=TRANSCRIBE、`id()` 正确 | 三个断言 |

### 2.2 路由（`crates/lmnotes-core/src/llm/routing.rs`）

| 用例 | 验证点 | 预期 |
|---|---|---|
| `resolves_primary_transcribe` | 首选 provider 在册 | 解析成功 + `transcribe()` 可调用 |
| `fallback_when_primary_missing_transcribe` | 首选不在册、备选在册 | 降级到备选 |
| `errors_when_all_missing_transcribe` | 首选与备选都不在册 | `is_err()` |

### 2.3 资源 ID（`crates/lmnotes-core/src/id.rs`）

| 用例 | 验证点 | 预期 |
|---|---|---|
| `resource_id_format_is_correct` | `new_resource_id("voice")` 前缀 + 4 位 Crockford | `at_voice_<4>` |
| `resource_id_lowercases_slug` | slug 大写转小写 | `at_meeting_...` |

### 2.4 配置→路由派生（`apps/desktop/src-tauri/src/llm_config.rs`）

| 用例 | 验证点 | 预期 |
|---|---|---|
| `build_auto_derives_transcribe_routing_from_provider_model` | provider 填了 `transcribe_model` 但 `routing.transcribe=None`（用户最常见场景） | **自动派生路由，`transcribe_for` 解析成功**，model 匹配 |
| `explicit_transcribe_routing_takes_precedence` | 显式配 `routing.transcribe` | 不被自动派生覆盖 |
| `build_without_transcribe_model_has_no_transcribe_routing` | provider 无 `transcribe_model` 且无显式路由 | 不插入 Transcribe 路由；`transcribe_for` 失败 |

> **回归保护**：`build_auto_derives_...` 用例是为捕获审查中发现的 **Bug #1**（用户配了 transcribe_model 但 UI 不写 routing.transcribe，导致 `no routing for task`）而加；删除 `derive_transcribe_ref` 逻辑后此用例会红。

### 2.5 护栏（`crates/lmnotes-core/src/llm/guard.rs`）

| 用例 | 验证点 | 预期 |
|---|---|---|
| `voice_path_blocked_when_cloud_not_allowed` | 默认 `cloud_allowed=false` + 云端 provider + 空内容 | `Deny`（local-first 契约） |
| `voice_path_allowed_when_cloud_authorized` | 用户显式 `cloud_allowed=true` | `Allow` |
| `voice_path_sensitive_pattern_does_not_block_empty_content` | 配了敏感词但内容为空 | `Allow`（固化"音频不可字符串扫描"的已知简化） |

### 运行方式

```bash
# 全部（与 CI 一致）
cargo test --workspace --all-targets

# 仅语音相关（按名过滤）
cargo test -p lmnotes-core --lib -- transcrib whisper resource_id voice_path
cargo test -p lmnotes-desktop --lib -- build_auto_derives build_without explicit_transcribe
```

---

## 3. 手动测试（端到端）

> 依赖：麦克风、可选的真实 Whisper API key（或 §3.5 的本地 mock 端点）。

### 3.1 前置环境

| 项 | 要求 |
|---|---|
| 麦克风 | 系统已接入；首次 `getUserMedia` 会弹权限请求，需**允许** |
| 网络 | 能访问所配 Whisper 端点 |
| Whisper provider | `~/.lmnotes/config.json` 配好（见 §3.2）或经设置 UI 配好 |
| OS | Windows/macOS 直接可用；Linux 需 PulseAudio |

### 3.2 配置 Whisper provider

**方式 A：设置 UI（推荐）**

1. 启动 `cd apps/desktop && npm run tauri dev`。
2. `Ctrl+,` 打开设置 → 找到 OpenAI 兼容 provider（若无则手动编辑 config.json 加一个，见方式 B）。
3. 填 **Base URL**（如 `https://api.openai.com/v1`、`https://api.groq.com/openai/v1`）、**API Key**、**Transcribe Model**（如 `whisper-1` / `whisper-large-v3`）。
4. 勾选 **允许云端 Provider（默认关闭，本地优先）**。
5. 保存 → **重启应用**（routing 在启动期 build，需重启生效）。

**方式 B：直接编辑 `~/.lmnotes/config.json`**

```json
{
  "providers": [
    {
      "type": "openai",
      "id": "openai",
      "base_url": "https://api.openai.com/v1",
      "api_key": "sk-...",
      "chat_model": "gpt-4o-mini",
      "embed_model": "text-embedding-3-small",
      "embed_dim": 1536,
      "transcribe_model": "whisper-1"
    }
  ],
  "routing": {
    "summarize": { "provider": "openai", "model": "gpt-4o-mini" },
    "link_suggest": { "provider": "openai", "model": "gpt-4o-mini" },
    "embed": { "provider": "openai", "model": "text-embedding-3-small" },
    "chat": { "provider": "openai", "model": "gpt-4o-mini" },
    "rewrite": { "provider": "openai", "model": "gpt-4o-mini" }
  },
  "guard": { "cloud_allowed": true, "sensitive_patterns": [] }
}
```

> `routing.transcribe` 留空即可——`build()` 会从 provider 的 `transcribe_model` 自动派生（Bug #1 修复后的行为）。
> 若要显式指定，可加 `"transcribe": { "provider": "openai", "model": "whisper-1" }`。

### 3.3 测试用例

每个用例：**前置 → 步骤 → 预期**。

---

#### TC-M01 录音 → 转录 → 生成笔记（happy path）

**前置**：§3.2 配置完成并重启；应用已启动。

**步骤**：
1. 按 `Ctrl+Shift+V`（或点左侧栏「🎤 语音」）。
2. 点 **● 开始录音** → 对麦克风说一句话（如"测试语音笔记"）。
3. 等 2–3 秒，点 **● 录音中…** 停止。

**预期**：
- 浮窗短暂显示「转录中…」。
- ~2–5s 后浮窗关闭，编辑器自动打开一条新笔记。
- 笔记路径形如 `transcripts/voicenote-YYYYMMDD.md`（标题含"Voice note"）。
- 正文为转录文字（中文可用）。
- frontmatter（用编辑器切换或看原文）含：
  ```yaml
  type: transcript
  title: Voice note YYYY-MM-DD HH:MM
  resource: /assets/audio/<2位>/<hash>.webm
  tags: [voice]
  id: at_voice_<4位>
  duration_ms: <约录音秒数×1000>
  mime: audio/webm
  transcribed_by: whisper-1@openai
  ```

---

#### TC-M02 音频归档与去重

**前置**：TC-M01 通过。

**步骤**：
1. 打开 `~/.lmnotes/default/assets/audio/<前2位hex>/`，确认存在 `<hash>.webm`。
2. 记下该文件的 SHA-256（与文件名 `at_` 前的 hex 段一致）。
3. 在编辑器里**手动改 transcript 笔记正文**触发保存。
4. 再次录一段**完全相同**的音频很难复现；改为：复制该 webm 文件到别处，用任何工具读其 bytes，
   调用 `archive_binary` 等价逻辑（或在文件系统层确认同 hash 目录下**不会**出现第二个同名文件）。

**预期**：
- 音频落 `assets/audio/<hash[..2]>/<hash>.webm`，分桶正确。
- 相同内容（同 SHA-256）只存一份（去重逻辑：`if !full.exists()` 才写）。

---

#### TC-M03 护栏：未允许云端时拒绝

**前置**：config.json 的 `guard.cloud_allowed` 改为 `false`（或设置 UI 取消勾选）→ 重启。

**步骤**：
1. `Ctrl+Shift+V` → 开始录音 → 说一句 → 停止。

**预期**：
- 浮窗显示红色错误：「转录失败：cloud not globally authorized」。
- 不生成笔记，不写音频文件（注：音频归档在护栏检查**之前**，故 assets/audio 可能已落盘——
  这是已知设计取舍，见 ADR-0006；若需"先检查再归档"可后续调整顺序）。

> ⚠️ **审查发现**：当前实现**先归档后检查护栏**（commands.rs 第 545 行 archive → 第 556 行 check）。
> 即被护栏拒绝时音频已落盘。功能上不影响（不生成 concept、不上传云端），但严格隐私洁癖下
> 可考虑把护栏检查提前到 archive 之前。列为**已知行为**，不阻塞 MVP。

---

#### TC-M04 护栏：未配 transcribe provider

**前置**：config.json 删除所有 `transcribe_model` 字段（或留空）→ 重启。

**步骤**：
1. `Ctrl+Shift+V` → 录音 → 停止。

**预期**：
- 错误：「转录失败：no routing for task Transcribe」。
- 说明：MVP 无优雅的"未配置"前端提示，仅透传后端错误。后续可在打开浮窗前预检并提示。

---

#### TC-M05 麦克风权限拒绝

**前置**：系统层禁用应用麦克风权限（Win: 设置→隐私→麦克风；macOS: 系统设置→隐私→麦克风）。

**步骤**：
1. `Ctrl+Shift+V` → 点开始录音。

**预期**：
- 浮窗显示：「已拒绝麦克风权限。」（`voice.permissionDenied`）。
- 不进入录音状态。

---

#### TC-M06 录音中途关闭浮窗

**步骤**：
1. 开始录音 → 录音中点浮窗外部（overlay 空白处）。

**预期**：
- 浮窗关闭；`onCleanup` 停止 recorder 与计时器（代码：`recorder.stop()` + `clearInterval`）。
- 麦克风指示灯熄灭（stream.getTracks().stop() 在 onstop 里——注意：若直接关窗未触发 onstop，track 可能未释放；
  **审查发现**：onCleanup 调 `recorder.stop()` 会触发 onstop → `stream.getTracks().stop()`，链路是通的）。

---

#### TC-M07 转录后的笔记可检索/被建议

**前置**：TC-M01 成功，已生成 transcript 笔记。

**步骤**：
1. 在搜索框输入转录内容的关键词 → 回车。
2. 等几秒（后台 `generate_suggestions` spawn），看右侧「建议中心」。

**预期**：
- 搜索结果命中该 transcript 笔记（`indexer.index_concept` 已索引）。
- 建议中心出现该笔记的摘要/标签建议（后台 spawn，可能需几秒）。

---

#### TC-M08 多语言 UI

**步骤**：
1. 设置 → 切换语言 中文/English。
2. 打开语音浮窗。

**预期**：标题、按钮、错误信息均随语言切换（`voice.*` key 在 en.ts/zh.ts 均有）。

---

#### TC-M09 快捷键不与粘贴冲突

**步骤**：
1. 编辑器内 `Ctrl+V` 粘贴剪贴板文本/图片。

**预期**：正常粘贴，**不**弹出语音浮窗（语音绑定的是 `Ctrl+Shift+V`）。

---

#### TC-M10 断网 → 自动降级本地 whisper.cpp

**前置**：已下载一个本地模型（设置 → 本地 STT → 下载 base）；云端 provider 配好且 `cloud_allowed=true`。

**步骤**：
1. 断开网络（关 WiFi / 拔网线）。
2. `Ctrl+Shift+V` → 录一句 → 停止。

**预期**：
- 短暂等待后（云端连接失败快速返回）本地 whisper.cpp 接手转录。
- 生成 transcript 笔记，frontmatter `transcribed_by: whisper-cpp`。
- stderr 应有 "cloud transcribe failed (...), falling back" 日志。

---

#### TC-M11 云端 401（鉴权失败）→ 不降级，报错

**前置**：本地模型已下载；云端 provider 的 API Key 故意填错（无效 key）。

**步骤**：
1. `Ctrl+Shift+V` → 录音 → 停止。

**预期**：
- 错误透传前端（如 "401 Unauthorized"），**不**降级到本地（401 属 4xx 用户配置问题，非网络不可用）。
- 这是设计取舍：让用户看到鉴权问题，而非静默用本地掩盖。

> 若希望 401 也降级，调整 `is_network_error` 放宽 4xx（不推荐，会掩盖配置错误）。

---

#### TC-M12 云端不可达 + 本地模型未下载 → 引导下载

**前置**：断网；本地无任何模型（`~/.lmnotes/models/` 空）。

**步骤**：
1. `Ctrl+Shift+V` → 录音 → 停止。

**预期**：
- 错误信息提示"云端不可用且本地未就绪，请到设置下载模型"（`voice.cloudDownNoLocal`）。
- 用户到 **设置 → 本地 STT** 下载模型后重试即成功。

---

#### TC-M13 模型下载中断恢复

**步骤**：
1. 设置 → 本地 STT → 点下载 small（~466MB）。
2. 下载到一半断网/关应用。
3. 重新打开，再次点下载。

**预期**：
- 进度条从 0 重新计（当前实现未做断点续传的续传，而是 `.part` 临时文件被覆盖重下）。
- 完整下载完成后 `.part` 重命名为 `ggml-small.bin`，状态刷新为已下载。

> 已知限制：当前 `download_whisper_model` 未实现 HTTP Range 断点续传，中断需重头下。后续可加。

---

#### TC-M14 本地 binary 缺失 → 不降级，明确报错

**前置**：开发模式（未打包 sidecar），且 `~/.lmnotes/bin/whisper` 不存在、PATH 无 whisper。

**步骤**：
1. 设置 → 本地 STT：应显示 "whisper.cpp 引擎：未找到"。
2. 断网 + 录音。

**预期**：
- 录音报错，提示引擎缺失（不静默失败）。
- 发布版（安装包含 sidecar）此场景不出现。

---

#### TC-M15 长音频本地转录超时保护

**步骤**：
1. 仅本地模式（不配云端）+ 本地模型就绪。
2. 录一段 >60s 的音频 → 停止。

**预期**：
- whisper.cpp 子进程在 60s 超时后被 kill，错误透传（"whisper.cpp timed out (60s)"）。
- 不卡死应用。长音频留 FR-CAP-09 后台队列（P2）。

---

### 3.4 验证产物 OKF 合规性

对 TC-M01 生成的 transcript 笔记，用 CLI Validator 校验：

```bash
cargo run -p lmnotes-cli -- validate ~/.lmnotes/default/transcripts/<file>.md
```

预期：通过（`type`/`id`/`resource` 字段合法，frontmatter round-trip 安全）。

### 3.5 无 API key 时用本地 mock 端点测试（可选）

不想花 OpenAI 配额 / 无网络时，可起一个本地假 Whisper 端点：

```bash
# 用 Python 起一个始终返回固定文本的 mock（需 requests/httpx 之外的 stdlib 即可）
python -m http.server 8080  # 仅示意；实际需响应 POST /audio/transcriptions
```

更可靠的是用 `wiremock`（已在核心层测试用）。但把它接进运行中的应用较重，
**推荐**：开发期临时把 `WhisperProvider::new("local-mock", "http://localhost:3030", "k")` 写进 config.json，
配合一个返回 `"mock transcript"` 的本地 HTTP 服务即可跑通除真实转录质量外的全部流程。

---

## 4. 自动化测试覆盖矩阵

| 需求点（来自 ADR-0006 / PRD） | 自动化 | 手动 |
|---|---|---|
| Whisper HTTP multipart 上传 + text/JSON 解析 | ✅ §2.1 | — |
| 路由首选→降级→全失败 | ✅ §2.2 | — |
| `at_` 资源 ID 格式 | ✅ §2.3 | — |
| config 自动派生 transcribe 路由 | ✅ §2.4 | — |
| 护栏 cloud_allowed 闸门（语音路径） | ✅ §2.5 | TC-M03 |
| 音频 SHA-256 归档去重 | （未单测）| TC-M02 |
| OKF transcript concept 生成（frontmatter 字段） | （未单测）| TC-M01 + §3.4 |
| 增量索引 + 建议生成 | （复用现有索引测试）| TC-M07 |
| 麦克风采集 / MediaRecorder | — | TC-M05/M06 |
| 端到端打开笔记 | — | TC-M01 |
| 多语言 | — | TC-M08 |
| 快捷键冲突 | — | TC-M09 |

**覆盖空白（后续可补自动化）**：
- `archive_binary` 的存储路径与去重（需临时 vault 目录 + `tokio::fs` 测试夹具）。
- `create_voice_note` 编排器整体（需 mock 全部 5 个 State，工作量大，性价比低，暂靠手动）。

---

## 5. 审查发现的问题清单（本次已修 / 已记录）

| # | 严重度 | 问题 | 状态 |
|---|---|---|---|
| 1 | 🔴 高 | 用户在 provider 填 `transcribe_model` 但 UI 不写 `routing.transcribe` → `transcribe_for` 报 "no routing"，功能不可用 | ✅ 已修：`build_routing()` 自动从首个带 `transcribe_model` 的 provider 派生；加 §2.4 回归测试 |
| 2 | 🟡 中 | 录制失败后录音按钮 `disabled`，用户无法在同一浮窗重试 | ✅ 已修：移除 `disabled={!!error()}`，`start()` 已清错 |
| 3 | 🟡 中 | 音频**先归档后检查护栏**，被 cloud_allowed 拒绝时音频已落盘 | 📝 已知行为，记 TC-M03；不阻塞 MVP，后续可调整顺序 |
| 4 | 🟢 低 | 未配 transcribe provider 时前端无优雅提示，只透传后端错误 | 📝 记 TC-M04；后续可在开浮窗前预检 |
| 5 | 🟢 低 | 设置 UI 健康检查不探测 Whisper provider（只探 chat/embed） | 📝 小优化，后续可加 |
| 6 | 🟢 低 | `archive_binary` 用同步 `full.exists()`（Tauri 壳层允许，非 bug） | 📝 不改 |

---

## 6. CI 集成

语音相关自动化测试已被 `cargo test --workspace --all-targets` 覆盖，无需额外 CI 配置。
前端 `npx tsc --noEmit && npm run build` 覆盖 `VoiceCapture.tsx`/`ProviderSettings.tsx` 的类型与编译。

```bash
# 本地完整门禁（与 CI 一致）
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cd apps/desktop && npx tsc --noEmit && npm run build
```

手动测试用例（§3）不进 CI，**发版前**按 §3 逐条跑一遍。
