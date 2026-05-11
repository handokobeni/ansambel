# Phase 2b — Interactive Surfaces (Frontend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Two new heavy components (xterm.js terminal + CodeMirror 6 editor),
one new tab + script picker, plus the file-click → editor wire that 2a deferred.
The terminal and editor both obey the hidden-mount hard rule because their
internal buffers (xterm scrollback, editor undo stack) are precious.

**Architecture:** Tab strip from 2a grows from 3 to 5 tabs (Chat / Diff / Files
/ Editor / Terminal). The Editor tab is empty until a file gets opened (via
FileBrowser file-click or search jump with line number). Multiple files can be
open at once — sub-tabs inside Editor panel manage them, with workspace-scoped
state. Terminal tab hosts an xterm.js instance per workspace, plus a script
picker dropdown. Script output streams into the same terminal panel as
interactive shell output.

**Tech stack:** Two new heavyweight deps:

- `@xterm/xterm` — terminal renderer (~150 KB)
- `@xterm/addon-fit` — auto-fit on container resize
- `codemirror` (@codemirror/{view,state,commands,language,language-data,
  lint,search})
- `@codemirror/lang-*` for the languages we want syntax highlighting for
  (typescript, rust, python, go, json, markdown, html, css, php, java) — picked
  by extension at open time

---

## Dependency additions

```json
{
  "@xterm/xterm": "^5.5.0",
  "@xterm/addon-fit": "^0.10.0",
  "codemirror": "^6.0.1",
  "@codemirror/view": "^6.34.0",
  "@codemirror/state": "^6.4.0",
  "@codemirror/commands": "^6.6.0",
  "@codemirror/language": "^6.10.0",
  "@codemirror/language-data": "^6.5.0",
  "@codemirror/lang-javascript": "^6.2.2",
  "@codemirror/lang-rust": "^6.0.1",
  "@codemirror/lang-python": "^6.1.6",
  "@codemirror/lang-json": "^6.0.1",
  "@codemirror/lang-markdown": "^6.3.0",
  "@codemirror/lang-html": "^6.4.9",
  "@codemirror/lang-css": "^6.3.0",
  "@codemirror/lang-php": "^6.0.1",
  "@codemirror/theme-one-dark": "^6.1.2"
}
```

Pinning to current stable major versions; ESM-first. Bundle size budget per
design spec is <30 MB binary, <150 MB idle RAM — xterm + CodeMirror together add
~600 KB minified+gzipped to the frontend chunk, well within budget.

---

## File Structure

```
src/lib/
├── ipc.ts                                # MODIFY: api.terminal.* + api.file.* + api.script.*
├── types.ts                              # MODIFY: TerminalChunk, OpenFile, RepoScript
├── stores/
│   ├── workspace-tabs.svelte.ts          # MODIFY: extend TabId to include 'editor' | 'terminal'
│   ├── editor-tabs.svelte.ts             # CREATE: per-workspace open files + active file
│   └── editor-tabs.svelte.test.ts        # CREATE
├── components/
│   ├── workspace/
│   │   ├── TabStrip.svelte               # MODIFY: 5 tabs (Chat / Diff / Files / Editor / Terminal)
│   │   ├── TabStrip.test.ts              # MODIFY
│   │   ├── WorkspaceView.svelte          # MODIFY: 5 hidden-mount panels
│   │   ├── WorkspaceView.test.ts         # MODIFY
│   │   ├── FileBrowser.svelte            # MODIFY: file-click → open in editor
│   │   ├── Editor.svelte                 # CREATE: CodeMirror wrapper
│   │   ├── Editor.test.ts                # CREATE
│   │   ├── EditorTabBar.svelte           # CREATE: open-file sub-tabs
│   │   ├── EditorTabBar.test.ts          # CREATE
│   │   ├── Terminal.svelte               # CREATE: xterm.js wrapper
│   │   ├── Terminal.test.ts              # CREATE
│   │   ├── ScriptPicker.svelte           # CREATE: dropdown of repo scripts
│   │   └── ScriptPicker.test.ts          # CREATE
└── lang.ts                               # CREATE: extension → CodeMirror language map
```

