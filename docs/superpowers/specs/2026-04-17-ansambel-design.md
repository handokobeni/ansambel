---
title: Ansambel — Design Spec
author: Handoko Beni <handoko.ben@talentlytica.net>
date: 2026-04-17
status: draft
license: private (Talentlytica)
tags: [design, spec, ansambel, tauri, svelte, rust]
---

# Ansambel — Design Specification

> **Orchestrate your AI ensemble.**
>
> A cross-platform (Windows + Linux + macOS) desktop application that orchestrates
> multiple Claude Code agents in isolated git worktrees. Modeled after [korlap]
> (macOS-only) and [Conductor] — built from scratch with clean cross-platform
> foundations and extended with Jira + Lark Bitable task providers.
>
> [korlap]: https://github.com/ariaghora/korlap
> [Conductor]: https://www.conductor.build

## Executive summary

| | |
|---|---|
| **Name** | Ansambel (Indonesian for "ensemble") |
| **Bundle ID** | `com.talentlytica.ansambel` |
| **Binary name** | `ansambel` |
| **Stack** | Tauri v2 + Rust + Svelte 5 + Bun + Tailwind v4 |
| **Platforms** | Windows (primary) + Linux (primary) + macOS (nice-to-have) |
| **AI provider** | Claude Code CLI only |
| **Git provider** | GitHub only (via `gh` CLI) |
| **Task providers** | Jira Cloud + Lark Bitable (pluggable) |
| **License** | Private (Talentlytica) |
| **Target effort** | 33–36 weeks, 9 phases, solo engineer |
| **Testing** | TDD mandatory, 95% unit + e2e coverage gate |

---

# 1. Identity & Paths

## Product identity

| Aspect | Value |
|---|---|
| Product name | Ansambel |
| Bundle identifier | `com.talentlytica.ansambel` |
| Window title | (empty — identity via aesthetic, per korlap convention) |
| Tagline | "Orchestrate your AI ensemble" |
| Binary name | `ansambel` (lowercase) |

## Data directory per OS (multi-user aware)

Resolved via Tauri `app.path().app_data_dir()` — automatically per-OS-user scoped:

| OS | Path |
|---|---|
| **Windows** | `%APPDATA%\com.talentlytica.ansambel\` → `C:\Users\<user>\AppData\Roaming\com.talentlytica.ansambel\` |
| **Linux** | `$XDG_DATA_HOME/com.talentlytica.ansambel/` → `~/.local/share/com.talentlytica.ansambel/` |
| **macOS** | `~/Library/Application Support/com.talentlytica.ansambel/` |

## Data layout

```
<app_data_dir>/
├── app_settings.json
├── repos.json
├── workspaces.json
├── sessions.json
├── context_meta.json
├── task_providers_cache.json
├── .ansambel.lock
├── workspaces/<workspace-id>/           # actual git worktree
├── contexts/<repo-id>/
│   ├── invariants.md
│   ├── facts.md
│   ├── context.md
│   ├── contradictions.md
│   ├── index.md
│   └── hot.md
├── messages/<workspace-id>.json
├── todos/<workspace-id>.json
├── autopilot_log/<workspace-id>.json
├── images/<workspace-id>/
└── logs/
    ├── ansambel.log
    └── crashes/
