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
  "app.graphBtn": "🕸 知识图谱 (Ctrl+G)",
  "app.voiceBtn": "🎤 语音",
  "app.voiceTooltip": "语音输入 → 转录笔记 (Ctrl+Shift+V)",

  // ── Capture ───────────────────────────────────────────────────────────
  "capture.placeholder": "快速记一条…（Esc 关闭，Ctrl+Enter 保存）",
  "capture.saving": "保存中…",

  // ── 语音输入 (FR-CAP-05) ──────────────────────────────────────────────
  "voice.title": "🎤 语音笔记",
  "voice.start": "开始录音",
  "voice.recording": "● 录音中… {sec}s（点击停止）",
  "voice.processing": "转录中…",
  "voice.permissionDenied": "已拒绝麦克风权限。",
  "voice.errorTranscribe": "转录失败：{msg}",
  "voice.cloudDownNoLocal": "云端转录不可用且本地未就绪。可下载一个本地模型，启用离线降级。",
  "voice.openSettings": "打开设置下载模型",

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
};
