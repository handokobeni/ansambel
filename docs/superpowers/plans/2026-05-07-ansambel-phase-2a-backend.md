# Phase 2a — Read-only Work Mode (Backend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Three new Tauri commands that turn the worktree into something the
frontend can browse, diff, and search — all read-only, with streaming for the
two outputs that can blow up in size (`git diff`, content search).

**Architecture:** Each command is a thin wrapper around a well-known Unix
primitive: `git diff` for diffs, `walkdir` (or `ignore` for gitignore-aware
traversal) for the file tree, and `ripgrep` (with a `walkdir` filename fallback)
for search. The diff and search commands stream batches over a Tauri
`Channel<T>` so a 5 MB diff or a 10 000-hit grep doesn't serialize through one
IPC payload. The file-tree command is bounded by the worktree size and returns a
single JSON tree (lazy expansion happens client-side via re-invoke per
directory).

**Tech Stack:** Rust 1.82+, Tauri v2 Channels, `git` shell-out via
`std::process::Command`, two new crates (`ignore` for gitignore-aware walking,
no new search crate — we shell out to `rg`). No PTY here; PTY arrives in 2b.

---

## Dependency additions

Add to `src-tauri/Cargo.toml`:

```toml
ignore = "0.4"            # gitignore-aware filesystem walker (used by ripgrep)
```

`which = "7"` is already present and reused for ripgrep detection. No ripgrep
crate — we shell out to the binary like `git`.

---

## File Structure

```
src-tauri/src/
├── commands/
│   ├── mod.rs                        # MODIFY: add `pub mod files;` `pub mod search;` `pub mod diff;`
│   ├── diff.rs                       # CREATE: workspace_diff command
│   ├── files.rs                      # CREATE: workspace_files command
│   └── search.rs                     # CREATE: workspace_search command
└── lib.rs                            # MODIFY: register 3 new commands
```

Each new command file ships with its own `#[cfg(test)] mod tests` block holding
both `_inner` unit tests (no Tauri context) and command-existence checks.

---

## Task 1: `workspace_diff` command [P0]

**Why:** Frontend needs the unified diff for uncommitted worktree changes to
render colored hunks in the Diff tab. `git diff` is the only correct source of
truth — `libgit2` is rejected per the design spec to avoid C-dep churn.

**Behavior:**

- Resolves workspace by id from AppState; reads `worktree_dir`.
- Runs `git -C <worktree> diff --no-color HEAD` (default) — stages and unstaged
  changes against the last commit, the same view a user gets in korlap's diff
  tab.
- Streams output in 64 KB chunks over a `Channel<DiffChunk>`. Final chunk
  carries `eof: true`. A non-zero exit code emits `DiffChunk::Error` with the
  captured stderr.
- Untracked files surface as synthetic `+++ b/<path>` blocks (since `git diff`
  excludes them by default). The command runs
  `git ls-files --others --exclude-standard` first, then concatenates a
  unified-diff-shaped header for each untracked file followed by its content as
  additions.

**Files:**

- Create: `src-tauri/src/commands/diff.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (register)
- Test: `src-tauri/src/commands/diff.rs` (`#[cfg(test)] mod tests`)

**Tests (TDD order):**

- [ ] `workspace_diff_inner_returns_empty_for_clean_worktree` — init temp git
      repo with one committed file, run command, assert chunks vector is
      `[{ eof: true }]`.
- [ ] `workspace_diff_inner_returns_unified_diff_for_modified_file` — modify
      committed file, assert concatenated chunks contain `--- a/foo.txt`,
      `+++ b/foo.txt`, and the `-old` / `+new` lines.
- [ ] `workspace_diff_inner_includes_untracked_as_full_addition` — create
      `untracked.txt` with content `hello\n`, assert output contains
      `+++ b/untracked.txt` and `+hello`.
- [ ] `workspace_diff_inner_chunks_large_diff` — generate a file with 8 192
      modified lines, assert the captured chunks vector has length ≥ 2 and every
      non-final chunk is ≤ 64 KB.
- [ ] `workspace_diff_inner_returns_error_for_invalid_workspace_id` — assert
      `Err` with descriptive message.
- [ ] `workspace_diff_inner_returns_error_for_non_git_worktree` — temp dir
      without `.git`, assert error contains "not a git repository".
- [ ] `workspace_diff_command_is_registered` — symbol existence check in
      `lib.rs::tests`.

---

## Task 2: `workspace_files` command [P0]

