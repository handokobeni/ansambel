# Phase 2b — Interactive Surfaces (Backend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Three interactive surfaces — workspace terminal, file editor
(read+write), and per-repo script runner — each backed by streaming Tauri
commands. Reuses `platform::pty` from Phase 1 for the terminal and script paths
so we don't ship a second PTY abstraction.

**Architecture:** Three loosely-related subsystems, all of which write into the
worktree (so the "zero writes to managed repo" rule still holds).

- **Terminal** is a per-workspace, long-lived `portable-pty` session. One PTY
  per workspace, kept alive across tab switches via a handle in AppState. Reuses
  the same broadcaster + Channel reattach pattern Phase 1 already proved out for
  agent processes.

- **Editor** is two synchronous commands (`file_read`, `file_write`) that work
  against canonicalized worktree-relative paths. No streaming — file content
  fits in one IPC payload up to a sane cap (1 MB default). Binary detection up
  front so the frontend doesn't have to re-implement it.

- **Script runner** spawns a one-shot PTY per script invocation, streams its
  output into the same terminal tab the user is already watching. This keeps the
  UX simple ("Run tests" replaces the active shell with a script run, scripts
  have lifecycle parity with the interactive shell).

**Tech stack:** No new crates beyond what Phase 1 brought in. We reuse
`portable-pty = "0.8"`, `dunce = "1"`, `tokio` async runtime. Editor saves use
the existing atomic-write helper from `persistence::atomic`.

---

## Dependency additions

None for backend.

---

## File Structure

```
src-tauri/src/
├── commands/
│   ├── mod.rs                            # MODIFY: add `pub mod terminal;` `pub mod file_io;` `pub mod scripts;`
│   ├── terminal.rs                       # CREATE: terminal_spawn / write / resize / kill / reattach
│   ├── file_io.rs                        # CREATE: file_read / file_write
│   └── scripts.rs                        # CREATE: script_list / script_run
├── state.rs                              # MODIFY: AppState.terminals: HashMap<WsId, TerminalHandle>
├── persistence/
│   └── repos.rs                          # MODIFY: persist `scripts: Vec<RepoScript>`
└── lib.rs                                # MODIFY: register 5 new commands + drain handle on exit
```

`TerminalHandle` shape mirrors `AgentHandle` from Phase 1 (writer, broadcaster,
child kill). Lives only in memory — terminal sessions die on app restart, same
as agent sessions.

---

## Task 1: `RepoScript` persistence + `script_list` command [P0]

**Why:** The script runner needs a backed source of "what scripts can I run for
this repo." Spec calls for `repos.json` to carry a `scripts` field; today it
doesn't. Add the field + persistence + a Tauri command that returns the list per
repo.

**Behavior:**

- `RepoScript { id, name, command }` lives on
  `RepoInfo.scripts: Vec<RepoScript>` (default empty for backward compat).
- `script_list(repo_id) -> Vec<RepoScript>` — looks up the repo, returns its
  scripts (empty when repo has none configured).
- `script_set(repo_id, scripts: Vec<RepoScript>)` — replace the repo's script
  list. Phase 2b ships read-only listing via the existing repo settings UI; the
  set command lays the wire for Phase 8 settings polish.

**Files:**

- Modify: `src-tauri/src/state.rs` (RepoInfo + serde default)
- Modify: `src-tauri/src/persistence/repos.rs` (no schema change beyond the new
  field — `#[serde(default)]` covers older files on read)
- Create: `src-tauri/src/commands/scripts.rs`
- Modify: `src-tauri/src/commands/mod.rs` + `lib.rs`

**Tests:**

- [ ] `repo_info_round_trips_scripts_field`.
- [ ] `repos_json_loads_legacy_files_with_no_scripts_field` (empty vec).
- [ ] `script_list_returns_empty_for_unknown_repo`.
- [ ] `script_list_returns_persisted_scripts_in_order`.
- [ ] `script_set_replaces_existing_list_atomically`.
- [ ] `script_list_command_is_registered` + `script_set_command_is_registered`.

---

## Task 2: `terminal_spawn` + handle lifecycle [P0]

**Why:** The Terminal tab needs a real shell rooted at the workspace worktree.
Per-workspace persistence means `cd`-ing in one tab doesn't affect another, and
command history survives tab switches.

**Behavior:**

- Input: `workspace_id`, `Channel<TerminalChunk>`, optional `cols/rows`
  (defaults 80×24).
- Resolves the workspace's `worktree_dir`, picks the user's default shell via
  `platform::shell::default_shell()`, spawns through `platform::pty::spawn_pty`
  rooted at the worktree.
