# Journal — 2026-05-19 — Phase 3a-4 team activity sidebar + watch view

## What shipped

Phase 3a-4 reads the dedicated `ansambel_team_activity` Bitable (the table Phase
3a-3 publishes to) and surfaces two UI surfaces:

- **Sidebar panel** below WORKSPACES — collapsible list of active team
  workspaces for the user's local repos, grouped by repo, with status dot +
  assignee + task title + relative last-activity time.
- **Mirror view** — replaces the main content when a row is clicked. Read-only:
  status / assignee / branch + GitHub branch link / last message preview / Open
  PR button when `ansambel_status == pr_ready` AND `pr_url` is set.

The backend exposes one new Tauri command `fetch_team_activity_rows` that builds
a `FilterSpec` internally from `AppState.repos` +
`team_activity_config.machine_label`, runs `git remote get-url origin` once per
repo (cached), calls `bitable_search_records`, and returns a tagged
`FetchResult` enum. Frontend polls every 10s with
`document.visibilityState`-aware pause.

## Backend

- `commands/team_activity.rs` (extended): added `TeamActivityRow` +
  `FetchResult` enum + `parse_record_to_row` parser + `read_remote_url_cached` +
  `fetch_team_activity_rows_inner` (pure-Rust testable core) +
  `fetch_team_activity_rows` Tauri command. The reader's canonical-URL cache is
  process-wide static (`OnceLock<Arc<Mutex<HashMap<RepoId, String>>>>`) and
  distinct from the publisher's enricher cache so read and write paths don't
  entangle.
- Filter construction inverts the original 3a-4 sketch: rather than the frontend
  building the FilterSpec, the backend does. The canonical `repo_remote_url` is
  computed by Rust (publisher enricher precedent) and stays out of the
  frontend's mental model. The IPC contract is argument-free:
  `api.teamActivity.fetchRows()`.

## Frontend

- `src/lib/types.ts`: added `TeamActivityRow` + `FetchResult` matching the Rust
  shapes via `serde(tag = "kind", rename_all = "snake_case")`.
- `src/lib/ipc.ts`: extended `teamActivity` namespace with `fetchRows()`
  wrapper.
- `src/lib/stores/team-activity.svelte.ts` (NEW): `TeamActivityStore` owns the
  polling loop. `start()` fires an immediate fetch + 10s recursive setTimeout
  cadence + `visibilitychange` listener (immediate fetch on tab focus). `stop()`
  clears both. `inflight` guard prevents overlapping ticks. Reconcile
  auto-closes the mirror view when the selected row disappears server-side.
- `src/lib/github-url.ts` (NEW): pure helper that converts the publisher's
  canonical remote URL (`https://...` or `git@...`) into a `/tree/<branch>` URL,
  returning `null` for unsupported schemes so the link can be hidden instead of
  dead.
- `src/lib/components/sidebar/TeamActivityPanel.svelte` (NEW): collapsible
  section mounted in `Sidebar.svelte` below the workspaces list. Groups rows by
  repo, renders status dot + assignee + task title + relative time.
  Empty/disabled/error states cover every `FetchResult` variant. Collapse state
  persists to localStorage.
- `src/lib/components/team/TeamWorkspaceMirror.svelte` (NEW): the watch view.
  Header (task title + assignee + status + relative time + back button), Code
  state section (branch badge + GitHub link + diff summary or "Not yet
  published" placeholder), Latest activity section (sanitised message preview in
  `<pre>` + Open PR button when applicable).
- `src/lib/components/TitleBar.svelte` (extended): when
  `teamActivity.selectedWorkspaceId !== null`, replaces the Plan/Work toggle
  with "Watching: {assignee} @ {task}" + a back button.
- `src/App.svelte` (extended): routes to `TeamWorkspaceMirror` when
  `selectedWorkspaceId` is set, otherwise falls back to the existing Plan/Work
  conditional.
- `src/lib/components/Sidebar.svelte` (extended): mounts `TeamActivityPanel` and
  drives `teamActivity.start()` / `stop()` through `onMount` / `onDestroy`.

## Architectural decisions

- **Backend-side filter construction.** The canonical `repo_remote_url` cache
  lives in Rust. Surfacing it to the frontend just so JS could rebuild the
  filter would be either extra IPC round-trips or a serialised field that trails
  the cache. Backend builds the filter, frontend calls `fetchRows()` with no
  args.
- **`assignee_machine isNotEmpty` instead of `private isNot true`.** The
  publisher's privacy escape (3a-3) clears `assignee_machine` along with the
  other sensitive columns when a workspace goes private. An empty
  `assignee_machine` is therefore the canonical "should not appear in sidebar"
  signal, and the text-field `isNotEmpty` operator is unambiguously documented
  in Lark's filter API (whereas the boolean-checkbox filter semantics are not).
- **10s polling + visibility pause.** Lark Bitable rate limit is 200 req/min per
  app. The publisher alone can use up to 200 req/min in the worst case. 10s
  ticks at 6 req/min per engineer give the publisher headroom, and
  `document.visibilityState` pause cuts inactive engineers to zero.
- **Mirror view = main-content replacement, not modal.** Keeps consistent
  navigation (TitleBar shows context, back button returns to the prior mode) and
  gives the watch view space to breathe.
- **GitHub link as escape hatch for missing `diff_summary` / `pr_url`.** Both
  columns are deferred to Phase 3a-5/6 (no commit/push/PR-create handler exists
  yet), but the canonical remote + branch is enough to construct a
  `/tree/<branch>` URL. Self-hosted git remotes that don't match GitHub's shape
  get a hidden link rather than a dead one.
- **Recursive setTimeout instead of setInterval for polling.** vitest 2.x fake
  timers double-fire `setInterval` callbacks under `runOnlyPendingTimersAsync`.
  The store re-schedules itself with `setTimeout` in `.finally()` so each tick
  fires exactly once.

## Followups deferred

- `diff_summary` and `pr_url` column population — wired in 3a-3 but no emitter
  exists. Phase 3a-5/6 or 3a-8 is the natural place to add a
  commit/push/PR-create surface that emits these events.
- Notifications when teammate status flips to `blocked` or `pr_ready` — Phase
  3a-6 (Lark IM ping).
- Per-table membership UI — single-table assumption holds for now; Phase 3a-7 if
  a team needs multiple Bitable tables.
- Handoff bundles — Phase 3a-8.
- Provider abstraction for non-GitHub remotes (GitLab / Bitbucket / Forgejo).
  The `githubBranchUrl` helper is GitHub-shaped today; refactor when a
  non-GitHub user needs it.

## Tests

- **Rust**: +36 unit tests covering parser, remote-URL cache, fetch_inner
  FetchResult variants, and Lark integration (wiremock).
- **TS**: +63 vitest cases covering store FetchResult mapping, reconciliation,
  poll lifecycle, panel rendering states, mirror view branch URL construction +
  back button + auto-close.
- **E2E**: 5 env-gated cases (`ANSAMBEL_LARK_FIXTURE=1`) covering the full
  sidebar → click → mirror → back round-trip plus the
  auto-close-on-row-disappear path and the disabled-config render.

## Aftermath

The publisher is now bi-directional: 3a-3 writes the row, 3a-4 reads it. The
"who is working on what" awareness loop closes for teams whose members all run
Ansambel against the same `team_activity_config.json`. Single-table assumption
holds; multi-table membership UI is Phase 3a-7.
