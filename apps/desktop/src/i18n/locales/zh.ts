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

  // ── Capture ───────────────────────────────────────────────────────────
  "capture.placeholder": "快速记一条…（Esc 关闭，Ctrl+Enter 保存）",
  "capture.saving": "保存中…",

  // ── Chat drawer ───────────────────────────────────────────────────────
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
};
