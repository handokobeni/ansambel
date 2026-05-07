# Phase 2a — Read-only Work Mode (Frontend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single-panel `WorkspaceView` (chat-only) with a tab strip
that hosts Chat, Diff, Files, and a Search modal. Each tab is a hidden-mount
panel — switching tabs flips `display`, never unmounts — matching the hard rule
that's already in place for xterm and the incoming CodeMirror in 2b.

**Architecture:** Tab state lives in the workspace store, keyed per workspace.
The Chat tab continues to use today's `ChatPanel`. Diff and Files become two new
components fed by the three new commands the backend plan introduces. Search is
a global modal triggered by `Ctrl+P` (filename) and `Ctrl+Shift+F` (content),
using the same backend command with a different `mode`.

**Tech Stack:** Svelte 5 runes, Tailwind v4, no new heavy deps. Diff rendering
reuses the existing `highlight.js` registry from `src/lib/markdown.ts` for
line-level syntax color. No diff parser needed beyond a small `parseUnifiedDiff`
helper that splits text into hunks.

---

## Dependency additions

None. We already have `highlight.js` from Phase 1 markdown work; the
unified-diff parser is small enough to live in `src/lib/diff.ts`.

---

## File Structure

```
src/lib/
├── diff.ts                                # CREATE: parseUnifiedDiff helper + types
├── diff.test.ts                           # CREATE: parser unit tests
├── ipc.ts                                 # MODIFY: api.workspace.diff/files/search
├── stores/
│   ├── workspace-tabs.svelte.ts           # CREATE: per-workspace activeTab + open paths
│   └── workspace-tabs.svelte.test.ts      # CREATE: tab store tests
├── components/
│   ├── workspace/
│   │   ├── WorkspaceView.svelte           # MODIFY: tab strip + hidden-mount panels
│   │   ├── WorkspaceView.test.ts          # MODIFY: tab switching + hidden-mount
│   │   ├── TabStrip.svelte                # CREATE: tab strip UI
│   │   ├── TabStrip.test.ts               # CREATE
│   │   ├── DiffView.svelte                # CREATE: streamed diff rendering
│   │   ├── DiffView.test.ts               # CREATE
│   │   ├── FileBrowser.svelte             # CREATE: lazy tree
│   │   ├── FileBrowser.test.ts            # CREATE
│   │   ├── SearchModal.svelte             # CREATE: global ctrl+p modal
│   │   └── SearchModal.test.ts            # CREATE
│   └── App.svelte                         # MODIFY: mount <SearchModal /> + bind shortcuts
└── types.ts                               # MODIFY: DiffChunk, FileEntry, SearchHit, WorkspaceTab
```

---

## Task 1: Add IPC bindings + shared types [P0]

**Why:** Frontend needs typed wrappers around the three new Tauri commands
before any component can call them, and the rune-based stores need the shared
types to model state.

**Tests (TDD order):**

- [ ] Type-only check via `bun run check` — failing if
      `DiffChunk |     FileEntry | SearchHit` aren't exported.
- [ ] `ipc.workspace.diff_returns_channel` — mock `invoke` to verify the
      `Channel` is wired and the command name is `workspace_diff`.
- [ ] Same for `files` and `search`.

**Files:**

- Modify: `src/lib/types.ts` — add the three discriminated unions mirroring the
  Rust `#[serde(tag = "kind")]` enums.
- Modify: `src/lib/ipc.ts` — `api.workspace.diff(workspaceId, channel)`,
  `api.workspace.files(workspaceId, path)`,
  `api.workspace.search(args, channel)`.

---

## Task 2: `workspace-tabs` store [P0]

**Why:** Tab state must persist across workspace switches without re-fetching
diff/files. A `SvelteMap<workspaceId, TabState>` lets each workspace remember
its active tab + which file paths are "open" in the file-browser tree (expanded
directories).

**Shape:**

```ts
type TabId = 'chat' | 'diff' | 'files';
type TabState = {
  active: TabId;
  expanded: SvelteSet<string>; // expanded directory paths in FileBrowser
};
```

**Tests (TDD order):**

- [ ] `workspace_tabs_defaults_to_chat`.
- [ ] `setActive_changes_active_tab`.
- [ ] `expanded_paths_persist_across_active_tab_changes`.
- [ ] `removing_workspace_clears_its_tab_state`.

---

## Task 3: `TabStrip` component [P0]

**Why:** Pure UI — the dumb tab buttons. Receives `active` + `onSelect` callback
and renders three buttons with focus-ring + aria-selected.

**Tests (TDD order):**

- [ ] Renders three buttons with the right labels.
- [ ] Active tab has `aria-selected="true"`.
- [ ] Click on a non-active tab fires `onSelect(tabId)`.
- [ ] `Ctrl+1/2/3` keyboard shortcut also fires `onSelect`.