---

## Task 1: IPC bindings + types [P0]

**Why:** Same as 2a — typed wrappers and shared discriminated unions before any
component can call the new commands.

- `api.terminal.spawn(workspaceId, channel, cols?, rows?)`
- `api.terminal.write(workspaceId, bytes)` — bytes as `Uint8Array`
- `api.terminal.resize(workspaceId, cols, rows)`
- `api.terminal.kill(workspaceId)`
- `api.terminal.reattach(workspaceId, channel)`
- `api.file.read(workspaceId, path)` → `{ content, is_binary, size, sha1 }`
- `api.file.write(workspaceId, path, content, expectedSha1)`
- `api.script.list(repoId)`, `api.script.set(repoId, scripts)`,
  `api.script.run(workspaceId, scriptId, channel)`

**Tests:**

- [ ] Type-only check via `bun run check`.
- [ ] One per `api.*` call: invoke is wired with the right command name.

---

## Task 2: `editor-tabs` store [P0]

**Why:** Multiple files can be open in the Editor panel at once. Each workspace
needs its own list of `{ path, content, dirty, sha1 }` plus the currently active
file. SvelteMap keyed by workspace id; each value holds an ordered list of open
files and an active path.

```ts
type OpenFile = {
  path: string;
  content: string; // current buffer
  diskSha1: string; // sha1 from last successful read/write
  isBinary: boolean; // true → editor refuses, shows placeholder
  dirty: boolean; // content != lastSavedContent
};

type EditorState = {
  open: OpenFile[];
  active: string | null; // path or null when no file is open
};
```

Methods: `openFile(wsId, path, content, sha1, isBinary)`,
`setActive(wsId, path)`, `updateContent(wsId, path, content)`,
`markSaved(wsId, path, newSha1)`, `closeFile(wsId, path)`.

**Tests (TDD order):**

- [ ] `defaults_to_no_open_files`.
- [ ] `openFile_appends_unique_paths_at_end`.
- [ ] `openFile_re-activates_existing_open_file_without_duplicating`.
- [ ] `setActive_updates_active_path`.
- [ ] `updateContent_marks_dirty_when_content_diverges_from_saved`.
- [ ] `markSaved_clears_dirty_and_updates_diskSha1`.
- [ ] `closeFile_removes_and_picks_neighbour_as_active`.
- [ ] `closeFile_when_last_open_resets_active_to_null`.
- [ ] `forget_clears_workspace_state`.

---

## Task 3: `lang.ts` extension → language map [P0]

Pure helper that maps a file extension to a CodeMirror `LanguageSupport`
extension. Used by Editor on open. Falls back to "no language" for unknown
extensions.

**Tests:**

- [ ] `langForPath_returns_javascript_for_ts_tsx_js_jsx_mjs`.
- [ ] `langForPath_returns_rust_for_rs`.
- [ ] `langForPath_returns_python_for_py`.
- [ ] `langForPath_returns_json_for_json`.
- [ ] `langForPath_returns_markdown_for_md`.
- [ ] `langForPath_returns_html_for_html_htm`.
- [ ] `langForPath_returns_css_for_css_scss`.
- [ ] `langForPath_returns_php_for_php`.
- [ ] `langForPath_returns_null_for_unknown_extension`.

---

## Task 4: `Editor.svelte` (CodeMirror wrapper) [P0]

**Why:** The actual editing surface. Mounted once per workspace; the underlying
`EditorView` swaps document state between open files via
`view.dispatch({ changes: ... })` rather than remount, so undo history survives
file switches.

**Behavior:**

- On mount: create `EditorView` with `oneDark` theme + base extensions (history,
  line numbers, search, folding).
- On `activeFile` change: swap `state` to a fresh `EditorState.create` with the
  file's content + the right language.
- Edit events update the `editor-tabs` store via `updateContent`.
- `Ctrl+S` triggers `save()` which calls `api.file.write`. On
  `FileChangedOnDisk` rejection, surface a toast + offer "Reload" / "Overwrite"
  buttons. On success, `markSaved`.
- Empty state when `active === null`: "Open a file from the Files tab"
  placeholder.
