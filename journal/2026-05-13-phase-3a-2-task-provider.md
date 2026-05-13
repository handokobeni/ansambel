# Journal — 2026-05-13 — Phase 3a-2 TaskProvider + Lark plugin

## PR: feat(phase-3a-2) — TaskProvider abstraction + Lark Bitable plugin

**Branch:** `feat/phase-3a-2-task-provider` **Author:** handokobeni **Phase:**
3a-2 (kanban sync — closes the 3a-2 deliverable)

### Summary

Introduces the `TaskProvider` trait + two impls (`LocalProvider` and
`LarkProvider`), wires a hot-swap-able provider handle into Tauri state, adds a
Bitable schema verify wizard for the 5 fields required at this phase, and makes
the kanban UI work against either backend. Source switch lives in the Settings
dialog; window-focus refresh (2s debounce) keeps a Lark-backed board in sync
with teammates without spamming the API.

### Yang shipped

- **`task_provider/mod.rs`** — `#[async_trait] TaskProvider` with `list_tasks`,
  `create_task`, `update_task`, `move_task`, `delete_task`. `CreateTaskArgs` +
  `TaskPatch` carry the same shape across both impls.
- **`task_provider/local.rs`** — `LocalProvider` wraps the existing `tasks.json`
  file. Uses a `Mutex<()>` to serialize read-modify-write cycles. Tests cover
  round-trip, repo filter, partial patch, move, delete, not-found paths.
- **`task_provider/lark.rs`** — `LarkProvider` translates Bitable rows ↔ `Task`.
  Critical: Bitable's `CurrentValue.[field_name]` filter grammar only resolves
  user-defined fields, so re-fetching a task by `record_id` after update/move
  required a new `bitable_get_record` endpoint (GET `/records/{record_id}`), not
  a filter query. Map errors surface both the field name AND the `record_id` so
  debugging a bad row in production has a single grep target.