```

**Zero writes to user repos.** Worktrees live under `<app_data>/workspaces/`; the
managed repo is only read from and occasionally fetched/pushed via `git`.

## Credential storage

Credentials (API tokens, app secrets, GH tokens) use the `keyring` crate:

| OS | Backend |
|---|---|
| Windows | Credential Manager |
| Linux | Secret Service (GNOME/KDE) with AES-GCM encrypted file fallback (for headless) |
| macOS | Keychain |

Linux headless fallback uses passphrase from `ANSAMBEL_PASSPHRASE` env var or
one-time-per-session prompt.

---

# 2. Phasing plan

9 phases, each ships a releasable artifact. Serial dependency: 0 → 1 → 2; after
that, order is flexible.

## Overview

| # | Phase | Effort | Shipping value |
|---|---|---|---|
| 0 | Foundation | 1 wk | Scaffold + CI build matrix across 3 OS |
| 1 | MVP Orchestrator | 4–5 wk | One Claude agent in worktree, chat, minimal kanban — first usable product |
| 2 | Work Mode Complete | 3 wk | Diff, terminal, file browser, editor, search, scripts |
| 3 | Task Providers | 2 wk | Jira + Lark Bitable with pluggable abstraction |
| 4 | AI Productivity | 4 wk | Review flow, commit gen, suggest replies, EditDiffBlock, TodoListBlock, AskUserQuestion |
| 5 | Knowledge Base | 4 wk | Invariants/Facts/Context/Contradictions + file affinity + pre-check |
| 6 | LSP + MCP | 4 wk | LSP server pool + built-in MCP + 3rd-party MCP config |
| 7 | Autopilot & Staging | 4 wk | Autopilot orchestrator, staging workspace, multi-PR, polling |
| 8 | Polish & Ship | 3 wk | Dependency graph, shortcuts, installers signed across 3 OS |

**Total:** ~33–36 weeks solo engineer (accounting for 95% test coverage overhead).

## Phase details

### Phase 0 — Foundation (1 wk)

**Scope.**
- `bun create tauri-app` + Svelte 5 + Tailwind v4 + TypeScript strict
- Tauri IPC typed wrappers (`ipc.ts`) + error handling convention
- Cross-platform path utilities
- Theme system (CSS vars + palette structure; no UI picker yet)
- GitHub Actions CI build matrix: `.msi` / `.AppImage` / `.dmg` + tests in 3 OSes
- Logging setup (`tracing` Rust, console frontend)
- Project-level `.claude/CLAUDE.md` with hard rules for the agent

**Ship.** Empty window launches in 3 OSes.

### Phase 1 — MVP Orchestrator (4–5 wk)

**Scope.**
- Repo management (add via folder picker, list, remove, bind gh profile)
- Workspace creation: `git worktree add` → `<app_data>/workspaces/<id>/`
- Workspace sidebar + status dot (running / waiting)
- Simple kanban (Todo / In Progress / Review / Done) with drag-drop
- Chat panel spawning `claude -p --output-format stream-json --verbose`
- NDJSON parser in Rust → Tauri Channel → Svelte render
- `SvelteMap<wsId, SvelteMap<msgId, Message>>` (reactive performance)
- Message input with basic send (no @-mentions yet)
- Persistence: `repos.json`, `workspaces.json`, `messages/<wsId>.json` (500ms debounced)
- Titlebar with Plan/Work mode toggle (⌘1/⌘2)
- 5 baseline shortcuts

**Ship.** Single dev can use it for real orchestration work.

**Known risks.**
- PTY cross-platform (ConPTY on Windows)
- Claude CLI binary location varies per OS (fallback detection logic)
- gh CLI on Windows (PATH variations)

### Phase 2 — Work Mode Complete (3 wk)

**Scope.** Diff viewer (syntax highlighted) · xterm.js terminal · File browser tree · CodeMirror 6 editor · Script runner (per-repo) · Search modal (files + content grep) · @-file mentions with autocomplete.

**Risks.** Font rendering on WebKitGTK (Linux) requires testing.

### Phase 3 — Task Providers (2 wk)

**Scope.** `TaskProvider` trait + registry · `JiraProvider` (Cloud REST v3 + API token) · `LarkBitableProvider` (tenant token + Bitable API) · JSON-Schema driven config UI · Import popover · Field mapping per provider · Deep-links back to Jira/Lark.

**Risks.** Lark Bitable field type variance (single/multi select, user, formula) requires a normalizer.

### Phase 4 — AI Productivity (4 wk)

**Scope.** Review flow (Opus-powered diff review, clean/issues classification) · Commit message generator · Suggest replies · Prioritize todos · Determine dependencies · AskUserQuestion tool · EditDiffBlock inline accept/reject · TodoListBlock in-chat checklist synced to kanban.

**Risks.** Cost management — many extra agent spawns. Add per-feature disable toggles.

### Phase 5 — Knowledge Base (4 wk)

**Scope.** Builder agent with 3-phase prompt (recon / synthesize / cleanup) + size-aware constraints (SMALL <500 / MEDIUM 500–5000 / LARGE >5000 files) · 4 markdown outputs (invariants, facts, context, contradictions) + index + hot.md · UI 4-tab CRUD · File affinity extraction and injection · Invariant pre-check agent · Contradiction resolution workflow · Incremental update on PR merge.

**Risks.** Prompt engineering is heavy — requires testing across repo sizes and styles.

### Phase 6 — LSP + MCP (4 wk)

**Scope.** LSP server pool + auto-detect (TypeScript, Rust, Python, Go defaults) · 6 LSP commands (hover, goto-def, references, rename, symbols, diagnostics) · Built-in MCP server (random port, HTTP, token-auth) · MCP tools exposed: workspace info, todos CRUD, LSP queries, notify · 3rd-party MCP config (stdio + SSE transport) · MCP OAuth flow.

**Risks.** LSP stdin/stdout on Windows needs careful PTY-like handling. OAuth from desktop needs loopback server pattern.

### Phase 7 — Autopilot & Staging (4 wk)

**Scope.** Staging workspace (isolated test/build) · Autopilot orchestrator with 11-type event stream · PR status polling (5s) + base branch polling (60s) · Multi-PR workspace (source_prs) · Combo PR checkout popover · Auto-answer agent questions within safety bounds.

**Risks.** Edge cases in autopilot are numerous. Provide semi-autopilot mode (pause on uncertainty).

### Phase 8 — Polish & Ship (3 wk)

**Scope.** Dependency graph visualization · Virtual scrolling optimization · System resource monitor · Complete keyboard shortcut set · Toast notifications · ≥3 theme palettes · Code signing (Windows signtool, macOS notarize, Linux GPG) · Auto-updater via tauri-plugin-updater.

**Risks.** Signing certificates require budget (Windows ~$300/yr, Apple Dev $99/yr).

## Ship gates

| Target version | Ship at | Status |
|---|---|---|
| v0.1 | End of Phase 1 | Internal testing |
| v0.5 | End of Phase 4 | Competitive with korlap (no KB yet) |
| v1.0 | End of Phase 8 | Full feature parity + cross-platform + Lark |

---

# 2.5 Testing strategy

## Core discipline

**Red → Green → Refactor.** No production code without a failing test first.

**Coverage gate: 95%** on both unit test line coverage and e2e scenario coverage.
CI blocks merge if either falls below threshold. The `superpowers:test-driven-development`
skill will be invoked during every implementation phase and followed rigidly.

## Stack

| Layer | Tool | Why |
|---|---|---|
| Rust unit | `cargo test` inline `#[cfg(test)]` | Standard, fast |
| Rust async | `tokio::test` | Async handlers |
| Rust mock | `mockall` | External APIs, providers |
| Rust integration | `cargo test --test <name>` in `tests/` | Command handlers end-to-end |
| Svelte unit | Vitest + `@testing-library/svelte` | Native Svelte 5 runes support |
| IPC contract | Typed helpers via `ipc.ts` | Keep Rust↔Svelte signatures synced |
| E2E | Playwright (with Tauri driver) | Cross-platform, already proven in korlap |
| Coverage | `cargo llvm-cov` + Vitest v8 coverage | Branch + line metrics |

