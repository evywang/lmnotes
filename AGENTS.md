# AGENTS.md

Workspace instructions for ZCode agents working in **LMNotes** (D:\zcode\LMNotes).
Read this before editing code.

## What this is

Local-first, LLM-native personal knowledge app. Notes are plain Markdown + YAML
frontmatter in the **Open Knowledge Format (OKF)**; an LLM engine continuously
indexes, links, summarizes, and answers questions over the vault. UI is bilingual
(中文 / English). Dual-licensed MIT / Apache-2.0.

## Tech stack & layout

Tauri 2 desktop app. Rust core, SolidJS frontend (NOT React), CodeMirror editor.

```
crates/
  lmnotes-core/   # ALL business logic. No UI deps. Pure `cargo test`.
    src/{okf, backend, index, indexer, llm, qa, search, graph}
  lmnotes-cli/    # Debug / OKF Validator CLI
  lmnotes-mcp/    # Embedded read-only MCP server (loopback only)
apps/desktop/
  src/            # SolidJS frontend: App.tsx + {capture,chat,editor,graph,
                  #   i18n,settings,store,suggestions,components}
  src-tauri/src/  # Tauri shell: commands.rs (IPC), lib.rs (run/wiring), llm_config.rs
docs/             # specs/PRD.md, adr/ADR-0001..0005, okf/, user-manual.md, superpowers/plans/
```

## Commands (match CI exactly)

```bash
# Rust quality gate — run all three before claiming done
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets

# Frontend (from apps/desktop)
cd apps/desktop && npm ci
cd apps/desktop && npx tsc --noEmit && npm run build

# Run desktop app (hot reload)
cd apps/desktop && npm run tauri dev
# Production bundle (.exe/.deb) -> apps/desktop/src-tauri/target/release/bundle/
cd apps/desktop && npm run tauri:build

# CLI
cargo run -p lmnotes-cli -- --help
```

Stable Rust toolchain pinned via `rust-toolchain.toml`. Node ≥ 20. Linux needs
the webkit2gtk/gtk deps listed in README §快速开始 and `.github/workflows/ci.yml`.

## Hard architecture boundary — read ADR-0002

**`lmnotes-core` MUST NOT use `std::fs`.** All file IO goes through the
`StorageBackend` trait. This is enforced by `clippy.toml` `disallowed-methods`
(`std::fs::read`, `std::fs::write`, `File::open`, `File::create`) and fails CI.
The Tauri shell layer (`apps/desktop/src-tauri/src/*.rs`) is the **only** place
exempt — it opens with `#![allow(clippy::disallowed_methods)]` and uses
`tokio::fs` / `std::fs` for the app shell (not core business logic).

When adding a new Tauri command: define it in `commands.rs`, then register it in
`lib.rs` `invoke_handler!` macro list. Tauri capabilities live in
`src-tauri/capabilities/default.json` — adding a new permission/plugin requires
editing it.

## Conventions

- **Commits:** Conventional Commits (`feat:`, `fix:`, `docs:`, `ci:`, `style:`,
  `chore:`). Release is tag-triggered (`git tag vX.Y.Z` → `.github/workflows/release.yml`).
- **Rust:** rustfmt + clippy `-D warnings` are non-negotiable. `clippy.toml`
  forbids raw `std::fs`. Comments are predominantly Chinese — match the file.
- **Line endings:** `.gitattributes` enforces LF. Don't commit CRLF.
- **Frontend:** SolidJS primitives (`createSignal`, `createStore`, `onMount`).
  State in `src/store/{vault,llm}.ts`. UI text MUST go through `src/i18n` with
  both `locales/zh.ts` and `locales/en.ts` entries.
- **OKF note model:** a `Concept` = YAML `frontmatter` + Markdown `body`
  (`crates/lmnotes-core/src/okf/concept.rs`). Don't invent ad-hoc note formats.

## Things to read before touching sensitive areas

- **`docs/specs/PRD.md` §5** — feature catalog. Features are numbered
  `FR-<DOMAIN>-<NN>` with priority `MVP`/`P1`/`P2`. Domains: STORE, CAPTURE,
  MEDIA, LLM, SEARCH. Check whether a requested feature already has an FR-ID.
- **`docs/adr/ADR-0003`** — three-layer index (Tantivy fulltext + sqlite-vec
  vectors + SQLite structured, RRF fusion). Index layer changes start here.
- **`docs/adr/ADR-0005`** — LLM provider guardrails: cloud providers (OpenAI-
  compatible) require explicit per-item auth; sensitive items default to local
  (Ollama). Any LLM call path must respect the guard.
- **`docs/okf/SPEC.v0.1.md`** — the on-disk format. Don't break frontmatter
  field validation (strong-validated; corrupt files are read-only protected).

## Runtime locations & gotchas

- Default vault: `~/.lmnotes/default` (fixed in M1a; UI picker is later).
- Config: `~/.lmnotes/config.json` (LLM providers, MCP, embed dim).
- MCP server is **read-only** and binds `127.0.0.1` only — external agents
  (Claude/Cursor/ZCode) can query but never mutate notes.
- On startup the app reindexes the vault (incremental; skips unchanged) and
  watches for external `.md` edits via `notify` → reindex + regenerate suggestions.
- Boot probes LLM provider health; if none healthy, LLM features silently disable
  (check stderr for the "No healthy LLM provider" warning when debugging).
- Indexes are derived/rebuildable — deleting `.lmnotes/` reindexes cleanly.

## Voice / STT (built)

Voice input / speech-to-text is **FR-CAP-05** + **FR-MEDIA-01** + **FR-MEDIA-05** — now implemented:
- Cloud Whisper (OpenAI-compatible) is the primary path (ADR-0006).
- Local whisper.cpp sidecar is the automatic runtime fallback when cloud is
  unreachable (ADR-0007). `WhisperCppProvider` lives in the Tauri shell
  (`apps/desktop/src-tauri/src/whisper_cpp.rs`), not `lmnotes-core` (it spawns
  subprocesses + writes temp WAVs → ADR-0002 boundary). Models download on-demand
  to `~/.lmnotes/models/`. Runtime fallback logic: `transcribe_with_fallback` in
  `commands.rs` + `Registry::transcribe_candidates` in `routing.rs`.
