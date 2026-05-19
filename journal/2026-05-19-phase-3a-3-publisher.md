# Journal — 2026-05-19 — Phase 3a-3 workspace state publisher

## What shipped

Phase 3a-3 ships the workspace state publisher: a backend async task that
subscribes to a `broadcast::Sender<WorkspaceEvent>` on `AppState`, aggregates
events per-workspace with a 3-second debounce, sanitises message previews, and
upserts one row per workspace into a dedicated Lark Bitable table (separate from
the per-repo task bindings introduced in Phase 3a-3.1). The Settings → Team
Activity panel configures the publisher's destination + a per-workspace privacy
toggle blanks sensitive columns on demand. The read side (team-activity sidebar)
is the next phase. The branch (`feat/phase-3a-3-publisher`) landed 24 commits in
20 plan tasks.

## Backend

- `platform/repo_identity.rs` (commit 726d0e6, 5796ea3): canonical repo URL
  helper — strips trailing `.git`, lowercases the host only, trims whitespace.
  Used so engineer A's `repo_abc` and engineer B's `repo_xyz` for the same
  upstream resolve to the same row in the team-activity Bitable.
  `read_origin_url` shells out to `git -C <path> remote get-url origin` and
  returns an empty string (not an error) when origin is missing.
- `platform/sanitize.rs` (commits 68cf015, 3ad8aec): regex-set redactor. Catches
  OpenAI-style `sk-...`, `Bearer ...`, JWTs, and named credentials
  (`<token>=...`, `password: ...`). Applied to every text column on the outbound
  row before HTTP send. Mirrored in TS (`src/lib/sanitize.ts`, commit 3b6e6f9)
  so settings-side validation and the future read-side view can apply the same
  redaction client-side without a round-trip.
- `state.rs` (commit 65ea280): `WorkspaceEvent` enum (`StatusChanged`,
  `MessageAppended`, `FileTouched`, `BranchChanged`, `DiffSummaryUpdated`,
  `PrCreated`) + a `broadcast::Sender<WorkspaceEvent>` field on `AppState`.
  Capacity 1024 — sized so a 100-event burst from one workspace can't starve the
  others.
- `persistence/team_activity.rs` + `commands/team_activity.rs` (commit d03d827,
  plus 07412ae and 2c1ece0): atomic JSON persister for `TeamActivityConfig`
  (`{app_token, table_id, machine_label}`) at
  `<data_dir>/team_activity_config.json`. Empty-token semantics: the persister
  treats `app_token == ""` as "publisher disabled" and writes `null` to disk,
  which the IPC `get` returns as `null` to the frontend — used by the Disconnect
  button to drop the config without a separate command.
- `platform/lark_client.rs` (commits cdc59c1, 2bcfae2): new `bitable_upsert_row`
  calling
  `PUT /open-apis/bitable/v1/apps/{app}/ tables/{table}/records?user_id_type=open_id`
  keyed on the primary `workspace_id`. Wrapped in `send_with_retry`: 30s timeout
  per attempt, one retry on 429 honouring `Retry-After` (default 1s when
  absent), and surfacing the second 429 as an error so the publisher can drop
  the in-flight payload and pick up the next coalesced state on the following
  tick.