- Inserts a `TerminalHandle { stdin_tx, broadcaster, child_kill }` into AppState
  keyed by workspace id. Spawning twice for the same workspace is rejected with
  a "terminal already active — call reattach" error so the frontend can route
  correctly.
- Spawns a reader thread that pumps PTY stdout bytes into the broadcaster as
  `TerminalChunk::Bytes(Vec<u8>)`. EOF → emit `TerminalChunk::Exited { code }`.
  The reader is the only path that writes binary data into the channel — keep it
  tight.

**Why bytes (not strings):** xterm.js wants raw bytes for ANSI escape parsing.
Strings would force UTF-8 validation we don't need.

**TerminalChunk shape:**

```rust
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalChunk {
    Bytes { bytes: Vec<u8> },
    Exited { code: Option<i32> },
}
```

**Files:**

- Modify: `src-tauri/src/state.rs` (AppState.terminals + TerminalHandle)
- Create: `src-tauri/src/commands/terminal.rs`
- Modify: `src-tauri/src/commands/mod.rs` + `lib.rs`

**Tests:**

- [ ] `spawn_terminal_inner_starts_shell_in_worktree_cwd` — spawn a tiny mock
      shell (an echo loop) and verify the first byte stream contains the
      worktree path after a `pwd` write.
- [ ] `spawn_terminal_inner_rejects_double_spawn`.
- [ ] `spawn_terminal_inner_returns_error_for_unknown_workspace`.
- [ ] `spawn_terminal_inner_returns_error_when_worktree_missing`.
- [ ] `terminal_reader_thread_emits_bytes_then_exit_chunk_on_eof`.
- [ ] `terminal_handle_is_dropped_after_kill`.
- [ ] `terminal_spawn_command_is_registered`.

Mock shell strategy: the existing `platform::pty::tests` use a cross-platform
helper that picks `cmd /c` on Windows and `sh -c` on Unix; reuse it.

---

## Task 3: `terminal_write` / `terminal_resize` / `terminal_kill` /

`terminal_reattach` [P0]