## File layout

```
src-tauri/src/
├── commands/workspace.rs              # #[cfg(test)] mod tests inline
├── task_provider/jira.rs              # #[cfg(test)] mod tests inline
└── tests/                             # integration tests
    ├── workspace_lifecycle.rs
    └── task_import.rs

src/lib/components/
├── ChatPanel.svelte
└── ChatPanel.test.ts                  # Vitest + testing-library

tests/e2e/
├── fixtures/mock-repo/                # small committed git repo
├── helpers/
│   ├── mock-claude.ts                 # deterministic CLI fixture
│   └── tauri-driver.ts
├── phase-1/
│   ├── repo-management.spec.ts
│   ├── workspace-creation.spec.ts
│   ├── chat-flow.spec.ts
│   └── kanban-drag.spec.ts
└── phase-N/…
```

## Rules (to be committed to `.claude/CLAUDE.md`)

- No production code without a failing test first
- Every `#[tauri::command]` has ≥1 unit test + ≥1 integration test
- Every Svelte component has ≥1 test (happy path + ≥1 edge case)
- Every phase ships with e2e tests covering its golden path
- External services (Claude CLI, Jira, Lark) are mocked in unit/integration tests
- E2E tests use real Tauri window via Playwright; CLI mocked via `ANSAMBEL_MOCK_CLAUDE=1`
- CI fails if coverage drops below 95% on changed files (ratchet)
- Never use `#[ignore]` or `test.skip` without a linked GitHub issue

## Mock Claude CLI

A small Rust binary fixture compiled in tests/dev that reads stdin, emits
deterministic stream-json to stdout, exits 0. Tests point at it via
`ANSAMBEL_CLAUDE_CLI_PATH=<fixture>`.

## E2E golden paths per phase

| Phase | Scenario |
|---|---|
| 1 | Add repo → create workspace → send message → see agent reply |
| 2 | Open diff tab → see highlighting → open terminal → run `ls` |
| 3 | Configure Jira → import 3 issues → see kanban cards with deep-link |
| 4 | Trigger review → see Opus verdict (clean/issues) |
| 5 | Click Rebuild → build completes → invariants/facts tabs populated |
| 6 | Hover symbol → LSP tooltip → agent calls MCP tool |
| 7 | Enable autopilot → task cycles Todo→Progress→Review→Done |
| 8 | Installer on fresh VM → app launches → creates repo |

## CI pipeline

```yaml
# Matrix: ubuntu-22.04, windows-2022 (macos-14 nightly + release only)
- cargo check --all-targets
- cargo llvm-cov --fail-under-lines 95 --fail-under-branches 95
- bun run check
- vitest run --coverage.thresholds.lines=95 --coverage.thresholds.branches=95
- tauri build --debug          # smoke
- playwright install
- playwright test --project=${{ matrix.os }}
```

---

# 3. High-level architecture

## Bird's-eye view