**Why:** File browser tree needs an enumeration of the worktree, gitignore-
aware (so `target/`, `node_modules/`, `dist/` don't drown the tree). Lazy
expansion happens client-side by re-invoking with a sub-path.

**Behavior:**

- Input: `workspace_id`, `path: Option<String>` (relative, defaults to `""` =
  worktree root).
- Resolves worktree dir, joins path (rejects path traversal — see security note
  below).
- Lists _immediate_ children only via
  `ignore::WalkBuilder::new(target) .max_depth(Some(1))`. Returns
  `Vec<FileEntry>` where each entry is `{ name, path, kind: "file" | "dir" }`
  sorted directories-first then alphabetical.
- Does NOT recurse — the tree expands lazily. This caps the per-call payload to
  a single directory's children (typically <500 entries) and avoids serializing
  the whole tree.

**Security note:** `path` is canonicalized with `dunce::canonicalize` (or plain
`canonicalize` on Unix) and then verified to start with the worktree root.
Rejects symlinks that escape the worktree. This is the same defense korlap
applies for its file commands.

**Files:**

- Create: `src-tauri/src/commands/files.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (register)

**Tests (TDD order):**

- [ ] `workspace_files_inner_lists_root_children` — temp worktree with `a.txt`,
      `b.txt`, `subdir/`, assert returned entries are
      `[subdir (dir), a.txt (file), b.txt (file)]`.
- [ ] `workspace_files_inner_lists_subdir_children` — subdir contains `c.txt`,
      call with `path = "subdir"`, assert single entry `c.txt`.
- [ ] `workspace_files_inner_respects_gitignore` — add `.gitignore` with
      `target/`, create `target/x.txt`, assert `target` is filtered out at root
      level.
- [ ] `workspace_files_inner_does_not_recurse` — nested dirs only show top level
      (depth check).
- [ ] `workspace_files_inner_rejects_path_traversal` — `path = "../etc"` asserts
      error containing "outside worktree".
- [ ] `workspace_files_inner_returns_error_for_invalid_workspace_id`.
- [ ] `workspace_files_command_is_registered`.

---

## Task 3: `workspace_search` command [P0]

**Why:** Cmd+P / Cmd+Shift+F equivalent — the frontend SearchModal needs
filename + content search. Ripgrep is the right tool when present; graceful
fallback to filename-only via `walkdir` keeps the feature usable on bare-bones
environments.

**Behavior:**

- Input: `workspace_id`, `query: String`, `mode: "filename" | "content"`,
  `Channel<SearchHit>`.
- Filename mode: walks via `ignore::Walk` (gitignore-aware), case-insensitive
  substring match against the relative path. No external binary required. Sends
  `SearchHit { kind: "filename", path }` per match.
- Content mode: prefers `which::which("rg").ok()`. If present, spawns
  `rg --json --max-count 100 --max-filesize 1M -- <query>` rooted at the
  worktree. Parses ripgrep's JSON-lines output and forwards `match` records as
  `SearchHit { kind: "content", path, line_number, line_text }`. If rg is
  absent, sends a single `SearchHit::RipgrepUnavailable { reason }` followed by
  `eof` so the frontend can show the "install rg" CTA without silently
  degrading. (Cap at 500 hits to stay bounded.)
- Final `SearchHit::Eof` always closes the stream so the frontend can release
  its loading spinner.

**Why `--json` and not raw output:** parsing colored grep output is brittle.
Ripgrep's `--json` is documented and stable.

**Why a single command for both modes (instead of two):** the frontend toggles
between modes on the same modal — keeping one IPC keeps the streaming/cancel
surface small.

**Files:**

- Create: `src-tauri/src/commands/search.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (register)

**Tests (TDD order):**

- [ ] `workspace_search_inner_filename_mode_finds_substring` — files
      `foo/bar.ts`, `baz.md`, query `bar`, assert one hit with path
      `foo/bar.ts`.
- [ ] `workspace_search_inner_filename_mode_is_case_insensitive`.
- [ ] `workspace_search_inner_filename_mode_respects_gitignore`.
- [ ] `workspace_search_inner_content_mode_finds_match` — assumes `rg` is
      available; gated with `if which::which("rg").is_err() { return; }` so the
      test is skipped on bare runners but runs on CI where rg is installed
      (Ubuntu + Windows GitHub runners both have it). Assert hit includes
      `line_number` and `line_text`.
- [ ] `workspace_search_inner_content_mode_caps_hits_at_500`.
- [ ] `workspace_search_inner_content_mode_emits_unavailable_when_rg_missing` —
      set `PATH` to an empty dir for the test process via the helper
      `with_empty_path`, assert first emitted hit is the unavailable sentinel.
- [ ] `workspace_search_inner_emits_eof_at_end` — every successful path ends
      with `Eof`.
- [ ] `workspace_search_inner_returns_error_for_empty_query`.
- [ ] `workspace_search_command_is_registered`.

---

## Streaming pattern

Mirrors the agent stream-json contract from Phase 1:

```rust
#[tauri::command]
pub async fn workspace_diff(
    state: State<'_, Arc<Mutex<AppState>>>,
    channel: Channel<DiffChunk>,
    workspace_id: String,
) -> Result<()> { ... }
```

`DiffChunk` and `SearchHit` are `#[serde(tag = "kind")]` enums so the frontend
can pattern-match without a discriminator field of its own. Cancellation:
dropping the `Channel` on the frontend side causes the backend `tx.send` to
error, which the streaming loop interprets as "stop" and exits cleanly. No
cancellation token needed for these short-lived operations.

---

## Risks

- **Ripgrep absence on Windows runners** — verified GitHub Actions
  `windows-2022` ships with rg in the default PATH. If a future runner drops it,
  the unavailable-sentinel branch is the user-visible fallback.
- **Symlink loops in `ignore` walker** — `WalkBuilder::follow_links(false)` is
  the default; assert this in tests so a future change can't silently enable
  link-following.
- **Large untracked binary file in diff** — concatenating a 100 MB binary blob
  as additions would balloon the diff. Mitigation: skip files >1 MB in the
  untracked-as-additions branch and emit a synthetic
  `Binary file <path> not shown` line, matching `git diff`'s default behavior
  for binary content.

---

## Testing strategy

Same hard rule: TDD red→green→refactor, ≥1 unit test + ≥1 integration test per
command. The 95% coverage gate applies. `tests/e2e/phase-2a/` arrives in the
frontend plan.

---

## Checklist (high level)

- [ ] Task 1 — `workspace_diff` command + tests merged green
- [ ] Task 2 — `workspace_files` command + tests merged green
- [ ] Task 3 — `workspace_search` command + tests merged green
- [ ] All three commands registered in `lib.rs`
- [ ] `cargo clippy --lib --all-targets -- -D warnings` clean
- [ ] Coverage on changed files ≥ 95%