- **`task_provider/schema.rs`** — `RequiredField` registry + `verify_schema`
  helper. Idempotent: diffs the live table against the registry, creates any
  missing fields via `bitable_create_field`, and surfaces type mismatches
  separately (the user has to fix those in Bitable UI — we never alter an
  existing field's type).
- **`platform/lark_client.rs`** — added `bitable_list_fields`,
  `bitable_create_field`, `bitable_get_record`. The create endpoint omits the
  `property` key entirely when `None` because Lark rejects `property: null` on
  text fields. `BitableRecord` got `#[serde(flatten)] extra: HashMap` so we can
  pull `created_time` (ms-since-epoch) without enumerating every reserved field
  — `extra_i64()` helper does the cast.
- **`state.rs`** — `TaskSource` enum (`Local`/`Lark`, snake_case on the wire),
  `AppSettings.task_source` with `serde(default)` for old persisted records,
  `TaskProviderHandle = Arc<RwLock<Arc<dyn TaskProvider>>>` typedef. The
  read-lock returns a cheap `Arc::clone` so command handlers don't hold the lock
  across async work; the write-lock swaps the inner `Arc` whole.
- **`lib.rs` setup**: `build_initial_provider` resolves Local vs Lark from
  settings + keyring at startup. Lark mode with missing creds falls back to
  Local with a `tracing::warn!` rather than panicking — the user can still open
  the app and re-enter credentials. Hydration kicks off in a `tokio::spawn` so a
  slow Bitable list doesn't block the window from appearing.
- **`commands/task.rs` refactor**: all 4 `_inner` fns now take
  `Arc<dyn TaskProvider>` instead of touching `tasks.json` directly. The lock
  discipline: acquire `tasks` mutex → snapshot → drop → provider call (async) →
  re-acquire → write mirror. `move_task_inner` still owns the
  auto-workspace-creation side effect; that path bypasses the provider and
  writes `workspace_id` directly to the mirror. Reviewed — fine for Local,
  flagged as a known limitation for Lark mode (the auto-created workspace ID is
  local-only).
- **3 new commands**: `refresh_tasks(repoId?)` re-hydrates the mirror from the
  provider + emits `tasks-rehydrated`; `get_task_source` / `set_task_source`
  surface the persisted setting + swap the provider handle in place
  (build-first, persist, swap, re-hydrate — so a bad Lark config fails before we
  lose Local).
- **`verify_lark_schema` command** — wraps `verify_schema()` against the
  currently-configured Bitable, returns `SchemaCheckResult`. Built on top of
  `LarkClient` so it shares the keyring lookup + base_url resolution from Phase
  3a-1b.
- **Frontend types + IPC** — `TaskSource`, `SchemaCheckResult` added; `api.task`
  gained `refresh`, `getSource`, `setSource`; `api.lark` gained `verifySchema`.
- **Tasks store optimistic move** — `move()` writes to the nested `SvelteMap`
  before awaiting the backend, and reverts + toasts on rejection. `refresh()`
  selectively replaces a single repo's entries (or all if no repoId).
- **Window-focus refresh** — `App.svelte` listens for `window` focus, debounces
  2s, then calls `tasks.refresh(repo.id)` only when `getSource() === 'lark'`.
  Local mode short-circuits so no network call fires.
- **`SettingsDialog` Task source section** — two radio buttons (Local / Lark
  Bitable) with optimistic + revert + toast. Sits above the existing
  `LarkSettings` panel.
- **`LarkSettings` schema section** — "Bitable schema" block with a Verify /
  Initialize button. Button stays disabled until credentials are saved. Result
  block lists `created` / `already_present` / `type_mismatches` separately so
  the user knows what the wizard did versus what they need to fix manually.
- **Env-gated E2E** — `tests/e2e/phase-3a-2/phase-3a-2-lark-sync.spec.ts` drives
  the source toggle + schema verify against a real tenant when `LARK_*` env vars
  are set; skips cleanly when they're not.

### Decisions taken

- **Optimistic local mirror + write-through, not server-of-truth**. Inspired by
  Linear/Figma. The mirror keeps the kanban snappy even when Lark is on the
  other side of the world; failed writes revert + toast rather than silently
  desyncing. Reads on focus reconcile teammate edits without polling.
- **App-level provider, not per-repo**. One backend choice covers all repos.
  Per-repo would have meant a `task_source` column on `repos.json` and a
  different `LarkClient` config per repo — overkill for the 1–3 repos a single
  user opens in this app.
- **Auto-create schema via wizard, not require manual setup**. Lark Bitable
  schema construction is fiddly (option enums need exact ordering for
  single-select fields); making the user do it by hand was a non-starter. Wizard
  is idempotent so re-runs are safe and cheap.
- **`record_id` becomes the `Task.id` for Lark-backed tasks** (no `tk_` prefix).
  Avoids a separate ID-mapping table. `workspace_id` is always `None` for Lark
  tasks because workspaces are a local-only concept in 3a-2.
- **`Arc<RwLock<Arc<dyn TaskProvider>>>` for hot-swap**. The double-Arc looks
  funny but the outer one is the managed Tauri state and the inner one is what
  the read-lock clones out so commands don't hold the lock during network calls.
  Write-lock is rare (only on `set_task_source`) so contention is fine.

### Surprises hit

- **`KanbanColumn` doesn't derive `Ord`** — plan assumed it did for sorting.
  Added a `column_rank()` helper in `lark.rs` instead of polluting `state.rs`
  with an `Ord` impl that has no other consumers.
- **Bitable `CurrentValue.[record_id]` doesn't work** — system metadata fields
  aren't filterable that way. Had to add a dedicated `bitable_get_record`
  endpoint. Caught by the code-quality reviewer on Task 6 before merging.
- **`bitable_create_field` rejects `property: null`** — has to be omitted
  entirely when the field type doesn't need property metadata (text fields).
  Wrote a struct-with-conditional-skip rather than reaching for serde
  `skip_serializing_if`.
- **Branch coverage barely above 95%** — main was at 95.13%. New code added a
  handful of untested branches (non-Error rejection paths in three different
  catch blocks, the no-op guard in `handleSourceChange`, the global refresh
  branch). Added targeted tests in a follow-up commit to lift coverage back to
  95.03%. CI gate would have failed without it.

### Apa yang deferred ke 3a-3+

- **Real-time push from Lark** — currently relies on focus-refresh; webhooks /
  EventSource would close the gap but add a long-running daemon.
- **Conflict resolution** — last-write-wins today. Two users editing the same
  row concurrently will see the loser's changes vanish on next refresh.
  Acceptable for the small-team usage Lark targets; not for shared-editor
  semantics.
- **Schema migration beyond 3a-2's 5 fields** — Phase 3a-3 will likely add agent
  metadata fields (assignee, due date, attachment count). Registry is designed
  to accept new entries without breaking older clients.
- **Auto-workspace-creation in Lark mode** — `move_task` to `in_progress`
  creates a local workspace but doesn't write the workspace_id back to Bitable.
  Either we add a workspace_id column or accept that workspace binding is
  local-only. Punt to a follow-up after some real usage.
- **Bulk import from existing tasks.json** — a one-click "copy my local board to
  Lark" would smooth the migration. Not in scope for 3a-2; do it if/when a user
  asks.

### Gates that ran

- `cargo test --lib` — 514 pass, 0 fail
- `bun run test --run` — 700 pass across 45 files
- `bun run test:coverage` — branches 95.03% ≥ 95% threshold
- `bun run check` — 0 errors
- `bun run lint` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
