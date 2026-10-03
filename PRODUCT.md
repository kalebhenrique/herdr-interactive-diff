# PRODUCT.md

This file provides guidance to agents when working with code in this repository.

## Project

Herdr Interactive Diff (`herdr-interactive-diff`) is a Rust TUI (ratatui + crossterm) that acts as an AI-augmented Git diff viewer and dual-agent pair-programming plugin for Herdr. It connects to Herdr's IPC socket to interact with AI coding agents (Antigravity CLI `agy`, Claude Code `claude`, and 15+ others) in split panes, triggers automated reviews, and annotates diff hunks with AI-generated classifications.

## Commands

```bash
cargo build                # debug build
cargo run                  # run against current directory's repo
cargo run -- --demo        # run with mock diff/classification data (no real agents/CLIs needed for data)
cargo run -- -p ~/some/repo
cargo test                 # run all tests
cargo test test_parse_demo_diff       # run a single test by name
cargo test extract_classifications    # substring matches multiple tests
cargo clippy               # lint
```

## Architecture

Single binary, no CLI framework (hand-rolled arg parsing in `main.rs`). One tokio runtime drives background work; the UI loop is synchronous with non-blocking `event::poll(30ms)`.

### Event loop and concurrency (main.rs)

- Background tasks (AI classification, diff watcher) communicate with the UI via `mpsc::unbounded_channel<AsyncAction>` (`ClassificationReady`, `DiffUpdated`), drained with `try_recv` each frame.
- Every 150ms the loop calls `app.update_herdr_state()`, `app.sync_claude_review_to_diff()`, and `app.check_artifacts_update()` — this polling is how Claude Code's terminal output gets scraped and turned into diff annotations.
- All keyboard/mouse handling lives in `main.rs`; `ui.rs` is pure rendering. Mouse hit-testing depends on hardcoded layout widths (diff tree drawer = 34 cols, artifact tree = 36 cols, top bar = 3 rows) — changing layout in `ui.rs` requires updating the hit tests in `main.rs`.

### State (app.rs)

`App` is the single mutable state container. Tests never spawn real agents or connect to Herdr — this is guarded by `cfg!(test)` checks in `App::new`, `launch_or_trigger_claude`, and `trigger_claude_review`. Preserve these guards when touching session-spawning code.

### Diff extraction and the hunk ID contract (git.rs)

`GitDiff::from_local_repo` tries fallbacks in order: `git diff HEAD` → `git diff` → `git diff --cached` → untracked files (via `git diff --no-index /dev/null <file>`) → branch vs main/master → `HEAD~1`.

The custom unified-diff parser produces `DiffFile`/`DiffHunk`. **Hunk IDs (`"{file_path}#{n}"`, e.g. `src/auth/session.rs#0`) are the join key** between the diff, AI classifications (`HashMap<String, HunkClassification>`), persisted sessions, and prompts sent to agents. Changing the ID format breaks classification persistence.

### AI pipeline (ai_engine.rs)

- Classification levels: `forte` (critical → "caveman" review tooltip), `media`, `curiosidade` (hint-only), `normal`.
- `build_claude_review_prompt` produces a 5-lens review prompt (security, readability, edge_cases, maintainability, architecture) that demands a trailing ```json block of classifications.
- `extract_classifications_from_review` parses that JSON, falling back to a resilient heuristic parser for `**[HIGH] [lens] [file:line] Title**` / `Why:` / `Fix:` markdown lines.
- One-shot CLI invocations use `agy -p` / `claude -p` (`classify_diff_with_antigravity`, `pipeline.rs`); interactive agent runs go through PTY sessions.

### Terminal sessions (terminal_session.rs)

PTY via `portable-pty`, screen state via `vt100` parser (10k scrollback), rendered with `tui-term`. A background reader thread per session feeds the parser. Prompts are injected with **bracketed paste mode** (`paste_command`), not plain typing, so multi-line prompts survive agent input handling. `read_screen_text()` is the screen-scraping primitive everything else (review sync, Herdr state detection, artifact detection) builds on.

### Herdr integration (herdr.rs) — optional

Talks to the Herdr workspace manager over a Unix socket (JSON-RPC per request; `~/.config/herdr/herdr.sock`, overridable via `HERDR_SOCKET_PATH`; pane discovery via `HERDR_PANE_ID`). Agent state (idle/working/blocked) is **inferred from terminal screen text** — spinner glyphs and permission-prompt phrases in `detect_session_state`. If you change agent CLI output handling, check these heuristics still match. All socket calls fail silently (return `Option`), so Herdr absence is always safe.

### Artifacts (markdown_renderer.rs)

Discovers Antigravity session artifacts (.md) from `~/.gemini/antigravity-cli/brain/<conversation_id>/`. The active conversation is found by querying the SQLite DB at `~/.gemini/antigravity-cli/conversation_summaries.db` **through the `sqlite3` CLI binary** (shelling out, not a Rust sqlite dep), then falling back to presence locks and brain dirs. The markdown renderer mimics render-markdown.nvim glyphs (Nerd Fonts icons) and is width-responsive.

### Session persistence (session_store.rs)

Reviews and pointwise questions (`?` key) persist per-repo as JSON under `~/.config/herdr-interactive-diff/sessions/` (or Herdr plugin state dir). Timestamps are hand-rolled (`current_timestamp_str`) — no chrono dep; don't casually introduce one.

## Conventions

- Comments and error strings are mixed PT-BR/English; UI-facing strings are English. Follow the surrounding file.
- `--demo` mode (`GitDiff::demo()` + `ClassificationResponse::demo()`) is the fixture set for many tests — keep demo data consistent with what tests assert (hunk IDs, complexity levels, tooltip text like "PONT NO CHECK").
- `pipeline.rs` is an older, simpler one-shot review path kept alongside the PTY-based flow; prefer the PTY/`ai_engine.rs` path for new work.
- Warnings are not denied; some modules use `#![allow(dead_code)]`. Don't remove those pragmas while refactoring public-ish helpers.
