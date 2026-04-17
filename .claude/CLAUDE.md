# Ansambel — Claude Code Instructions

Tauri v2 + Svelte 5 + Bun desktop app that orchestrates parallel Claude Code
agents across git worktrees. Cross-platform (Windows + Linux, macOS
nice-to-have). Full design spec in
`docs/superpowers/specs/2026-04-17-ansambel-design.md`.

---

## Hard rules

### Rust

- Every `#[tauri::command]` returns `Result<T, String>` — never panic, never
  unwrap in command handlers.
- No `.unwrap()` or `.expect()` outside tests.
- All shared state through `Arc<Mutex<_>>` in Tauri managed state — separate
  state types may be registered when isolation from AppState locking is required
  (LspServerPool, etc). No globals, no `lazy_static`.
- Mutex discipline: acquire lock, extract data, drop lock before any blocking /
  async / spawn work.
- PTY reader threads handle EOF/errors gracefully, emit `agent-status` event on
  exit.
- `portable-pty`: always close the slave end in parent after spawning child.
- Spawn `claude` with explicit env — inject `GH_TOKEN` per-process, never rely
  on ambient shell state.
- Agent processes use `--permission-mode bypassPermissions` with
  `--disallowedTools EnterWorktree,ExitWorktree` — never
  `--dangerously-skip-permissions`.
- `detect_default_branch` resolves only from remote tracking refs
  (`origin/HEAD`, `origin/main`, `origin/master`) — never falls back to local
  refs or HEAD.
- Never call `gh auth switch` globally — use `gh auth token --user <profile>`
  and inject per-process.
- Git operations return descriptive errors, not generic ones.

### Frontend

- PTY output never touches Svelte state — xterm.js owns its buffer.
- Messages use `SvelteMap<id, Message>`, mutated in place — never replace entire
  arrays.
- xterm instances use `display: none / block` on workspace switch — never
  mount/unmount.
- Tauri Channel API for binary streams — never `listen()` + JSON for
  high-frequency data.
- All `invoke()` calls wrapped in try/catch with user-visible error handling
  (enforced by convention in `src/lib/ipc.ts` typed wrappers).
- Tooltips use the `tooltip` Svelte action (`use:tooltip={{ text, shortcut? }}`)
  appended to `document.body` — never native `title` attributes.
- Token input counts sum all three sources (`input_tokens` +
  `cache_creation_input_tokens` + `cache_read_input_tokens`) — never
  `input_tokens` alone.

### Data

- All app data under the OS-resolved app-data dir
  (`Tauri app.path().app_data_dir()`) — zero writes to managed repos.
- Worktrees: `<data_dir>/workspaces/<workspace-id>/`
- Messages: `<data_dir>/messages/<workspace-id>.json`
- Metadata: `<data_dir>/workspaces.json`, `<data_dir>/sessions.json`,
  `<data_dir>/repos.json`
- Atomic writes via `.tmp` + rename. Debounced writes for messages/workspaces/
  sessions at 500ms; immediate for app_settings and repo/provider config.
- Workspace status resets from `Running` to `Waiting` on app restart (agent
  process is dead after restart).

### Testing (hard rule — per project feedback)

- No production code without a failing test first. TDD: red → green → refactor.
- Every `#[tauri::command]` has ≥1 unit test + ≥1 integration test.
- Every Svelte component has ≥1 test (happy path + ≥1 edge case).
- Every phase ships with E2E tests covering its golden path.
- External services (Claude CLI, Jira, Lark) are mocked in unit/integration
  tests.
- E2E tests use real Tauri window via Playwright; Claude CLI mocked via
  `ANSAMBEL_MOCK_CLAUDE=1`.
- CI fails if coverage drops below **95%** on changed files (both unit-test
  line+branch+function coverage and E2E scenario coverage of documented golden
  paths).
- Never use `#[ignore]` or `test.skip` without a linked GitHub issue.

### Commands

- Use `bun`, not `npm`, `npx`, or `yarn`.
- Type check: `bun run check`.
- Rust check: `cargo check` (never `cargo build` or `tauri build` in checks).

### Linting & formatting

- Lint all: `bun run lint` (ESLint + Prettier check)
- Auto-fix: `bun run lint:fix`
- Format only: `bun run format` / `bun run format:check`
- Rust fmt: `cd src-tauri && cargo fmt --all -- --check`
- Rust clippy: `cd src-tauri && cargo clippy --lib --all-targets -- -D warnings`
- Git hooks are installed automatically by `bun install` (`prepare` script).
  - `pre-commit`: lint-staged (TS/Svelte/MD) + `cargo fmt --check`
  - `commit-msg`: `commitlint` enforces conventional commits
    (feat/chore/fix/ci/docs/test/refactor/perf/style/build/revert)
  - `pre-push`: full clippy with `-D warnings` + unit tests + `bun run check`
- CI will re-run the same gates and a final authoritative check.

### General

- No `console.log` in production paths — use `tracing` in Rust and the
  `src/lib/logging.ts` wrapper in Svelte (added in Phase 1).
- No hardcoded paths — derive from repo root or Tauri app data dir via
  `platform::paths::*` helpers.
- Async filesystem/process ops must have timeouts.

---

## Architecture

See `docs/superpowers/specs/2026-04-17-ansambel-design.md` for the full
architecture. Key modules:

- `src-tauri/src/platform/` — cross-platform abstractions (paths, binary, PTY,
  keyring, shell).
- `src-tauri/src/commands/` — Tauri IPC handlers, one file per subsystem.
- `src-tauri/src/task_provider/` — Jira and Lark Bitable plugins (Phase 3).
- `src-tauri/src/persistence/` — atomic and debounced JSON writers.
- `src/lib/ipc.ts` — typed invoke wrappers.
- `src/lib/stores/` — Svelte 5 runes state (added in Phase 1).
- `src/lib/components/` — grouped by surface (kanban/, workspace/, chat/,
  knowledge/).

---

## What not to build (without explicit instruction)

- Codex support (Claude CLI only).
- Checkpoint / restore of Claude conversation history.
- Multi-repo open simultaneously.
- Public / marketplace release (private license).
- GitLab / Bitbucket git providers.
- Linear / Notion / Asana task providers (architecture ready, not shipped).
- Remote / multi-machine agent orchestration.
- Mobile / web versions.

---

## Build strategy

9 phases, each with its own implementation plan under `docs/superpowers/plans/`.
Work on one phase at a time. Phase 0 establishes this foundation; Phase 1 is the
MVP orchestrator.