- `publisher/state_publisher.rs` (commits 98591f8, dbabc6c, 8dce127): the core
  async task. Owns a per-workspace `HashMap<WorkspaceId, PendingState>` with a
  private_lock semantics flag (private workspaces blank `last_message_preview`,
  `branch_name`, `diff_summary`, `pr_url` but still upsert the row so a teammate
  can see the workspace exists in the team's view). Three-second debounce
  coalesces all events for a given workspace into one upsert. The uploader trait
  is bound at startup to `LarkClient::bitable_upsert_row`; tests inject a
  recording mock.
- `lib.rs` (commit 5385e17): spawns the state_publisher at app startup once
  `AppState::lark_client` is ready. The task ignores events when
  `TeamActivityConfig` is unset, so the publisher is a no-op for users who
  haven't opted in — verified by Task 9 integration smoke.
- Event emission (commits 70be2d5, 1f9cff5, 34c1541, 203cda1): `agent_core`
  emits `StatusChanged` on every workspace status flip (`running` → `waiting` →
  etc.) and `MessageAppended` when an assistant message is persisted to disk.
  `commands/workspace.rs` emits `FileTouched` from `workspace_files_recursive`,
  `BranchChanged` from `branch_change`, and `DiffSummaryUpdated` from
  `workspace_diff`. `commands/pr.rs` emits `PrCreated` after `pr_create`
  succeeds. (Note: per the design spec, `DiffSummaryUpdated` and `PrCreated` are
  wired but the publisher does not yet consume them into the corresponding
  Bitable columns — they are future-deferred placeholders. See Followups.)

## Frontend

- `src/lib/types.ts` (commit 07412ae): added `TeamActivityConfig` (flat shape:
  `{app_token, table_id, machine_label}`) and an optional
  `team_activity_private?: boolean` on `Workspace`. Both serde-optional so
  pre-Task-18 workspaces deserialise without the field.
- `src/lib/ipc.ts` (commit 07412ae): new `api.teamActivity.get/set/ setupTable`
  wrappers and `api.workspace.setTeamActivityPrivate`.
- `src/lib/components/lark/TeamActivitySettings.svelte` (commit da13894): the
  3-field form (app_token / table_id / machine_label) plus a "Setup table
  schema" button (Task 16 IPC) and a Disconnect confirm chip. Save emits an
  optimistic toast — "Restart app to apply changes" — because the publisher only
  re-reads its config at boot today. Default machine label is `'me@machine'`;
  OS-derived default is a deferred follow-up.
- `src/lib/components/SettingsDialog.svelte` (commit da13894): mounts
  TeamActivitySettings below the existing LarkGlobalSettings panel, re-keyed on
  `open` so each open re-runs `onMount` and refreshes status.
- `src/lib/stores/workspaces.svelte.ts` (commit 39e30a1): optimistic
  `setTeamActivityPrivate(workspaceId, repoId, isPrivate)` — flips the in-store
  SvelteMap entry then calls the IPC; on failure reverts and toasts.
- `src/lib/components/workspace/WorkspaceView.svelte` (commit 39e30a1): privacy
  toggle in the workspace header reading the optimistic flag from the store.
  ARIA-pressed reflects state; `data-private` mirror attribute drives E2E
  selectors.

## Architectural decisions

- **Separate Bitable table** — the publisher writes to a dedicated team-activity
  table, not the per-repo task bindings introduced in Phase 3a-3.1. Mixing
  per-task rows with per-workspace state would contaminate the task search
  filters that 3a-3.1 ships and make schema evolution painful (e.g. adding
  `pr_url` to every customer's tasks table is a no-go).
- **Canonical repo URL via `git remote get-url origin`** — chose over a
  user-configurable repo ID so the same upstream resolves consistently across
  machines without engineers coordinating. Lowercased host / stripped `.git` /
  trimmed whitespace are the only normalisations.
- **3s debounce + 30s timeout + 429 retry-once** — the debounce caps a 100-event
  burst to one upsert; the 30s per-call timeout bounds tail latency; the single
  retry honouring `Retry-After` is enough for the occasional Lark rate-limit
  reload without us building a full back-off queue. Verified by the burst test
  in `state_publisher.rs`.
- **`private_lock` semantics** — flipping the privacy toggle blanks sensitive
  columns (`last_message_preview`, `branch_name`, `diff_summary`, `pr_url`) on
  the next upsert but keeps the row present so the team-activity sidebar still
  shows that the workspace exists. Removing the row entirely would race with
  privacy flips during a debounce window.
- **Per-workspace persisted privacy flag** —
  `Workspace.team_activity_ private: Option<bool>` rides the existing workspace
  JSON instead of a separate file. Optional + serde default so older workspaces
  hydrate without a migration.

## Followups deferred

- `diff_summary` stays blank — the publisher's column is wired (see
  `snapshot_to_fields` and the `DiffSummaryUpdated` variant) but no emission
  site exists. The clean trigger is a backend commit/push handler that doesn't
  exist yet (agents currently run `git commit` via the `Bash` tool, so Ansambel
  never sees the event). When Phase 3a-5/6 introduces a first-class commit/push
  surface — or when 3a-8 ships handoff bundles — wire `commands/diff.rs` to emit
  `DiffSummaryUpdated` with the `+45 -12 across 3 files` short-stat string. A
  workaround (`git diff --shortstat` shell-out in the publisher's flush path)
  was considered and rejected: every 3-second flush would shell out for every
  active workspace, and for large repos the cost outweighs the readability win.
- `pr_url` stays blank — same story. `PrCreated` is on the broadcast bus but
  there's no `gh pr create` Tauri handler (agents shell out). Wire when a proper
  PR-creation command lands.
- Enrichment refresh — `build_app_enricher` reads workspace + repo + task fields
  once per flush via the in-memory `remote_url_cache`. If the user re-binds a
  repo or renames a task mid-session, the cached canonical URL doesn't refresh
  until restart. Acceptable today (rename is rare); when this bites someone,
  swap the per-repo cache for a `tokio::sync::watch` feed off the existing
  repo/task persistence layer.
- Restartable publisher on config change — Save currently toasts "Restart app to
  apply changes" because the publisher's `TeamActivityConfig` is captured at
  spawn time. A `tokio::sync::watch<TeamActivityConfig>` channel would close
  that gap, but the wiring touches several modules; deferred.
- OS-derived `machine_label` default — today defaults to `"me@machine"` on first
  run. A tiny Rust helper returning `$USER@$(hostname)` (or the Windows
  equivalent) would make first-launch saner, especially for teams onboarding
  multiple engineers at once.

## Tests

- **Rust: 736 unit tests passing** — up from 663 at the start of the phase. New
  coverage:
  - Task 1 (commits 726d0e6, 5796ea3): `canonicalise_remote_url` table tests
    covering trailing `.git`, host case, empty, whitespace, SSH-style URLs.
  - Task 2 (commits 68cf015, 3ad8aec): sanitiser regex matrix (sk-, Bearer, JWT,
    password=, token=, case sensitivity, partial-match bleeding).
  - Task 3 (commit 65ea280): broadcast channel send/receive on the `AppState`
    clone path.
  - Task 4 (commit d03d827): atomic JSON persister round-trip + empty-
    token-as-None rule + corrupt-file recovery.
  - Task 5 (commit cdc59c1): `bitable_upsert_row` against `MockServer` asserting
    URL shape, `user_id_type=open_id`, payload primary key.
  - Task 6 (commits 98591f8, dbabc6c): publisher loop using
    `tokio::test (start_paused)` to drive virtual time through the 3s debounce;
    100- event burst test asserts exactly one upsert lands.
  - Task 7 (commit 2bcfae2): `send_with_retry` honours `Retry-After`, surfaces
    double-429 as error, and falls back to a default 1s when the header is
    absent.
  - Tasks 8 + 9 (commits 8dce127, 5385e17): wiring tests verifying the uploader
    binding and the startup spawn no-op when config absent.
  - Tasks 10–13 (commits 70be2d5, 1f9cff5, 34c1541, 203cda1): one emission test
    per event variant asserting the event reaches the broadcast subscriber with
    the right fields.
- **TypeScript: 855 vitest cases passing** — up from 791 at the start.
  - Task 14 (commit 3b6e6f9): `sanitize.ts` mirrors the Rust regex set; 10 cases
    lock the parity.
  - Task 15 (commit 07412ae): `api.teamActivity.get/set` invoke shape +
    `setWorkspaceTeamActivityPrivate` invoke shape (`ipc.test.ts`).
  - Tasks 16 + 17 (commits 2c1ece0, da13894): `TeamActivitySettings` component
    (load / save / setup / disconnect / disconnect-cancel) + `SettingsDialog`
    mount order regression.
  - Task 18 (commit 39e30a1): `workspaces.svelte.test.ts`
    `setTeamActivityPrivate` optimistic + revert; `WorkspaceView.test .ts`
    toggle ARIA + click behaviour.
- **E2E: env-gated under `ANSAMBEL_LARK_FIXTURE=1`** (commit db4b184):
  `tests/e2e/phase-3a-3-publisher/publisher-roundtrip.spec.ts` with two tests —
  settings-save happy path and privacy-toggle round-trip — both layering a
  publisher-domain mock on top of the base `installTauriShim` so the IPC
  boundary is asserted without hitting the Bitable HTTP path (Rust integration
  tests already cover that).

## Aftermath

The publisher is on by default once a user configures it via Settings → Team
Activity; before that it's a no-op. Read side (the team-activity sidebar that
consumes these Bitable rows for the local user's repos) is Phase 3a-4.

Smoke-test polish landed alongside the main 20-task plan:

- **Duplicate-row prevention on app restart** (`build_lark_uploader`): the
  in-memory `row_id_cache` is empty after each process boot, so the first
  publish for any workspace would POST a new row before this fix. Now we search
  Lark by `workspace_id IS X` before deciding POST vs PUT, so a restart never
  duplicates an existing row. Discovered in manual smoke testing — Bitable had 5
  rows for the same workspace after a few iterations.
- **`spawn_agent` idempotency** (`commands/agent.rs`): the Tauri command now
  downgrades to `reattach` when `agents` map already has an entry for this
  workspace. Frontend `Plan ↔ Work` toggle no longer surfaces "agent already
  running for ws\_…" errors. The frontend's onMount keeps using
  `messages.statusFor` first (cheaper than backend round-trip) but the backend
  guard means a stale read can't escalate to a toast.
- **Hydrate dedupe** (`messages.svelte.ts`): WorkspaceView's local user echo
  (`msg_user_<ts>` id) lives in the singleton store; on remount the persisted
  version arrives via `list_messages` with a different ULID id and renders
  alongside. `hydrate` now drops local echoes whose text matches a persisted
  user message in the same batch.
- **Snapshot enrichment** (`build_app_enricher` + `Publisher.enricher`):
  `repo_remote_url`, `repo_display_name`, `task_title`, `branch_name` are now
  populated for every flush by looking up the workspace in `AppState`.
  `repo_remote_url` is computed once per repo via `git remote get-url origin`
  and cached in-process; the rest come straight from existing struct fields.
  Privacy lock skips enrichment so the stripped-row contract still holds.

The `diff_summary` / `pr_url` consumer wiring remains the most obvious sweep
when Phase 3a-4 / 3a-5 lands a backend commit/push/PR-create surface — both
events are already on the broadcast bus.
