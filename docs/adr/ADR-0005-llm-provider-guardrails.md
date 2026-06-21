# ADR-0005: LLM Provider 抽象与隐私护栏

| 状态 | Accepted |
| 日期 | 2026-06-21 |
| 关联 | PRD §5.4, §7, §10, §15.6（O6c）; ADR-0002 |
| 决策者 | 用户（O6c：本地 LLM 不确定，需云端 fallback） |

## 背景（Context）

LMNotes 的核心差异化是"LLM 原生"（PRD §1.1, §2）：摘要、链接建议、问答、改写、转录、视觉——每类任务可能用不同模型，且模型来源混合（本地 Ollama / 云端 GLM/OpenAI 兼容）。

forces：
1. **任务多样性**：chat / embed / transcribe / vision 四种能力，单一 Provider 通常不全。
2. **隐私**：本地优先（§10）；云端必须用户显式授权，敏感内容默认仅本地（FR-LLM-08）。
3. **本地 LLM 不确定（O6c）**：首次启动可能无本地环境，需云端 fallback 但要明确告知隐私含义。
4. **可观测**：用量、成本、是否上云需透明（FR-MODEL-05）。
5. **核心层承载**（ADR-0002）：Provider 编排在 Rust 核心，前端只选配置。

## 决策（Decision）

### 1. Provider 抽象（核心层 `lmnotes-core::llm`）

按能力拆分 trait（避免纯 chat 的 Provider 强制实现空 transcribe/vision，评审 F7）：

```rust
/// 所有 Provider 必须实现：身份、类型、能力声明、健康检查
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;                          // 唯一标识
    fn kind(&self) -> ProviderKind;                // Local | Cloud
    fn capabilities(&self) -> Capabilities;        // {chat, embed, transcribe, vision} 位集
    async fn health(&self) -> Result<HealthStatus>; // 连通性 + 配额
}

/// 能力是独立 trait，Provider 按需实现；调度侧按 trait object 动态分发
#[async_trait]
pub trait ChatCap: LlmProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatStream>;
}
#[async_trait]
pub trait EmbedCap: LlmProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}
#[async_trait]
pub trait TranscribeCap: LlmProvider {
    async fn transcribe(&self, audio: AudioInput) -> Result<Transcript>;
}
#[async_trait]
pub trait VisionCap: LlmProvider {
    async fn vision(&self, image: ImageInput, prompt: &str) -> Result<String>;
}
```

- **能力探测驱动 UI 与路由**：调度器拿到 Provider 后，按任务所需能力做 trait 检查（如 `provider.capabilities().contains(Cap::Chat)` 且能 downcast 到 `ChatCap`）；设置面板按任务只列出具备该能力的 Provider。
- **能力缺失的降级**：某任务的路由首选 Provider 缺该能力时，调度器按 Routing 配置回退到次选（如视觉首选 Provider 无 vision → 回退到 OpenAI 兼容）。
- **Provider 注册表**：内置实现 + 用户自定义（OpenAI 兼容口任意 base_url + key）。

### 2. 内置 Provider（M0–M4 覆盖）

| Provider | kind | capabilities | 角色 |
|---|---|---|---|
| **Ollama** | Local | chat / embed（视模型） | 默认本地首选 |
| **OpenAI 兼容** | Cloud | chat / embed / vision / transcribe(Whisper) | 通用云端 fallback |
| **GLM** | Cloud | chat / embed / vision | 国内可用云端 |
| **whisper.cpp** | Local | transcribe | 本地转录首选 |

> 命令式子进程调用（Ollama HTTP、whisper.cpp CLI/绑定），**不静态链接模型权重或 GPL 依赖**，规避许可证传染（O6a）。

### 3. 任务→Provider 路由（Routing）

```rust
pub struct Routing {
    summarize: ProviderRef,
    link_suggest: ProviderRef,
    embed: ProviderRef,
    chat: ProviderRef,
    transcribe: ProviderRef,
    vision: ProviderRef,
}
```
- 全局默认 + **单条 concept 覆盖**（frontmatter 扩展字段 `llm_local_only: true` 强制仅本地）。
- 设置面板可视化分派；切换即时生效。

### 4. 隐私护栏（强制，核心层不可绕过）

```
                   ┌─ concept 标记 local_only? ─┐
请求 → PreFilter ──┤                            ├─→ 命中 → 仅允许 kind=Local 的 Provider
                   └─ 命中敏感关键词/正则? ─────┘
                                                    └─→ Provider kind=Cloud?
                                                         └─→ 用户全局授权 cloud? 否 → 拒绝并提示
```

- **三层门**：① concept 级 `llm_local_only` 标记；② 用户配置的敏感关键词/正则清单；③ 云端全局授权开关（默认关，FR-LLM-08）。
- **任一命中且目标 Provider 为 Cloud → 拒绝**，返回结构化错误并提示用户。
- **不可绕过**：护栏在核心层 `dispatch()` 入口，所有任务类型必经。
- **脱敏日志**：云端调用只记 `{endpoint, tokens, ts}`，**绝不记内容**（§10）。

### 5. 首次启动探测（对应 O6c）

```
on_first_run:
  1. 探测本地 Ollama (localhost:11434 /health) 与 whisper.cpp 可用性
  2. 若可用 → 默认 Routing 全部指向本地 Provider，提示"本地优先已启用"
  3. 若不可用 → 引导配置云端 Provider（GLM/OpenAI 兼容），
     显式弹窗告知"将上传内容到云端，可在设置随时关闭/切换为本地"
  4. 不预填任何 API Key；密钥存 OS 钥匙串（§10）
```

### 6. 可观测

- 用量仪表盘：本地调用次数 / 云端 token 估算 / 按任务分布（FR-MODEL-05）。
- 每次 LLM 产出带溯源（source concept ids + run id），写入 SQLite 建议表，可回滚（FR-LLM-09，ADR-0003）。

## 后果（Consequences）

**正面：**
- 任务分派灵活，不同任务用最合适模型（O6c 的 fallback 自然落地）。
- 三层护栏强制隐私默认，本地优先可信。
- 核心层承载编排，前端只配置，符合 ADR-0002 边界。
- 子进程调用规避许可证传染，兼容开源（O6a）。

**负面：**
- Provider 抽象需覆盖能力差异（如本地 Ollama 无 vision 时降级）。缓解：按能力拆 trait（ChatCap/EmbedCap/TranscribeCap/VisionCap），调度器按 trait 动态分发；缺能力任务自动回退到次选 Provider（见决策 1）。
- 护栏可能误判（敏感关键词过宽）。缓解：清单可配置、可按 concept 例外、误拒时给明确原因。
- 云端 token 成本不可见。缓解：仪表盘估算 + 单次调用前预览 token 数（高级选项）。

**缓解措施汇总：** 能力降级回退、护栏可配置例外、成本仪表盘。

## 考虑过的替代方案（Alternatives）

- **单一 Provider（仅 Ollama）**：拒绝。O6c 明确本地环境不确定，且云端 GLM/OpenAI 兼容是常见需求，单一会限制可用性。
- **把护栏放前端**：拒绝。前端可被绕过（devtools/篡改），违背"强制"语义；必须在核心层 dispatch 入口。
- **静态链接 whisper.cpp / 模型权重**：拒绝。权重多为非商业或 GPL 风险，且包体爆炸；子进程调用更干净。
- **Provider 配置硬编码**：拒绝。用户需自定义 base_url/key（自建 OpenAI 兼容服务常见）；必须支持任意 OpenAI 兼容端点。
- **把摘要等任务直接写死模型**：拒绝。任务→模型分派是核心差异化（§7.3），需可配置。
