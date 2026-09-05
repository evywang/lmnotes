/**
 * 中文消息字典。key 必须与 en.ts 完全一致（由 MessageKey 类型约束，缺译会 TS 报错）。
 * 这是当前 UI 的原始文案，逐条对应。
 */
import type { MessageKey } from "./en";

export const zh: Record<MessageKey, string> = {
  // ── App shell ─────────────────────────────────────────────────────────
  "app.searchPlaceholder": "搜索…（回车）",
  "app.newNoteTitle": "新笔记",
  "app.noteTitlePrompt": "笔记标题：",
  "app.templatePrompt": "模板名（留空 = 空白笔记）：",
  "app.noTemplate": "（空白笔记）",
  "app.newNoteBtn": "+ 新建",
  "app.newNoteTooltip": "新建笔记",
  "app.importBtn": "📥 导入",
  "app.importTooltip": "导入 .md 文件",
  "app.importFilterName": "文档",
  "app.chatBtn": "💬 Chat with Vault (Ctrl+J)",
  "app.searching": "搜索中…",
  "app.searchHint": "输入关键词搜索笔记",
  "app.files": "📁 文件",
  "app.placeholder": "选择左侧笔记或搜索",
  "app.suggestionCenter": "建议中心",
  "app.settingsTooltip": "Provider 设置 (Ctrl+,)",
  // ── 通用对话框（应用名标题）───────────────────────────────────────────
  "dialog.ok": "确定",
  "dialog.cancel": "取消",

  // ── 多库管理 (FR-STORE-01) ─────────────────────────────────────────────
  "vault.title": "库",
  "vault.add": "添加库…",
  "vault.remove": "移除",
  "vault.switch": "切换",
  "vault.removeConfirm": "把 \"{name}\" 移出库清单？（不删除任何文件）",
  "vault.switchConfirm": "切换库并重启应用？未保存改动会自动保存（800ms 防抖）。",
  "vault.hint": "切换会重启应用，索引、文件监听与 MCP server 将干净地重新绑定。",
  "vault.badgeTooltip": "当前库——点击管理",
  "app.graphBtn": "🕸 知识图谱 (Ctrl+G)",
  "app.voiceBtn": "🎤 语音",
  "app.voiceTooltip": "语音输入 → 转录笔记 (Ctrl+Shift+V)",

  // ── Capture ───────────────────────────────────────────────────────────
  "capture.placeholder": "快速记一条…（Esc 关闭，Ctrl+Enter 保存）",
  "capture.saving": "保存中…",

  // ── 语音输入 (FR-CAP-05) ──────────────────────────────────────────────
  "voice.title": "🎤 语音笔记",
  "voice.start": "开始录音",
  "voice.holdToRecord": "按住说话",
  "voice.recording": "● 录音中… {sec}s（点击停止）",
  "voice.processing": "转录中…",
  "voice.permissionDenied": "已拒绝麦克风权限。",
  "voice.errorTranscribe": "转录失败：{msg}",
  "voice.queuedHint": "录音超过阈值——已加入后台队列转录。请在「⏳ 任务」查看进度。",
  "editor.mediaQueued": "已加入后台队列转录（⏳ 任务可查）。",
  "voice.cloudDownNoLocal": "云端转录不可用且本地未就绪。可下载一个本地模型，启用离线降级。",
  "voice.openSettings": "打开设置下载模型",
  "voice.localSetupHint": "尚无本地模型。下载后即可离线转录（云端不可用时自动降级），无需重启：",

  // ── 本地 STT（whisper.cpp 降级，ADR-0007）─────────────────────────────
  "localStt.title": "🎙️ 本地 STT（离线降级）",
  "localStt.description": "云端 Whisper 不可达时，转录会自动降级到本地 whisper.cpp。下载一个模型即可启用。",
  "localStt.binaryOk": "whisper.cpp 引擎：就绪",
  "localStt.binaryMissing": "whisper.cpp 引擎：未找到（随安装包分发）",
  "localStt.ffmpegOk": "ffmpeg（音频转码）：就绪",
  "localStt.ffmpegMissing": "ffmpeg（音频转码）：未找到",
  "localStt.models": "模型",
  "localStt.download": "下载",
  "localStt.downloading": "下载中… {downloaded} / {total} MB",
  "localStt.downloadingIndeterminate": "下载中… {downloaded} MB",
  "localStt.downloadFailed": "下载失败：{msg}",
  "localStt.noModelsDownloaded": "尚未下载任何模型。",
  "localStt.modelNote.base": "体积最小，速度最快；英文尚可，中文一般。推荐首试。",
  "localStt.modelNote.small": "中英文质量与速度平衡；日常速记推荐。",
  "localStt.modelNote.medium": "中文质量好但慢、占空间；长录音场景用。",

  // ── Chat drawer ───────────────────────────────────────────────────────────────
  "chat.title": "💬 Chat with Vault",
  "chat.clearTooltip": "清空历史",
  "chat.clear": "清空",
  "chat.emptyPrompt": "问我关于你笔记的任何问题…",
  "chat.emptyExample": "例如：注意力机制的公式是什么？",
  "chat.inputPlaceholder": "问一个问题…（Enter 发送，Shift+Enter 换行）",

  // ── Editor ────────────────────────────────────────────────────────────
  "editor.toggleTooltip": "编辑/预览切换",
  "editor.edit": "✏️ 编辑",
  "editor.preview": "👁 预览",
  "editor.loading": "加载中…",
  "editor.history": "历史",
  "editor.historyTooltip": "快照历史与恢复",
  "editor.extractActions": "✅ 抽取行动项",
  "editor.extractBusy": "抽取中…",
  "editor.extractTooltip": "从此转录/会议笔记抽取行动项",
  "editor.mediaTranscribing": "正在转录拖入的媒体…",
  "editor.mediaFailed": "媒体转录失败：",

  // ── 历史版本面板 (FR-LLM-09) ──────────────────────────────────────────
  "history.empty": "暂无快照。每次 LLM 改写前会自动保存快照。",
  "history.restore": "恢复此版本",
  "history.restoring": "恢复中…",
  "history.previewTitle": "快照 · {time}",
  "history.close": "关闭",

  // ── 行动项 (FR-LLM-06) ────────────────────────────────────────────────
  "actions.extractFailed": "抽取失败：",

  // ── 双链补全 (FR-CAP-03) ──────────────────────────────────────────────
  "linkComplete.detail": "笔记",

  // ── 媒体任务中心 (FR-MEDIA-04) ────────────────────────────────────────
  "tasks.openBtn": "⏳ 任务",
  "tasks.title": "媒体任务",
  "tasks.empty": "暂无媒体任务。拖一个音视频文件到编辑器即可后台转录。",
  "tasks.pending": "排队中",
  "tasks.running": "处理中",
  "tasks.done": "已完成",
  "tasks.failed": "失败",
  "tasks.cancelled": "已取消",
  "tasks.openResult": "打开笔记",
  "tasks.retry": "重试",
  "tasks.cancel": "取消",
  "tasks.cancelRunningHint": "取消运行中的任务会立即终止其子进程。",

  // ── Rewrite menu ──────────────────────────────────────────────────────
  "rewrite.polish": "润色",
  "rewrite.expand": "扩写",
  "rewrite.translate": "翻译为英文",
  "rewrite.summarize": "总结要点",
  "rewrite.busy": "改写中…",

  // ── Provider settings ─────────────────────────────────────────────────
  "settings.title": "Provider 设置",
  "settings.loading": "加载中…",
  "settings.ollamaLocal": "Ollama（本地）",
  "settings.openaiCompat": "OpenAI 兼容：{id}",
  "settings.health": "健康状态",
  "settings.healthy": "可用",
  "settings.unhealthy": "不可达",
  "settings.reprobe": "重新探测",
  "settings.cloudAllowed": "允许云端 Provider（默认关闭，本地优先）",
  "settings.transcribeModel": "转录模型（可选）",
  "settings.transcribeModelPlaceholder": "如 whisper-1（填后启用语音输入）",
  "settings.transcribeModelHint": "需端点支持 Whisper 兼容接口 /audio/transcriptions（如 OpenAI、硅基流动；智谱 GLM 不支持）。主 Provider 无此端点时，可新增一个专门用于在线转录。",
  "settings.addProvider": "添加 OpenAI 兼容 Provider",
  "settings.removeProvider": "移除",
  "settings.visionModel": "视觉模型（可选）",
  "settings.visionModelPlaceholder": "如 gpt-4o-mini / llava（粘贴图片自动生成描述）",
  "settings.backgroundMedia": "所有媒体转录入队后台处理",
  "settings.backgroundMediaHint": "关（默认）：60s 内的录音同步转录、即时反馈，更长媒体自动入队；开：全部入队（⏳ 任务可查进度）。",

  // ── 数据导出 (FR-STORE-05) ────────────────────────────────────────────
  "data.title": "数据",
  "data.exportZip": "导出库为 ZIP",
  "data.exportDialog": "导出库到",
  "data.exportDone": "已导出 {n} 个文件。",
  "data.gitInit": "初始化 git 仓库",
  "data.hint": "ZIP 不含派生数据（.lmnotes/）。git init 会写入排除它的 .gitignore，配置好 git 身份后自动完成首次提交。",
  "settings.saving": "保存中…",
  "settings.save": "保存",
  "settings.cancel": "取消",
  "settings.restartHint": "保存后需重启应用生效。默认配置指向本地 Ollama（localhost:11434）。",
  "settings.language": "语言",
  "settings.languageZh": "中文",
  "settings.languageEn": "English",
  "settings.generalSection": "通用",

  // ── Suggestion center ─────────────────────────────────────────────────
  "suggestion.empty": "暂无待审建议",
  "suggestion.acceptTooltip": "接受 (Enter)",
  "suggestion.rejectTooltip": "拒绝",

  // ── File tree ─────────────────────────────────────────────────────────
  "filetree.empty": "暂无笔记",
  "filetree.newNoteTooltip": "新建笔记",
  "filetree.newFolderTooltip": "新建文件夹",
  "filetree.deleteTooltip": "删除",
  "filetree.ctxNewNote": "📄 新建笔记",
  "filetree.ctxNewFolder": "📁 新建文件夹",
  "filetree.ctxOpen": "📄 打开",
  "filetree.ctxDelete": "🗑 删除",
  "filetree.ctxMove": "✂️ 移动到…",
  "filetree.ctxReveal": "🖥 在文件管理器中打开",
  "filetree.deleteConfirm": "确定删除 \"{name}\"？此操作不可撤销。",
  "filetree.deleteFailed": "删除失败: ",
  "filetree.createFailed": "创建失败: ",
  "filetree.folderNamePrompt": "文件夹名称：",
  "filetree.folderNameDefault": "new-folder",
  "filetree.openFailed": "打开失败: ",
  "filetree.moveFailed": "移动失败: ",
  "filetree.noMoveTarget": "没有可移动到的目录",
  "filetree.moveDialogTitle": "移动 \"{name}\" 到",
  "filetree.moveDialogCancel": "取消",

  // ── 知识图谱 ──────────────────────────────────────────────────────────
  "graph.titleDrawer": "🕸 笔记关联",
  "graph.titleFull": "🕸 知识图谱",
  "graph.fullView": "全库图谱",
  "graph.fullViewTooltip": "展示整个 vault 的图谱",
  "graph.relayout": "重新布局",
  "graph.relayoutTooltip": "重新计算图谱布局",
  "graph.loading": "加载图谱中…",
  "graph.empty": "暂无可显示的笔记",
  "graph.explicitEdge": "显式链接",
  "graph.semanticEdge": "语义近邻",
  "graph.nodes": "节点",
  "graph.edges": "边",

  // ── 命令面板（FR-SEARCH-01，v0.7）─────────────────────────────────────
  "palette.placeholder": "搜索笔记或执行命令…",
  "palette.sectionCommands": "命令",
  "palette.sectionRecent": "最近打开",
  "palette.sectionNotes": "笔记",
  "palette.noResults": "无匹配结果",
  "palette.newNote": "新建笔记",
  "palette.quickCapture": "快速捕获",
  "palette.voice": "语音输入",
  "palette.chat": "Chat with Vault",
  "palette.graph": "知识图谱",
  "palette.timeline": "时间线",
  "palette.daily": "今日笔记",
  "palette.settings": "设置",
  "palette.tasks": "媒体任务中心",

  // ── 时间线 / 每日笔记 / 标签（FR-SEARCH-05，v0.7）────────────────────
  "app.dailyBtn": "📅 今日",
  "app.dailyTooltip": "打开或创建今日 daily note",
  "app.timelineBtn": "🕘 时间线",
  "timeline.title": "🕘 时间线（最近变更）",
  "timeline.titleTagPrefix": "标签：",
  "timeline.today": "今天",
  "timeline.yesterday": "昨天",
  "timeline.empty": "暂无笔记",
  "tags.section": "标签",
  "tags.empty": "暂无标签",

  // ── 快速捕获浮窗（FR-CAP-01，v0.7）───────────────────────────────────
  "quickCapture.placeholder": "随手记一笔…（Ctrl+Enter 保存）",
  "quickCapture.saved": "已存入今日笔记 ✓",
  "quickCapture.hint": "Ctrl+Enter 保存 · Esc 隐藏",

  // ── 库导入（FR-STORE-06，v0.8）───────────────────────────────────────
  "import.title": "📥 导入外部库",
  "import.hint": "从 Obsidian/Foam/纯 Markdown 目录导入：wikilink 自动转 OKF 链接，图片/音视频归档去重；先预览报告再确认执行。",
  "import.pickBtn": "选择目录…",
  "import.pickDir": "选择要导入的库目录",
  "import.confirm":
    "将导入 {n} 篇笔记、{a} 个资源。\n链接解析：{r} 成功 / {u} 未解析（未解析的保留原文）。\n\n确认执行？",
  "import.warnPrefix": "警告：",
  "import.running": "导入中…",
  "import.done": "已导入 {n} 篇笔记、{a} 个资源（位于 {d}）。",
};