---

## Task 4: `DiffView` component [P0]

**Why:** Renders unified-diff text with hljs syntax color per language and
green/red row highlights. Streams chunks from the backend Channel and appends
them to a buffer; final render parses the full buffer once `eof` arrives
(parsing per-chunk is brittle because a hunk header can split across chunks).

**Behavior:**

- On mount: `api.workspace.diff(workspaceId, channel)`. Show a "Loading…" state
  while accumulating chunks.
- On `eof`: hand the buffer to `parseUnifiedDiff` → array of file entries →
  render each with header (path) + per-hunk colored lines.
- Empty diff: render "No uncommitted changes." (matches korlap UX).
- Re-runs on workspace switch (Diff tab is freshly mounted per workspace open —
  this is fine for read-only fetches; expensive panels like CodeMirror in 2b
  will obey the hidden-mount rule).
- Provides a "Refresh" button that re-invokes the command.

**Tests (TDD order):**

- [ ] `renders_loading_state_initially`.
- [ ] `renders_empty_state_for_clean_worktree`.
- [ ] `renders_one_file_block_per_diff_entry`.
- [ ] `colors_added_lines_green_and_removed_lines_red`.
- [ ] `applies_hljs_class_for_known_language` (e.g. `.ts` → hljs language tag).
- [ ] `handles_chunked_arrival` (verifies parsing works when chunks split a
      hunk).
- [ ] `re_invokes_on_refresh_click`.
- [ ] `surfaces_error_chunk_as_inline_banner`.

---

## Task 5: `parseUnifiedDiff` helper [P0]

**Why:** Pure function — easy to TDD without DOM. Splits unified diff text into:

```ts
type ParsedDiff = {
  files: Array<{
    path: string;
    oldPath: string;
    newPath: string;
    isBinary: boolean;
    hunks: Array<{
      oldStart: number;
      oldLines: number;
      newStart: number;
      newLines: number;
      lines: Array<{ kind: 'add' | 'del' | 'ctx'; text: string }>;
    }>;
  }>;
};
```

**Tests (TDD order):**

- [ ] `parses_single_file_with_one_hunk`.
- [ ] `parses_multi_file_diff`.
- [ ] `marks_binary_files_with_isBinary_true`.
- [ ] `treats_unparseable_section_as_pass_through_context`.
- [ ] `handles_empty_input` (returns `{ files: [] }`).

---

## Task 6: `FileBrowser` component [P0]

**Why:** Lazy tree of the worktree. Each directory is a `<details>`-style
expand/collapse; clicking a directory invokes `api.workspace.files(id, path)`
and replaces children. The expanded set lives in the workspace- tabs store so
switching tabs and back keeps tree state.

**Behavior:**

- On mount: load root via `api.workspace.files(id, "")`.
- Click a directory: toggle expand. On expand, fetch children if not yet cached.
- Click a file: emit `onOpen(relPath)` — for 2a this is wired to the Search
  modal "click hit → open file in tree" flow only; the editor arrives in 2b.
  Until then a file-click highlights the row and is a no-op.
- Indent children 16 px per depth, chevron ▸ / ▾ for dirs.

**Tests (TDD order):**

- [ ] `renders_root_entries_after_load`.
- [ ] `expands_directory_on_click_and_loads_children`.
- [ ] `collapses_directory_on_second_click`.
- [ ] `expanded_paths_persist_via_workspace_tabs_store`.
- [ ] `file_click_invokes_onOpen_callback_with_path`.
- [ ] `surfaces_load_error_as_inline_message_per_directory`.

---

## Task 7: `SearchModal` component [P0]

**Why:** Global Ctrl+P / Ctrl+Shift+F modal. Streams hits from the search
command, renders a list, click-to-jump scrolls FileBrowser to the hit file (and
expands ancestor directories on the way).

**Behavior:**

- Opens via global keybind. Two tabs inside the modal: "Files" (filename match)
  and "Content" (rg). Default to Files. Shift toggles content.
- Search-on-enter (not as-you-type) — keeps the channel cancellation surface
  small for 2a; debounced as-you-type can come in polish later.
- Renders results as a virtualized list (already-built `VirtualScroller` utility
  from Phase 1e? — confirm; if absent, skip virtualization for 2a and add later.
  Bound results to 500 hits at the backend, well within an unvirtualized list's
  perf budget).
- Click hit → close modal + emit `onJump(path, line?)` → `WorkspaceView`
  switches to Files tab + expands ancestors + highlights the row. (Line jump in
  editor is 2b territory — for now we just highlight the file row in the tree.)
- Shows the "ripgrep not found" CTA when the unavailable sentinel arrives: a
  one-line banner with a link to the install docs URL.