```
┌─────────────────────────────────────────────────────────────────────┐
│                     ANSAMBEL — Desktop App (Tauri)                  │
│                                                                     │
│  ┌───────────────────────────────┐    ┌───────────────────────────┐ │
│  │   FRONTEND (Svelte 5 + TS)    │    │   BACKEND (Rust + Tokio)  │ │
│  │                               │IPC │                           │ │
│  │  routes/                      │◄──►│  commands/                │ │
│  │  components/ (Svelte 5)       │    │  task_provider/ (trait)   │ │
│  │  stores/ (runes-based)        │    │  platform/ (cross-OS)     │ │
│  │  ipc.ts (typed wrappers)      │    │  lsp/ + mcp/ + git/       │ │
│  │                               │    │  state.rs (AppState)      │ │
│  └───────────────────────────────┘    └───────────────────────────┘ │
│                                                    │                │
│                                                    ▼                │
│                                   ┌────────────────────────────┐    │
│                                   │   External processes       │    │
│                                   │   • claude CLI             │    │
│                                   │   • gh CLI                 │    │
│                                   │   • git CLI                │    │
│                                   │   • LSP servers            │    │
│                                   │   • PTY (portable-pty)     │    │
│                                   └────────────────────────────┘    │
│                                                    │                │
│         ┌──────────────────────────────────────────┼────────┐       │
│         ▼                     ▼                    ▼        ▼       │
│   ┌─────────────┐      ┌─────────────┐     ┌───────────┐ ┌───────┐  │
│   │ MCP Server  │      │ Jira API    │     │ Lark API  │ │ ... │    │
│   │ (built-in,  │      │ (handoko-   │     │ (Bitable) │ │       │  │
│   │  :random)   │      │  ben…)      │     │           │ │       │  │
│   └─────────────┘      └─────────────┘     └───────────┘ └───────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Backend Rust modules

```
src-tauri/src/
├── main.rs                      # bootstrap
├── lib.rs                       # Tauri builder: state init, command registration
├── state.rs                     # AppState + sub-states, Arc<Mutex<T>> managed
├── error.rs                     # AppError + Result<T> alias
├── logging.rs                   # tracing init
│
├── platform/                    # CROSS-PLATFORM abstractions
│   ├── paths.rs                 # data_dir, worktree_dir, binary_detect
│   ├── shell.rs                 # default shell detection
│   ├── pty.rs                   # portable-pty wrapper
│   ├── keyring.rs               # OS keyring abstraction
│   └── open.rs                  # open file/url
│
├── commands/                    # Tauri IPC commands
│   ├── repo.rs
│   ├── workspace.rs
│   ├── agent.rs
│   ├── git.rs
│   ├── github.rs
│   ├── task.rs
│   ├── files.rs
│   ├── terminal.rs
│   ├── scripts.rs
│   ├── context.rs
│   ├── lsp.rs
│   ├── system.rs
│   └── persistence.rs
│
├── task_provider/               # Jira, Lark, future providers
│   ├── mod.rs                   # trait TaskProvider, registry
│   ├── types.rs
│   ├── jira.rs
│   ├── lark.rs
│   └── schema.rs                # JSON schema for UI forms
│
├── git_provider/                # GitHub via gh CLI
├── lsp/                         # Language Server Protocol pool + protocol
├── mcp/                         # built-in server + 3rd-party client
├── context/                     # knowledge base builder/checker
├── autopilot/                   # autopilot orchestration
└── testing/                     # #[cfg(test)] fixtures + mocks
```

## Frontend Svelte modules

```
src/
├── app.html
├── app.css                      # Tailwind + CSS-var theme tokens
├── routes/+page.svelte          # main shell
│
├── lib/
│   ├── ipc.ts                   # typed invoke wrappers
│   ├── types.ts                 # mirrors Rust serde types
│   ├── themes.ts                # palette definitions
│   ├── markdown.ts
│   ├── actions/                 # Svelte actions
│   ├── stores/                  # SvelteMap-based runes state
│   │   ├── messages.svelte.ts
│   │   ├── repos.svelte.ts
│   │   ├── workspaces.svelte.ts
│   │   ├── theme.svelte.ts
│   │   ├── toasts.svelte.ts
│   │   └── keybindings.svelte.ts
│   │
│   └── components/
│       ├── TitleBar.svelte
│       ├── Sidebar.svelte
│       ├── Toasts.svelte
│       ├── SearchModal.svelte
│       ├── VirtualList.svelte
│       ├── DependencyGraph.svelte
│       ├── RepoSettings.svelte
│       ├── kanban/
│       ├── workspace/
│       ├── chat/
│       └── knowledge/
```

## IPC boundary

All Rust↔Svelte calls go through a typed `ipc.ts`:

```typescript
export const api = {
  repo: {
    add: (path: string): Promise<Repo> => invoke("add_repo", { path }),
    list: (): Promise<Repo[]> => invoke("list_repos"),
    remove: (id: string) => invoke("remove_repo", { repoId: id }),
  },
  workspace: {
    create: (args: CreateWorkspaceArgs): Promise<Workspace> =>
      invoke("create_workspace", args),
    // ...
  },
  task: {
    list: (repoId: string, filter?: TaskFilter): Promise<ExternalTask[]> =>
      invoke("list_tasks", { repoId, filter }),
    import: (repoId: string, taskIds: string[]) =>
      invoke("import_tasks", { repoId, taskIds }),
  },
};

export function agentChannel(): Channel<AgentEvent> { ... }
export function terminalChannel(): Channel<Uint8Array> { ... }
```

## State management

**Rust** — `AppState` behind `Arc<Mutex<_>>`, separate states when I/O isolation matters:

```rust
pub struct AppState {
    pub repos: HashMap<String, RepoInfo>,
    pub workspaces: HashMap<String, WorkspaceInfo>,
    pub sessions: HashMap<String, SessionInfo>,
    pub agents: HashMap<String, AgentHandle>,
    pub context_meta: HashMap<String, ContextMeta>,
    pub context_agents: HashMap<String, AgentHandle>,
    pub task_providers: HashMap<String, ProviderConfig>,
}

