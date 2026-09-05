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
  "app.templatePrompt": "Template name (empty = blank note):",
  "app.noTemplate": "(blank note)",
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

  // ── Vault management (FR-STORE-01) ────────────────────────────────────
  "vault.title": "Vaults",
  "vault.add": "Add vault…",
  "vault.remove": "Remove",
  "vault.switch": "Switch",
  "vault.removeConfirm": "Remove \"{name}\" from the vault list? (files are NOT deleted)",
  "vault.switchConfirm": "Switch vault and restart the app? Unsaved edits are auto-saved (800ms debounce).",
  "vault.hint": "Switching restarts the app so the index, watcher and MCP server rebind cleanly.",
  "vault.badgeTooltip": "Current vault — click to manage",
  "app.graphBtn": "🕸 Knowledge Graph (Ctrl+G)",
  "app.voiceBtn": "🎤 Voice",
  "app.voiceTooltip": "Voice input → transcript note (Ctrl+Shift+V)",

  // ── Capture ───────────────────────────────────────────────────────────
  "capture.placeholder": "Quick note… (Esc to close, Ctrl+Enter to save)",
  "capture.saving": "Saving…",

  // ── Voice input (FR-CAP-05) ───────────────────────────────────────────
  "voice.title": "🎤 Voice Note",
  "voice.start": "Start recording",
  "voice.holdToRecord": "Hold to record",
  "voice.recording": "● Recording… {sec}s (click to stop)",
  "voice.processing": "Transcribing…",
  "voice.permissionDenied": "Microphone permission denied.",
  "voice.errorTranscribe": "Transcription failed: {msg}",
  "voice.queuedHint": "Recording is longer than the threshold — queued for background transcription. Check ⏳ Tasks.",
  "editor.mediaQueued": "Queued for background transcription (⏳ Tasks).",
  "voice.cloudDownNoLocal": "Cloud transcribe unavailable and no local STT ready. You can download a local model to enable offline fallback.",
  "voice.openSettings": "Open settings to download a model",
  "voice.localSetupHint": "No local model yet. Download one to transcribe offline (automatic fallback when cloud is down) — no restart needed:",

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
  "editor.history": "History",
  "editor.historyTooltip": "Snapshot history & restore",
  "editor.extractActions": "✅ Extract action items",
  "editor.extractBusy": "Extracting…",
  "editor.extractTooltip": "Extract action items from this transcript/meeting note",
  "editor.mediaTranscribing": "Transcribing dropped media…",
  "editor.mediaFailed": "Media transcription failed: ",

  // ── History panel (FR-LLM-09) ─────────────────────────────────────────
  "history.empty": "No snapshots yet. Snapshots are taken before each LLM rewrite.",
  "history.restore": "Restore this version",
  "history.restoring": "Restoring…",
  "history.previewTitle": "Snapshot · {time}",
  "history.close": "Close",

  // ── Action items (FR-LLM-06) ──────────────────────────────────────────
  "actions.extractFailed": "Extraction failed: ",

  // ── Link completion (FR-CAP-03) ───────────────────────────────────────
  "linkComplete.detail": "note",

  // ── Media task center (FR-MEDIA-04) ──────────────────────────────────
  "tasks.openBtn": "⏳ Tasks",
  "tasks.title": "Media Tasks",
  "tasks.empty": "No media tasks yet. Drop an audio/video file into the editor to transcribe it in the background.",
  "tasks.pending": "Queued",
  "tasks.running": "Running",
  "tasks.done": "Done",
  "tasks.failed": "Failed",
  "tasks.cancelled": "Cancelled",
  "tasks.openResult": "Open note",
  "tasks.retry": "Retry",
  "tasks.cancel": "Cancel",
  "tasks.cancelRunningHint": "Cancelling a running task kills its subprocess immediately.",

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
  "settings.transcribeModelHint": "Requires an endpoint with the Whisper-compatible /audio/transcriptions API (e.g. OpenAI, SiliconFlow; Zhipu GLM does not offer one). If your main provider lacks it, add a dedicated one for online STT.",
  "settings.addProvider": "Add OpenAI-compatible provider",
  "settings.removeProvider": "Remove",
  "settings.visionModel": "Vision Model (optional)",
  "settings.visionModelPlaceholder": "e.g. gpt-4o-mini / llava (describes pasted images)",
  "settings.backgroundMedia": "Transcribe all media in the background",
  "settings.backgroundMediaHint": "Off (default): recordings under 60s transcribe inline for instant feedback; longer media is queued. On: everything is queued (check ⏳ Tasks for progress).",

  // ── Data export (FR-STORE-05) ─────────────────────────────────────────
  "data.title": "Data",
  "data.exportZip": "Export vault as ZIP",
  "data.exportDialog": "Export vault to",
  "data.exportDone": "Exported {n} files.",
  "data.gitInit": "Initialize git repository",
  "data.hint": "ZIP excludes derived data (.lmnotes/). git init adds a .gitignore for it and makes an initial commit when git identity is configured.",
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

  // ── Command palette (FR-SEARCH-01, v0.7) ─────────────────────────────
  "palette.placeholder": "Search notes or run a command…",
  "palette.sectionCommands": "Commands",
  "palette.sectionRecent": "Recently opened",
  "palette.sectionNotes": "Notes",
  "palette.noResults": "No matches",
  "palette.newNote": "New note",
  "palette.quickCapture": "Quick capture",
  "palette.voice": "Voice input",
  "palette.chat": "Chat with Vault",
  "palette.graph": "Knowledge graph",
  "palette.timeline": "Timeline",
  "palette.daily": "Today's daily note",
  "palette.settings": "Settings",
  "palette.tasks": "Media task center",

  // ── Timeline / daily note / tags (FR-SEARCH-05, v0.7) ────────────────
  "app.dailyBtn": "📅 Today",
  "app.dailyTooltip": "Open or create today's daily note",
  "app.timelineBtn": "🕘 Timeline",
  "timeline.title": "🕘 Timeline (recent changes)",
  "timeline.titleTagPrefix": "Tag: ",
  "timeline.today": "Today",
  "timeline.yesterday": "Yesterday",
  "timeline.empty": "No notes yet",
  "tags.section": "Tags",
  "tags.empty": "No tags yet",

  // ── Quick capture mini window (FR-CAP-01, v0.7) ───────────────────────
  "quickCapture.placeholder": "Capture a thought… (Ctrl+Enter to save)",
  "quickCapture.saved": "Saved to today's note ✓",
  "quickCapture.hint": "Ctrl+Enter save · Esc hide",
} satisfies Record<string, string>;

export type MessageKey = keyof typeof en;