**Why:** Once the terminal is alive, the frontend needs to push keystrokes,
react to window resize, gracefully close, and re-subscribe on workspace switch +
back (the Channel handler is GC'd on unmount, exactly like the agent).

**Behavior:**

- `terminal_write(workspace_id, bytes: Vec<u8>)` — find handle, push bytes onto
  stdin_tx. No-op error if no terminal exists yet.
- `terminal_resize(workspace_id, cols, rows)` — call `pty.resize()` on the
  handle. Cap rows/cols at sane bounds (1..1000) to defend against malformed UI
  input.
- `terminal_kill(workspace_id)` — child_kill, drop handle. Idempotent.
- `terminal_reattach(workspace_id, channel)` — subscribe a fresh
  `Channel<TerminalChunk>` to the existing broadcaster. Returns error if no
  terminal is active — frontend then falls back to spawn.

**Files:**

- Modify: `src-tauri/src/commands/terminal.rs`
- Modify: `src-tauri/src/commands/mod.rs` + `lib.rs`

**Tests:**

- [ ] `terminal_write_inner_pushes_bytes_to_stdin`.
- [ ] `terminal_write_inner_returns_error_when_no_handle`.
- [ ] `terminal_resize_inner_clamps_extreme_values`.
- [ ] `terminal_kill_inner_is_idempotent`.
- [ ] `terminal_reattach_inner_delivers_subsequent_chunks_to_new_channel`.
- [ ] `terminal_reattach_inner_errors_when_no_active_session`.
- [ ] All four commands registered.

---

## Task 4: `file_read` + `file_write` [P0]

**Why:** Editor needs read on open and write on Ctrl+S. Both must respect the
worktree-canonicalize-and-prefix-check defense already in `workspace_files`.
Atomic write via `.tmp` + rename so a crash mid-save never leaves a half-written
file.

**Behavior:**

- `file_read(workspace_id, path)` returns
  `{ content: String, is_binary: bool, size: u64, sha1: String }`. NUL byte
  detection in first 8 KB → `is_binary = true`, content stays empty so the
  frontend renders a "binary file" placeholder. Files >1 MB return
  `is_binary: false, size: N` with a one-line content stub saying "file too
  large" and the editor refuses to open. Cap is defended at the IPC boundary so
  a misbehaving frontend can't OOM the app.
- `file_write(workspace_id, path, content, expected_sha1)` writes `content`
  atomically. The optional `expected_sha1` is what the frontend received from
  `file_read`; if the on-disk sha1 doesn't match, the command rejects with a
  `FileChangedOnDisk` error so the editor can prompt "the file changed under
  you, reload?". This catches the Claude-agent-edited-while-you-typed race that
  plagues korlap.

**Files:**

- Create: `src-tauri/src/commands/file_io.rs`
- Modify: `src-tauri/src/commands/mod.rs` + `lib.rs`

Reuses `commands::files::resolve_within_worktree` (extract to a shared helper if
it isn't already pub).

**Tests:**

- [ ] `file_read_inner_returns_text_for_text_file`.
- [ ] `file_read_inner_marks_binary_for_nul_bytes`.
- [ ] `file_read_inner_caps_at_1mb_with_descriptive_marker`.
- [ ] `file_read_inner_returns_sha1_of_content`.
- [ ] `file_read_inner_rejects_path_traversal`.
- [ ] `file_write_inner_writes_atomically`.
- [ ] `file_write_inner_rejects_when_expected_sha1_mismatches`.
- [ ] `file_write_inner_creates_parent_directories_as_needed`.
- [ ] `file_write_inner_rejects_path_traversal`.
- [ ] `file_write_inner_persists_to_disk_after_successful_call` (re-read sanity
      check).
- [ ] Both commands registered.

---

## Task 5: `script_run` [P0]

**Why:** The whole point of the runner — pick a script, watch its output stream
into the terminal tab. Output goes through the same broadcaster that interactive
`terminal_spawn` uses, but the script's PTY is ephemeral (lifetime = script
run).

**Behavior:**

- Input: `workspace_id`, `script_id`. Resolves the workspace, looks up the
  script via the workspace's repo. Spawns a fresh PTY rooted at the worktree
  (`sh -c "<script.command>"` on Unix, `cmd /c` on Windows), shares the
  workspace's TerminalHandle broadcaster so output arrives in whichever Channel
  the frontend is subscribed to. Emits `TerminalChunk::Bytes` for stdout and an
  `Exited { code }` chunk when the child exits.
- If no terminal is active for the workspace, the script run creates one
  transiently — bytes still flow into a freshly-subscribed Channel the frontend
  wires up before invoking. Exit chunk closes the run.

**Files:**

- Modify: `src-tauri/src/commands/scripts.rs`
- Modify: `src-tauri/src/commands/mod.rs` + `lib.rs`

**Tests:**

- [ ] `script_run_inner_streams_output_into_broadcaster`.
- [ ] `script_run_inner_emits_exit_chunk_with_status`.
- [ ] `script_run_inner_returns_error_for_unknown_script_id`.
- [ ] `script_run_inner_returns_error_for_unknown_workspace_id`.
- [ ] `script_run_inner_writes_to_existing_terminal_broadcaster_when_active`.
- [ ] `script_run_command_is_registered`.

---

## Lifecycle / shutdown

- On `RunEvent::ExitRequested` (lib.rs): drain all `TerminalHandle` child kills
  before the runtime stops. Mirrors how the agent handles on-exit — orphaned PTY
  readers are a leak vector.
- On workspace remove: kill the workspace's terminal handle if any.

---

## Risks

- **Windows + portable-pty quirks** — Phase 1's agent already shipped PTY on
  Windows runners, but the interactive shell path uses ConPTY more aggressively
  (resize events, control sequences). Mitigation: E2E spec runs on
  `windows-2022` with a `cmd /c echo hello` smoke.
- **Long-running script captures** — a `bun test --watch` will never exit,
  blocking the tab on a single run. Mitigation: a "Stop" affordance on the
  terminal panel that calls `terminal_kill`. Same pattern the agent's Stop
  button uses.
- **expected_sha1 race** — agent edits a file between `file_read` and
  `file_write`; user's keystrokes get rejected. By design — the alternative
  (silently overwrite) is worse. Surface the rejection as a clear "agent edited
  this file, reload?" toast.

---

## Testing strategy

Same hard rule: TDD red→green→refactor, ≥1 unit test + ≥1 integration test per
command, 95% coverage gate. Add the new files (`terminal|file_io|scripts`) to
the CI exclusion regex since the async tauri shells aren't unit-testable,
mirroring the Phase 2a pattern.

---

## Checklist (high level)

- [ ] Task 1 — `RepoScript` persistence + `script_list` / `script_set`
- [ ] Task 2 — `terminal_spawn` + handle lifecycle
- [ ] Task 3 — `terminal_write` / `terminal_resize` / `terminal_kill` /
      `terminal_reattach`
- [ ] Task 4 — `file_read` / `file_write` with sha1 race-detect
- [ ] Task 5 — `script_run` streaming into terminal broadcaster
- [ ] All 8 commands registered in `lib.rs`
- [ ] `cargo clippy --lib --all-targets -- -D warnings` clean
- [ ] CI coverage exclusion regex updated for new files
- [ ] Coverage on changed files ≥ 95%
