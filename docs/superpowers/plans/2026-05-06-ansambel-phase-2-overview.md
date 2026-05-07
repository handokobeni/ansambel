# Phase 2 — Work Mode Complete (Overview)

> This is the **overview** for Phase 2. Detailed task-by-task plans live in
> `2026-XX-XX-ansambel-phase-2{a,b,c}-{backend,frontend}.md` and are written at
> the start of each sub-phase (mirroring the Phase 1 cadence — `phase-1a-*` was
> written separately from `phase-1b-*`, etc.).

## Goal

Make Work Mode actually useful. Today, opening a workspace shows a chat panel
and nothing else — no diff, no terminal, no editor, no file browser. Phase 2
fills the surfaces that turn Ansambel from "chat with claude in a worktree" into
a real coding environment.

## Scope (per design spec §2 — Phase 2)

Seven features:

1. **Diff viewer** — syntax-highlighted, shows uncommitted changes in the
   worktree
2. **xterm.js terminal** — per-workspace shell rooted at the worktree cwd
3. **File browser** — tree of worktree files with collapse/expand
4. **CodeMirror 6 editor** — open any worktree file, edit, save
5. **Script runner** — per-repo scripts (from `repos.json.scripts`)
6. **Search modal** — files (filename) + content (grep)
7. **@-file mentions** — autocomplete in chat input that injects file paths

## Sub-phase split

Phase 1 was split into 1a / 1b / 1c / 1e by surface area + dependency. Phase 2
follows the same pattern. The 7 features cluster into 3 sub-phases:

### 2a — Read-only foundation (~1 wk)

- Diff viewer
- File browser tree
- Search modal (file + content)

**Why grouped:** all read-only. Shared backend infra (`git diff` + `walkdir` +
optional `ripgrep`) and shared frontend infra (tab system + virtualized list).
No PTY, no editor write path. Lowest risk and builds the tab infrastructure that
2b/2c plug into.

### 2b — Interactive surfaces (~1.5 wk)

- xterm.js terminal (reuses `platform/pty.rs`)
- CodeMirror 6 editor (open + save)
- Script runner (output rendered in terminal tab)

**Why grouped:** all involve write paths or running processes. Terminal reuses
existing PTY code already proven for agent processes. Editor + script runner
share a "file save → notify worktree" flow.

### 2c — Productivity glue (~0.5 wk)

- @-file mentions in chat input

**Why separate:** depends on the file index from 2a. Chat-panel UX matures on
its own track from raw work-mode tabs. Small scope, natural tail.

## Architecture decisions (binding)

- **Tab system in `WorkspaceView`** — replace today's single chat panel with a
  tab strip (Chat / Diff / Files / Editor / Terminal). Tab state persists across
  workspace switches using the same `display: none / block` hidden mount pattern
  that's a hard rule for xterm. Per-workspace tab state lives in the workspace
  store.
- **xterm + CodeMirror never remount** on workspace switch — they obey the hard
  rule.
- **Search backend = ripgrep** via `which::which("rg")` with a `walkdir`
  fallback. Ripgrep is treated as an optional binary (missing → degrade to
  `walkdir`-only filename search; UI flag this).
- **Diff backend = `git diff` shell-out**, not `libgit2` — matches
  `detect_default_branch` pattern and avoids C-dep churn.
- **Editor save = atomic write** to the worktree path (worktree, not the managed
  repo — zero-write-to-managed-repo rule still holds).
- **Script runner reuses PTY** — output streams into the terminal tab via the
  same Channel pattern as the agent.
- **Diff for large outputs streams** via Tauri Channel rather than a single JSON
  serialization (`git diff` against a large refactor can exceed 5 MB).

## Out of scope (deferred)

- Multi-cursor / advanced editor features → Phase 4+
- Diff "accept hunk" UX → Phase 4 (`EditDiffBlock`)
- Command palette / fuzzy-finder → Phase 8 polish
- Inline LSP hover / diagnostics in editor → Phase 6
- Code folding, minimap → Phase 8

## Risks

- **WebKitGTK font rendering** — already flagged in design spec. Mitigation:
  bundle Space Grotesk + a monospace fallback; visual-regression test on Linux
  runner.
- **xterm.js bundle size** — adds ~150 KB to frontend. Within perf budget (<30
  MB binary; <150 MB idle RAM).
- **CodeMirror 6 + Svelte 5 integration** — no first-party wrapper, ecosystem
  wrappers often lag. Mitigation: mount `EditorView` from `@codemirror/view`
  directly with a ref pattern in a Svelte component.
- **ripgrep optional dep** — needs the same binary-detection chain as `claude` /
  `gh`. Surface "rg not found" via the same actionable-error CTA that Phase 1e
  introduced for missing claude.
- **Large diff/search output** — must stream, not serialize. Pattern reuse from
  agent stream-json.

## Testing strategy

Same hard rule as Phase 1: TDD, 95% line+branch coverage gate, every
`#[tauri::command]` has ≥1 unit test + ≥1 integration test, every Svelte
component has happy-path + ≥1 edge-case test.

Per sub-phase E2E golden paths:

- **2a:** Open workspace → click Diff tab → see colored hunks reflecting
  uncommitted changes. Open Search → type a query → see hits with line context.
  Click a hit → file opens in Files tab tree expanded to that path.
- **2b:** Open Terminal tab → type `pwd` → see worktree path. Open a file in the
  Editor tab → modify → save (`Ctrl+S`) → re-read disk and verify. Run a
  configured script → output streams into Terminal tab.
- **2c:** In chat input, type `@src/li` → autocomplete shows `src/lib/...` files
  → pick one → message body now contains `@src/lib/whatever.ts`.

## Ship gate

Per design spec §2 ("Ship gates") and §7.10 ("Versioning"):

- **v0.2.0** — End of Phase 2.

Sub-phase intermediate tags optional, following Phase 1 cadence
(`v0.1.x-phase2a`, etc.); decide per merge.

## Implementation cadence

Detailed plans (`phase-2a-backend.md`, `phase-2a-frontend.md`, etc.) are written
at sub-phase start, not now. Phase 1 history shows the detailed plans are most
useful when they reflect the codebase shape at implementation time; pre-writing
all three risks drift. This overview is the binding architectural commitment —
sub-phase detail will refine, not contradict it.

## Checklist

- [ ] Phase 2a — Read-only foundation merged
- [ ] Phase 2b — Interactive surfaces merged
- [ ] Phase 2c — Productivity glue merged
- [ ] Phase 2 retrospective entry in `journal/`
- [ ] `v0.2.0` tagged, README status updated