// Registered separately:
pub struct LspServerPool { /* its own Mutex */ }
pub struct SharedProviderRegistry { /* task providers */ }
```

**Mutex discipline.**
- Acquire lock → extract data → drop lock *before* any I/O or spawn.
- Never hold a lock across spawned processes.
- Never `.unwrap()` in command handlers; always `map_err(|e| e.to_string())`.

**Svelte** — runes-based, nested `SvelteMap` for collections that update frequently:

```typescript
class MessagesStore {
  #byWorkspace = new SvelteMap<string, SvelteMap<string, Message>>();
  get(wsId: string): SvelteMap<string, Message> { ... }
  upsert(wsId: string, msg: Message) {
    this.get(wsId).set(msg.id, msg);  // in-place reactive mutation
  }
}
```

## Event-flow patterns

1. **Request/response** (`invoke`) — command-style sync calls.
2. **High-frequency stream** (`Channel`) — zero-copy binary/struct stream
   (PTY, Claude stream-json).
3. **Broadcast event** (`emit`/`listen`) — low-frequency notifications
   (agent status change, workspace created).

---

# 4. Cross-platform strategy

## 4.1 PTY

`portable-pty` crate. Unix → `fork()` + `openpty`. Windows → ConPTY (Win 10 1809+).

```rust
let pty_system = native_pty_system();
let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, ..Default::default() })?;
let mut builder = CommandBuilder::new(cmd);
builder.args(args).cwd(cwd);
let child = pair.slave.spawn_command(builder)?;
drop(pair.slave);  // critical for Unix EOF semantics
```

**Windows gotchas.** Extra ANSI escapes from ConPTY (xterm.js handles OK);
`\r\n` line endings — forward bytes verbatim; explicit stdin flush required;
`child.kill()` terminates whole tree on Windows, only parent on Unix.

## 4.2 Paths

- Never string-concat; always `PathBuf::join()`.
- Store `PathBuf`; serialize with forward-slash String for portability.
- `dunce::canonicalize` on Windows to normalize UNC paths.
- Reject reserved names on Windows: `CON`, `PRN`, `NUL`, etc.
- NFC-normalize paths on macOS (APFS composed vs HFS+ decomposed).

## 4.3 Shell & env injection

```rust
pub fn default_shell() -> (PathBuf, Vec<String>) {
    #[cfg(windows)]
    {
        if let Ok(p) = which::which("pwsh") { return (p, vec!["-NoLogo".into(), "-Command".into()]); }
        if let Ok(p) = which::which("powershell") { return (p, vec!["-NoLogo".into(), "-Command".into()]); }
        (PathBuf::from("cmd.exe"), vec!["/C".into()])
    }
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        (PathBuf::from(shell), vec!["-lc".into()])
    }
}
```

Env injection via `Command::env()` per-process — never mutate global/ambient env.

## 4.4 Binary detection

`claude`, `gh`, `git`, LSP servers — fallback chain:

1. Check user override setting.
2. `which::which(...)` on PATH.
3. OS-specific fallback paths.
4. Fail loudly → show install-guide modal with deep-link.

## 4.5 Credential storage

`keyring` crate, Linux headless fallback via AES-GCM encrypted file. See §1.

## 4.6 WebView consistency

| Feature | macOS (WKWebView) | Windows (WebView2) | Linux (WebKitGTK) |
|---|---|---|---|
| CSS `:has()` | 15.4+ | ✓ | 6–12 mo lag |
| `backdrop-filter` | ✓ | ✓ | partial |
| WebGL | ✓ | ✓ | GPU-driver dependent |

**Mitigations.** Use Baseline-widely-available CSS only. Bundle Space Grotesk
WOFF2 in `static/fonts/`. Use Tauri clipboard plugin (bypasses webview quirks).

## 4.7 CI build matrix

- Ubuntu 22.04 and Windows Server 2022 on every PR.
- macOS 14 on nightly + release tag only (cost saving).
- Playwright runs per OS with the WebKit engine on Linux/Mac, Chromium-equivalent
  logic on Windows.

## 4.8 Platform-specific feature gating

Via `#[cfg(target_os = "...")]` and UI feature detection. No single-OS-only
features in core paths; any such features (macOS traffic-light positioning, Windows
jumplist) are purely cosmetic additions.

---

# 5. Task provider design

## 5.1 Trait

```rust
#[async_trait]
pub trait TaskProvider: Send + Sync {
    fn id(&self) -> &'static str;                           // "jira" | "lark-bitable"
    fn display_name(&self) -> &'static str;
    fn config_schema(&self) -> serde_json::Value;           // JSON Schema for UI
    fn validate_config(&self, config: &Value) -> Result<(), String>;
    async fn test_connection(&self, config: &Value) -> Result<TestResult, ProviderError>;
    async fn list_tasks(&self, config: &Value, filter: &TaskFilter) -> Result<Vec<ExternalTask>, ProviderError>;
    async fn get_task(&self, config: &Value, external_id: &str) -> Result<ExternalTask, ProviderError>;
    async fn update_status(&self, config: &Value, external_id: &str, new_status: &str)
        -> Result<(), ProviderError> { Err(ProviderError::Unsupported) }
    fn deep_link(&self, config: &Value, external_id: &str) -> Option<String>;
    async fn list_statuses(&self, config: &Value) -> Result<Vec<StatusOption>, ProviderError> { Ok(vec![]) }
}
```

## 5.2 Shared types

```rust
pub struct ExternalTask {
    pub provider_id: String,
    pub external_id: String,
    pub title: String,
    pub description: Option<String>,   // markdown
    pub status: String,
    pub assignee: Option<Assignee>,
    pub labels: Vec<String>,
    pub priority: Option<String>,
    pub url: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub raw: Value,                    // escape hatch
}

pub struct TaskFilter { status, assignee, label, query, limit, cursor }

pub enum ProviderError {
    AuthFailed(String),
    NotFound,
    RateLimited { retry_after_s: u32 },
    NetworkError(String),
    ConfigInvalid(String),
    Unsupported,
    ProviderError(String),
}
```

## 5.3 Registry

`SharedTaskRegistry = Arc<TaskProviderRegistry>` managed by Tauri. `new()` inserts
`"jira"` and `"lark-bitable"` entries.