**Tests (TDD order):**

- [ ] `does_not_render_when_open_is_false`.
- [ ] `focus_traps_to_input_on_open`.
- [ ] `escape_closes_modal`.
- [ ] `enter_invokes_search_with_filename_mode_by_default`.
- [ ] `mode_toggle_swaps_to_content_search`.
- [ ] `renders_results_streamed_from_channel`.
- [ ] `click_hit_calls_onJump_with_path_and_line`.
- [ ] `shows_install_rg_banner_when_unavailable_sentinel_arrives`.
- [ ] `cleans_up_channel_handler_on_close`.

---

## Task 8: Wire tabs into `WorkspaceView` [P0]

**Why:** This is where everything assembles. The chat-only `WorkspaceView`
becomes a tab host: header (already there) + `<TabStrip />` + a stack of panels
switched via `class:hidden`.

**Hidden-mount pattern:**

```svelte
<div class="flex-1 overflow-hidden relative">
  <div class:hidden={tab !== 'chat'} class="absolute inset-0">
    <ChatPanel ... />
  </div>
  <div class:hidden={tab !== 'diff'} class="absolute inset-0">
    <DiffView workspaceId={workspace.id} />
  </div>
  <div class:hidden={tab !== 'files'} class="absolute inset-0">
    <FileBrowser workspaceId={workspace.id} onOpen={...} />
  </div>
</div>
```

The Chat panel keeps its own scroll position when the user pops to Diff and back
— exactly the regression that the hidden-mount rule prevents elsewhere.

**Tests (TDD order):**

- [ ] `renders_tab_strip_alongside_existing_header`.
- [ ] `default_tab_is_chat`.
- [ ] `clicking_diff_tab_renders_DiffView` (mock its module).
- [ ] `clicking_files_tab_renders_FileBrowser`.
- [ ] `chat_panel_remains_in_DOM_when_diff_tab_is_active` — assert the ChatPanel
      root element is still present, just `hidden`.
- [ ] `tab_state_persists_across_workspace_switches`.

---

## Task 9: Mount `<SearchModal />` in App.svelte [P0]

**Why:** SearchModal is global, not per-workspace. Pulls `currentWorkspace` from
the workspace store; when a hit is clicked, dispatches a workspace- tab change
to "files" and stamps the path into the FileBrowser's "selected" slot.

**Tests (TDD order):**

- [ ] `ctrl_p_opens_modal_in_filename_mode`.
- [ ] `ctrl_shift_f_opens_modal_in_content_mode`.
- [ ] `escape_closes_modal_globally`.
- [ ] `clicking_hit_switches_active_tab_to_files`.

---

## Risks

- **Diff streaming completes mid-render** — if the user clicks Diff, then
  Refresh while the previous stream is in flight, both stream handlers will
  write to the same buffer. Mitigation: each `Channel` invocation gets a unique
  generation id; chunks from a stale generation are dropped.
- **Content search is slow on huge repos** — ripgrep with
  `--max-count 100 --max-filesize 1M` caps work, but the modal may show no
  progress for several seconds. Mitigation: a header counter (`12 hits…`)
  updates as chunks arrive, so the user sees liveness.
- **Path traversal via UI** — the FileBrowser only renders paths the backend
  returns, so traversal is impossible from the UI alone. The backend's
  canonicalize check is the real gate.

---

## Testing strategy

Same hard rule: TDD, ≥1 happy path + ≥1 edge case per component, 95% coverage on
changed lines/branches/functions/statements.

`tests/e2e/phase-2a/`:

- [ ] `phase2a-diff.spec.ts` — workspace open → Diff tab → see green/red hunks
      for a modified file in the mock worktree.
- [ ] `phase2a-files.spec.ts` — workspace open → Files tab → expand a directory
      → see children.
- [ ] `phase2a-search.spec.ts` — Ctrl+P → type filename → see hit → click →
      Files tab opens with that path highlighted.

---

## Checklist (high level)

- [ ] Task 1 — IPC bindings + types added
- [ ] Task 2 — `workspace-tabs` store with full coverage
- [ ] Task 3 — `TabStrip` shipped
- [ ] Task 4 — `DiffView` shipped
- [ ] Task 5 — `parseUnifiedDiff` helper shipped
- [ ] Task 6 — `FileBrowser` shipped
- [ ] Task 7 — `SearchModal` shipped
- [ ] Task 8 — `WorkspaceView` tab integration shipped
- [ ] Task 9 — `<SearchModal />` mounted in App.svelte
- [ ] All E2E specs pass on Ubuntu + Windows runners
- [ ] Coverage on changed files ≥ 95%
- [ ] Journal `journal/2026-XX-XX.md` describes the sub-phase
- [ ] PR opened against `main`
