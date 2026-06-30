/**
 * English message dictionary — the source of truth for message keys.
 * Add a key here first, then add it to zh.ts (the type `MessageKey` is derived
 * from this object, so zh.ts gets compile-time enforcement to match keys).
 */
export const en = {
  // ── App shell ─────────────────────────────────────────────────────────
  "app.searchPlaceholder": "Search… (Enter)",
  "app.newNoteTitle": "New note",
  "app.noteTitlePrompt": "Note title:",
  "app.newNoteBtn": "+ New",
  "app.newNoteTooltip": "New note",
  "app.importBtn": "📥 Import",
  "app.importTooltip": "Import .md file",
  "app.importFilterName": "Documents",
  "app.chatBtn": "💬 Chat with Vault (Ctrl+J)",
  "app.searching": "Searching…",
  "app.searchHint": "Type keywords to search notes",
  "app.files": "📁 Files",
  "app.placeholder": "Select a note on the left or search",
  "app.suggestionCenter": "Suggestion Center",
  "app.settingsTooltip": "Provider Settings (Ctrl+,)",

  // ── Capture ───────────────────────────────────────────────────────────
  "capture.placeholder": "Quick note… (Esc to close, Ctrl+Enter to save)",
  "capture.saving": "Saving…",

  // ── Chat drawer ───────────────────────────────────────────────────────
  "chat.title": "💬 Chat with Vault",
  "chat.clearTooltip": "Clear history",
  "chat.clear": "Clear",
  "chat.emptyPrompt": "Ask me anything about your notes…",
  "chat.emptyExample": "e.g. What is the formula for the attention mechanism?",
  "chat.inputPlaceholder": "Ask a question… (Enter to send, Shift+Enter for newline)",

  // ── Editor ────────────────────────────────────────────────────────────
  "editor.toggleTooltip": "Toggle edit/preview",
  "editor.edit": "✏️ Edit",
  "editor.preview": "👁 Preview",
  "editor.loading": "Loading…",

  // ── Rewrite menu ──────────────────────────────────────────────────────
  "rewrite.polish": "Polish",
  "rewrite.expand": "Expand",
  "rewrite.translate": "Translate to English",
  "rewrite.summarize": "Summarize key points",
  "rewrite.busy": "Rewriting…",

  // ── Provider settings ─────────────────────────────────────────────────
  "settings.title": "Provider Settings",
  "settings.loading": "Loading…",
  "settings.ollamaLocal": "Ollama (local)",
  "settings.openaiCompat": "OpenAI-compatible: {id}",
  "settings.health": "Health",
  "settings.healthy": "Reachable",
  "settings.unhealthy": "Unreachable",
  "settings.reprobe": "Re-probe",
  "settings.cloudAllowed": "Allow cloud providers (off by default, local first)",
  "settings.saving": "Saving…",
  "settings.save": "Save",
  "settings.cancel": "Cancel",
  "settings.restartHint":
    "Restart the app after saving for changes to take effect. Default config points to local Ollama (localhost:11434).",
  // ── Language toggle (this settings section) ──────────────────────────
  "settings.language": "Language",
  "settings.languageZh": "中文",
  "settings.languageEn": "English",
  "settings.generalSection": "General",

  // ── Suggestion center ─────────────────────────────────────────────────
  "suggestion.empty": "No pending suggestions",
  "suggestion.acceptTooltip": "Accept (Enter)",
  "suggestion.rejectTooltip": "Reject",

  // ── File tree ─────────────────────────────────────────────────────────
  "filetree.empty": "No notes yet",
  "filetree.newNoteTooltip": "New note",
  "filetree.newFolderTooltip": "New folder",
  "filetree.deleteTooltip": "Delete",
  "filetree.ctxNewNote": "📄 New note",
  "filetree.ctxNewFolder": "📁 New folder",
  "filetree.ctxOpen": "📄 Open",
  "filetree.ctxDelete": "🗑 Delete",
  "filetree.ctxMove": "✂️ Move to…",
  "filetree.ctxReveal": "🖥 Reveal in file manager",
  "filetree.deleteConfirm": 'Delete "{name}"? This cannot be undone.',
  "filetree.deleteFailed": "Delete failed: ",
  "filetree.createFailed": "Create failed: ",
  "filetree.folderNamePrompt": "Folder name:",
  "filetree.folderNameDefault": "new-folder",
  "filetree.openFailed": "Open failed: ",
  "filetree.moveFailed": "Move failed: ",
  "filetree.noMoveTarget": "No folder to move to",
  "filetree.moveDialogTitle": 'Move "{name}" to',
  "filetree.moveDialogCancel": "Cancel",
} satisfies Record<string, string>;

export type MessageKey = keyof typeof en;
