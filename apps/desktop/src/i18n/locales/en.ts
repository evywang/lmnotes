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
  // ── Common dialogs (app-name titled) ─────────────────────────────────
  "dialog.ok": "OK",
  "dialog.cancel": "Cancel",
  "app.graphBtn": "🕸 Knowledge Graph (Ctrl+G)",
  "app.voiceBtn": "🎤 Voice",
  "app.voiceTooltip": "Voice input → transcript note (Ctrl+Shift+V)",

  // ── Capture ───────────────────────────────────────────────────────────
  "capture.placeholder": "Quick note… (Esc to close, Ctrl+Enter to save)",
  "capture.saving": "Saving…",

  // ── Voice input (FR-CAP-05) ───────────────────────────────────────────
  "voice.title": "🎤 Voice Note",
  "voice.start": "Start recording",
  "voice.recording": "● Recording… {sec}s (click to stop)",
  "voice.processing": "Transcribing…",
  "voice.permissionDenied": "Microphone permission denied.",
  "voice.errorTranscribe": "Transcription failed: {msg}",
  "voice.cloudDownNoLocal": "Cloud transcribe unavailable and no local STT ready. You can download a local model to enable offline fallback.",
  "voice.openSettings": "Open settings to download a model",

  // ── Local STT (whisper.cpp fallback, ADR-0007) ────────────────────────
  "localStt.title": "🎙️ Local STT (offline fallback)",
  "localStt.description": "When the cloud Whisper endpoint is unreachable, transcription automatically falls back to a local whisper.cpp engine. Download a model to enable it.",
  "localStt.binaryOk": "whisper.cpp engine: ready",
  "localStt.binaryMissing": "whisper.cpp engine: not found (bundled in the installer)",
  "localStt.ffmpegOk": "ffmpeg (audio transcoding): ready",
  "localStt.ffmpegMissing": "ffmpeg (audio transcoding): not found",
  "localStt.models": "Models",
  "localStt.download": "Download",
  "localStt.downloading": "Downloading… {downloaded} / {total} MB",
  "localStt.downloadingIndeterminate": "Downloading… {downloaded} MB",
  "localStt.downloadFailed": "Download failed: {msg}",
  "localStt.noModelsDownloaded": "No models downloaded yet.",
  "localStt.modelNote.base": "Smallest and fastest; decent English, mediocre Chinese. Recommended first try.",
  "localStt.modelNote.small": "Best quality/speed balance for Chinese and English; recommended for daily notes.",
  "localStt.modelNote.medium": "Good Chinese quality but slow and large; for long recordings.",

  // ── Chat drawer ───────────────────────────────────────────────────────────────
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
  "settings.transcribeModel": "Transcribe Model (optional)",
  "settings.transcribeModelPlaceholder": "e.g. whisper-1 (enables voice input)",
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

  // ── Knowledge graph ──────────────────────────────────────────────────
  "graph.titleDrawer": "🕸 Note Neighborhood",
  "graph.titleFull": "🕸 Knowledge Graph",
  "graph.fullView": "Full graph",
  "graph.fullViewTooltip": "Show the whole vault graph",
  "graph.relayout": "Re-layout",
  "graph.relayoutTooltip": "Recompute graph layout",
  "graph.loading": "Loading graph…",
  "graph.empty": "No notes to graph yet",
  "graph.explicitEdge": "Explicit link",
  "graph.semanticEdge": "Semantic neighbor",
  "graph.nodes": "nodes",
  "graph.edges": "edges",
} satisfies Record<string, string>;

export type MessageKey = keyof typeof en;