## 5.4 Jira Cloud impl

- Auth: email + API token (Basic auth header), token stored in keyring.
- Config: `site`, `email`, `api_token`, `project_key`, `default_jql`.
- API: `/rest/api/3/search`, `/issue/{key}`, `/issue/{key}/transitions`.
- Description parsing: ADF → markdown converter.
- Deep link: `https://{site}/browse/{key}`.
- Rate limit: 10 req/s per user, exponential backoff on 429.

## 5.5 Lark Bitable impl

- Auth: tenant access token (App ID + App Secret). OAuth upgrade in Phase 3.5 if
  multi-user scoping needed.
- Config: `region` (larksuite-sg / feishu-cn / global), `app_id`, `app_secret`,
  `base_app_token`, `table_id`, `view_id` (optional), `field_mapping`.
- When `base_app_token` + `table_id` entered, UI auto-calls field-list API and
  populates dropdowns for `field_mapping`.
- API base: `https://open.larksuite.com/open-apis/` (Singapore) or
  `https://open.feishu.cn/open-apis/` (China).
- Key endpoints:
  - `POST /auth/v3/tenant_access_token/internal`
  - `POST /bitable/v1/apps/{app_token}/tables/{table_id}/records/search`
  - `GET /bitable/v1/apps/{app_token}/tables/{table_id}/records/{record_id}`
  - `PUT .../records/{record_id}` (status update)
  - `GET .../fields` (for mapping UI)
- Field value normalizer handles: Text, SingleSelect, MultiSelect, User, DateTime, Formula.
- Deep link: `https://{region}.larksuite.com/base/{app_token}?table={table_id}&view={view_id}&record={record_id}`.