- Binary file: refuse to render the editor body, show "Binary file — preview not
  available."

**Tests (TDD order):**

- [ ] `renders_empty_state_when_no_file_open`.
- [ ] `renders_editor_with_file_content_when_file_is_active`.
- [ ] `applies_language_extension_for_known_extension`.
- [ ] `typing_updates_open-file_content_in_store`.
- [ ] `ctrl_s_invokes_file_write_with_expected_sha1`.
- [ ] `ctrl_s_marks_file_clean_after_successful_save`.
- [ ] `ctrl_s_surfaces_FileChangedOnDisk_as_toast_with_action`.
- [ ] `switching_active_file_swaps_doc_without_remount`.
- [ ] `binary_file_renders_placeholder_not_editor`.
- [ ] `clears_listeners_on_unmount` (memory leak guard).

---

## Task 5: `EditorTabBar.svelte` [P0]

**Why:** Sub-tabs inside the Editor panel — one button per open file with a
dirty marker (•) and a close ✕. Clicking switches active file.

**Tests:**

- [ ] `renders_one_button_per_open_file`.
- [ ] `marks_dirty_files_with_a_dot_indicator`.
- [ ] `clicking_a_button_calls_setActive`.
- [ ] `clicking_close_calls_closeFile_and_does_not_propagate_to_button`.
- [ ] `confirms_close_when_file_is_dirty` (window.confirm — covered with test
      mock).

---

## Task 6: `Terminal.svelte` (xterm.js wrapper) [P0]

**Why:** Renders raw bytes from `terminal_spawn` into an xterm.js buffer. Per
the hard rule, the xterm `Terminal` instance is owned by this component for the
lifetime of the workspace — never unmount, only hide via `display:none`.

**Behavior:**

- On mount: create `Terminal` + `FitAddon`, attach to ref, call
  `api.terminal.reattach` first (in case backend already has a session); on
  rejection, fall back to `api.terminal.spawn`.
- `terminal.onData(bytes => api.terminal.write(wsId, bytes))` — every keystroke
  goes through IPC.
- ResizeObserver on the container → `fit.fit()` then
  `api.terminal.resize(wsId, cols, rows)`.
- Channel handler: `terminal.write(chunk.bytes)` for byte chunks; on `Exited`
  chunk render an inline marker line ("[process exited with code N]") and stop
  accepting input until next spawn.
- Unmount: detach the ref but keep the xterm instance + the backend session
  alive (the broadcaster keeps streaming into a queue if no consumer; reattach
  on next mount drains it).

**Tests:**

- [ ] `mounts_an_xterm_instance_into_the_container`.
- [ ] `calls_reattach_first_then_spawn_on_failure`.
- [ ] `pipes_keyboard_input_through_terminal_write`.
- [ ] `resize_observer_calls_terminal_resize_on_container_size_change`.
- [ ] `renders_streamed_bytes_into_xterm_buffer`.
- [ ] `renders_exited_marker_when_session_ends`.
- [ ] `survives_workspace_switch_via_hidden_mount` — mount + hide + remount
      equivalence.

---

## Task 7: `ScriptPicker.svelte` [P0]

**Why:** Dropdown attached to the Terminal tab header that lists the repo's
scripts. Selecting a script invokes `script_run` and the output streams into the
same terminal buffer.

**Behavior:**

- On mount: `api.script.list(repoId)`. Empty list → "No scripts configured. Edit
  repo settings to add one." (links to settings UI when that surface ships in
  Phase 8).
- Click script → `api.script.run(wsId, scriptId, channel)`. Channel forwards
  bytes to the same Terminal component already mounted.

**Tests:**

- [ ] `renders_no-scripts_placeholder_when_repo_has_none`.
- [ ] `renders_one_button_per_script_with_its_name`.
- [ ] `clicking_a_script_invokes_script_run_with_correct_ids`.

---

## Task 8: Wire FileBrowser file-click → open editor [P0]

**Why:** Phase 2a left this as a no-op (`onOpen` only highlighted the row). Now
we actually open the file.

**Behavior change:**