**Initial target data (user's real setup).**
- Region: `larksuite-sg`
- Base app token: `FNFlbS3jPa3Yq4sjm8Illj18gnb`
- Table ID: `tblfA8LNgy6dq2tu`

## 5.6 Dynamic config form

`TaskProviderConfig.svelte` lists providers, shows selected provider's JSON
schema via a custom `SchemaForm.svelte` renderer (text / password / enum / nested
object). Test-connection and save buttons gate on validation.

## 5.7 Import popover

`TaskImportPopover.svelte` — loads cached tasks (stale-while-revalidate), filter
chips for status/assignee/query, virtual-list with checkboxes, import button
converts selected to kanban cards in "Todo" column (mapped via
`status_column_mapping`).

## 5.8 Two-way sync (Phase 3.5 — optional)

- Card moves column → `update_status`.
- PR merged via Ansambel → `update_status("Done")`.
- Conflict resolution: timestamp comparison (provider > local = pull; local > provider = push; tie = ask user).
- Toggle per repo: "Sync status changes back to Jira/Lark".

## 5.9 Security

- HTTPS only; reject http://
- `rustls` default TLS verification (no disable)
- Regex scrub secrets in logs
- Reference-by-ID in JSON, secret in keyring

---

# 6. Data model & storage

## 6.1 Principles

- Single source of truth per entity
- JSON files (human-readable, no SQLite)
- Versioned schema (`schema_version` field)
- Debounced writes (500ms)
- Atomic writes via `.tmp` + rename
- Backup on migration

## 6.2 File schemas (excerpts)

### `app_settings.json`

```json
{
  "schema_version": 1,
  "theme": "warm-dark",
  "selected_repo_id": "repo_...",
  "selected_workspace_id": "ws_...",
  "recent_repos": ["..."],
  "font_family": "Space Grotesk",
  "window": { "width": 1400, "height": 900, "maximized": false },
  "shortcuts_overrides": {},
  "onboarding_completed": true
}
```

### `repos.json`

```json
{
  "schema_version": 1,
  "repos": {
    "repo_8f3a2c": {
      "id": "repo_8f3a2c",
      "name": "talentlytica-web",
      "path": "/home/handoko/Work/talentlytica-web",
      "gh_profile": "handokoben",
      "default_branch": "main",
      "created_at": 1776000000,
      "updated_at": 1776099000,
      "task_provider": {
        "provider_id": "lark-bitable",
        "config_ref": "repo_8f3a2c",
        "non_secret_config": {
          "region": "larksuite-sg",
          "base_app_token": "FNFlbS3jPa3Yq4sjm8Illj18gnb",
          "table_id": "tblfA8LNgy6dq2tu",
          "field_mapping": { "title_field": "Task", "status_field": "Status" }
        },
        "status_column_mapping": {
          "Todo": "todo", "Doing": "in_progress", "Review": "review", "Done": "done"
        },
        "sync_enabled": false
      },
      "pr_template_cache": { "content": "...", "cached_at": 1776099000 },
      "scripts": [
        { "id": "sc_1", "name": "Run tests", "command": "bun test" }
      ],
      "default_provider": "claude"
    }
  }
}
```

### `workspaces.json`

```json
{
  "schema_version": 1,
  "workspaces": {
    "ws_b71a9e": {
      "id": "ws_b71a9e",
      "repo_id": "repo_8f3a2c",
      "branch": "feat/task-123-fix-login",
      "base_branch": "main",
      "custom_branch": false,
      "title": "Fix login bug",
      "description": "...",
      "task_id": "PROJ-123",
      "task_provider_id": "jira",
      "task_url": "https://handokoben.atlassian.net/browse/PROJ-123",
      "status": "Running",
      "column": "in_progress",
      "provider_override": null,
      "source_pr": null,
      "source_prs": [],
      "created_at": 1776000000,
      "updated_at": 1776099500,
      "diff_stats": { "added": 45, "deleted": 12 }
    }
  }
}
```

Other files follow similar pattern: `sessions.json`, `context_meta.json`,
`task_providers_cache.json`, `messages/<wsId>.json`, `todos/<wsId>.json`,
`autopilot_log/<wsId>.json`.

## 6.3 IDs

Format `{prefix}_{6char nanoid}` using a restricted alphabet:

```rust
pub fn repo_id() -> String { format!("repo_{}", nanoid!(6, ALPHABET)) }
pub fn workspace_id() -> String { format!("ws_{}", nanoid!(6, ALPHABET)) }
pub fn message_id() -> String { format!("msg_{}", nanoid!(6, ALPHABET)) }
```

## 6.4 Atomic write

```rust
pub fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let file = File::create(&tmp)?;
        serde_json::to_writer_pretty(BufWriter::new(file), value)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
```

## 6.5 Debounced write

Single queue backed by `mpsc::channel`. 100ms tick checks deadlines, flushes via
`spawn_blocking`. Messages + workspaces + sessions use debounce; settings and
config changes write immediately.

## 6.6 Schema migration

Load path reads `schema_version`, backs up original as `.v{N}.bak`, runs migration
chain, writes migrated state back atomically. Start at `schema_version: 1`.

## 6.7 Backup & export

- Automatic: pre-migration backups, weekly rolling snapshot.
- Manual: Settings → Export / Import (zip of data dir minus logs/cache).

## 6.8 Concurrency

- Single `DebouncedWriter` queue serializes writes per file.
- Advisory lock `<data_dir>/.ansambel.lock` with PID + liveness check for multi-instance detection.
- File-watcher refresh when external changes detected.

## 6.9 Log rotation

`tracing-appender` daily rotation, 14-day retention, JSON format optional via
`ANSAMBEL_LOG_FORMAT=json`.

---

# 7. Error handling, observability, build & distribution

## 7.1 Error type

```rust
#[derive(Error, Debug)]
pub enum AppError {
    #[error("I/O error: {0}")] Io(#[from] std::io::Error),
    #[error("Git operation failed: {0}")] Git(String),
    #[error("External command failed: {cmd}: {msg}")] Command { cmd: String, msg: String },
    #[error("Task provider error: {0}")] TaskProvider(#[from] ProviderError),
    #[error("Not found: {0}")] NotFound(String),
    #[error("Invalid state: {0}")] InvalidState(String),
    #[error("Config error: {0}")] Config(String),
    #[error("Serialization: {0}")] Serde(#[from] serde_json::Error),
    #[error("Keyring: {0}")] Keyring(#[from] keyring::Error),
    #[error("Other: {0}")] Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
```

## 7.2 Command handler pattern

```rust
#[tauri::command]
pub async fn create_workspace(...) -> std::result::Result<Workspace, String> {
    create_workspace_inner(...).await.map_err(|e| {
        tracing::error!(error = %e, "create_workspace failed");
        e.to_string()
    })
}

async fn create_workspace_inner(...) -> Result<Workspace> { /* real impl with ? */ }
```

Rules: no `unwrap()`/`expect()` in prod, opaque strings to frontend, secrets
stripped from error messages.

## 7.3 Frontend error handling

```typescript
async function safeInvoke<T>(cmd: string, args?: any): Promise<Result<T>> {
  try {
    return { ok: true, data: await invoke<T>(cmd, args) };
  } catch (e) {
    toasts.error(humanizeError(String(e)));
    return { ok: false, error: String(e) };
  }
}
```

Error humanizer maps technical strings to user-friendly text.

## 7.4 Panic handling

Global panic hook writes crash dump to `<data>/logs/crashes/`, shows user dialog
on next launch.

## 7.5 Observability

- **Rust logging.** `tracing` + `tracing-subscriber` + `tracing-appender` with
  daily rotation.
- **Structured fields preferred** over string interpolation.
- **Frontend logging.** `console.*` in dev; production forwards via IPC to
  backend tracing.
- **Metrics** (Phase 8): in-app command latency p50/p95, agent spawn time, token
  usage per session. Local only; no remote telemetry.
- **Crash reporting.** In-app only; text crash logs available to user for
  voluntary sharing.

## 7.6 Performance targets

| Metric | Target |
|---|---|
| App cold start | <2 s |
| Workspace switch | <100 ms |
| Agent spawn | <500 ms |
| Message render | 60 fps |
| Kanban drag | 60 fps |
| KB build (SMALL repo) | <30 s |
| Binary size | <30 MB |
| Idle RAM | <150 MB |
| RAM per 10 agents | <600 MB |

Perf regression tests via Playwright timing; CI fails on p95 regression >20%.

## 7.7 Security

| Threat | Mitigation |
|---|---|
| Secret exfiltration via logs/errors | OS keyring + regex scrub |
| Command injection | No `sh -c`; always args array |
| Path traversal | Canonicalize + `starts_with(worktree)` |
| Malicious repo hooks | Default trust (user adds); `GIT_NO_HOOKS=1` optional |
| MCP server hijack | 127.0.0.1 only, random port, token auth; 3rd-party with scoped disclosure |
| Vulnerable deps | `cargo audit` + `bun audit` in CI, Dependabot/Renovate |

## 7.8 Build targets

| OS | Bundles | Signing |
|---|---|---|
| Windows | `.msi` + `.exe` (NSIS) | Code-sign cert (~$300/yr) |
| Linux | `.AppImage` + `.deb` + `.rpm` | GPG sign |
| macOS | `.dmg` + `.app` | Apple Developer ID + notarize ($99/yr) |

Tauri bundle config in `src-tauri/tauri.conf.json`. Release pipeline triggered by
`v*` tags.

## 7.9 Auto-updater (Phase 8)

`tauri-plugin-updater` with:
- Self-hosted manifest endpoint (private license).
- Ed25519 signed updates; public key embedded in binary.
- User consent prompt.
- Rollback capability.

## 7.10 Versioning

Semver `MAJOR.MINOR.PATCH`. Phase milestones:
- v0.1.0 — End of Phase 1
- v0.X.0 — End of Phase X
- v1.0.0 — End of Phase 8

## 7.11 First-run onboarding

1. Welcome screen.
2. Binary check (claude / gh / git) with install guides if missing.
3. Optional Jira/Lark connection.
4. Add first repo via folder picker.
5. `onboarding_completed = true`.

## 7.12 Internationalization

Infrastructure from day 1 (`svelte-i18n` + ICU). Default English; Indonesian
translation deferred to post-Phase 1 as time permits.

## 7.13 Documentation deliverables per phase

- Rust doc comments (`///`) and TSDoc.
- User-facing README update.
- CHANGELOG entries.
- ADRs for non-obvious decisions (`docs/adr/NNNN-title.md`).

---

# Appendix A — Hard rules for the agent (copied to `.claude/CLAUDE.md`)

## Rust

- Every `#[tauri::command]` returns `Result<T, String>` — no panics, no unwrap.
- No `unwrap()`/`expect()` outside tests.
- Shared mutable state via `Arc<Mutex<_>>` in Tauri `manage()`; separate states
  when I/O isolation matters (LspServerPool). No globals, no `lazy_static`.
- Mutex discipline: lock → extract → drop before I/O/async.
- PTY reader threads handle EOF/errors gracefully, emit status on exit.
- `portable-pty`: always close slave end in parent after spawning child.
- Spawn `claude` with explicit env — inject `GH_TOKEN` per-process.
- Agent processes use `--permission-mode bypassPermissions` with
  `--disallowedTools EnterWorktree,ExitWorktree` — never
  `--dangerously-skip-permissions`.
- Detect default branch from remote tracking refs only (origin/HEAD, origin/main,
  origin/master). Never fall back to local refs.
- Never call `gh auth switch` globally — use `gh auth token --user <profile>` and
  inject per-process.

## Frontend

- PTY output never touches Svelte state — xterm.js owns its buffer.
- Messages use `SvelteMap<id, Message>`, mutated in place — never replace
  entire arrays.
- xterm instances use `display: none/block` on workspace switch — never
  mount/unmount.
- Tauri Channel API for binary streams — never `listen()` + JSON for
  high-frequency data.
- All `invoke()` calls wrapped in try/catch with user-visible error handling.
- Tooltips via the `use:tooltip` Svelte action — never native `title`.
- Token input counts sum `input_tokens` + `cache_creation_input_tokens` +
  `cache_read_input_tokens`.

## Data

- All app data under the OS-resolved app-data dir — zero writes to managed
  repos.
- Worktrees at `<data_dir>/workspaces/<workspace-id>/`.
- Atomic writes via `.tmp` + rename.
- Debounced writes for messages/workspaces/sessions (500ms); immediate for
  settings.
- Workspace status resets to `Waiting` on app restart.

## Testing

- No production code without a failing test first (TDD).
- 95% unit + branch coverage gate; 95% e2e scenario coverage of documented
  golden paths.
- Every `#[tauri::command]` has ≥1 unit + ≥1 integration test.
- Every Svelte component has ≥1 test (happy path + ≥1 edge case).
- External services mocked in unit/integration; E2E uses Playwright with mock
  Claude binary.
- `#[ignore]` / `test.skip` only with linked GitHub issue.

## Commands

- Use `bun`, not `npm`/`npx`/`yarn`.
- Type-check: `bun run check`.
- Rust check: `cargo check`.

## General

- No `console.log` in production paths; use `tracing` in Rust and a
  backend-forwarded logger in Svelte.
- No hardcoded paths — derive from app data dir / repo root.
- Async filesystem/process ops must have timeouts.

---

# Appendix B — Not in scope

Explicitly **not building** (inherited from korlap scope statement and Ansambel
decisions):

- Codex support (Claude CLI only).
- Checkpoint/restore of Claude conversation history.
- Multi-repo open simultaneously.
- Public / marketplace release (private license).
- GitLab / Bitbucket git providers (Phase 1–8).
- Linear / Notion / Asana task providers (architecture ready, not shipped).
- Remote / multi-machine agent orchestration.
- Mobile / web versions.

---

# Appendix C — Open decisions (Phase 3.5+)

Parked for later:

- Two-way task sync (Phase 3.5) — currently opt-in.
- Lark OAuth (vs tenant token).
- macOS support full polish (currently nice-to-have).
- Indonesian UI translation.
- Auto-updater manifest hosting strategy.

---

*End of design spec.*