- WorkspaceView's `onOpen={...}` callback (passed to FileBrowser):
  `await api.file.read(wsId, path)`. On success, dispatch into the `editor-tabs`
  store via `openFile(...)`, then switch active tab to Editor.
- Search jump with a line number additionally calls
  `view.dispatch({ selection: ..., effects: scrollIntoView })` after the file is
  open.

**Tests:**

- [ ] `clicking_a_file_in_FileBrowser_calls_file_read`.
- [ ] `successful_read_opens_file_in_editor_tabs_store`.
- [ ] `read_failure_surfaces_toast_without_changing_tabs`.
- [ ] `tab_switches_to_editor_after_successful_open`.

---

## Task 9: TabStrip + WorkspaceView extension [P0]

- TabStrip grows to 5 buttons (Chat / Diff / Files / Editor / Terminal). Each
  gets a `⌃<n>` shortcut.
- WorkspaceView grows to 5 hidden-mount panels. Editor + Terminal panel
  visibility flips `display:none/block` like the others, but the internal Editor
  and Terminal components are NEVER unmounted across workspace lifetime.

**Tests:**

- [ ] `TabStrip_renders_5_tabs_with_correct_labels`.
- [ ] `Ctrl_4_activates_editor_tab` (App-level shortcut wiring).
- [ ] `Ctrl_5_activates_terminal_tab`.
- [ ] `editor_panel_remains_in_DOM_when_other_tab_is_active`.
- [ ] `terminal_panel_remains_in_DOM_when_other_tab_is_active`.

---

## Risks

- **xterm.js size + cold-start latency** — first render of the terminal tab can
  stall while ~150 KB of xterm parses. Mitigation: lazy-import via
  `await import('@xterm/xterm')` so the splash + Plan mode aren't penalized.
  Already a known pattern.
- **CodeMirror + Svelte 5 Reactivity** — CM owns its DOM; Svelte must not
  re-render it. Use a single `<div>` ref + `onMount` to instantiate once.
  `$effect` watches `activeFile` and dispatches `view.dispatch` state changes
  inside `untrack(...)` to avoid the effect-loop trap that Phase 2a's
  FileBrowser hit.
- **Atomic save mid-Claude-edit** — the agent and the user editing the same file
  simultaneously. The `expected_sha1` race-detect catches it on the backend; the
  editor's role is to surface the conflict and let the user choose. Not silently
  overwrite.
- **WebKitGTK font fallback for xterm** — Linux WebKit may render box- drawing
  characters poorly. Mitigation: bundle a Nerd Font subset fallback for the
  terminal; fall back to system mono only after that fails. Keep this in the
  design spec as a known issue.

---

## Testing strategy

Same hard rule: TDD, ≥1 happy path + ≥1 edge case per component, 95% coverage.
Mocks of `@xterm/xterm` and CodeMirror in unit tests use a minimal stub (a
`class MockTerminal { open(); write(); resize(); }`). The real binding is
exercised by E2E.

`tests/e2e/phase-2b/`:

- [ ] `phase2b-editor.spec.ts` — workspace open → Files → click a file → Editor
      tab activates with content → type → Ctrl+S → re-read from disk asserts new
      content.
- [ ] `phase2b-terminal.spec.ts` — Terminal tab → type `pwd` Enter → output
      contains worktree path.
- [ ] `phase2b-script.spec.ts` — set a script via `script_set` → click script in
      dropdown → output streams into terminal.

---

## Checklist (high level)

- [ ] Task 1 — IPC bindings + types
- [ ] Task 2 — `editor-tabs` store
- [ ] Task 3 — `lang.ts` helper
- [ ] Task 4 — Editor component
- [ ] Task 5 — EditorTabBar
- [ ] Task 6 — Terminal component
- [ ] Task 7 — ScriptPicker
- [ ] Task 8 — FileBrowser file-click → open in editor
- [ ] Task 9 — TabStrip + WorkspaceView 5-tab extension
- [ ] All E2E specs pass on Ubuntu + Windows runners
- [ ] Coverage on changed files ≥ 95%
- [ ] Journal entry describes the sub-phase
- [ ] PR opened against `main`
