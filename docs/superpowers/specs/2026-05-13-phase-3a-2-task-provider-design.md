# Phase 3a-2 — TaskProvider Abstraction + Lark Bitable Plugin

**Status:** Design (approved 2026-05-13) **Implements:** Phase 3a-2 of
`docs/superpowers/plans/2026-05-09-ansambel-phase-3a-lark-team-sync.md`
**Depends on:** Phase 3a-1 (Lark client, auth, settings UI — merged in PR #22
and #23, polished in PR #24).

## Goal

Introduce a `TaskProvider` trait so the kanban can be backed either by the
existing local `tasks.json` store or by a shared Lark Bitable table. App-level
setting (`Settings → Task source`) picks one. Lark mode keeps the kanban
responsive via optimistic UI + write-through to Bitable, and ships a one-click
"Initialize Bitable schema" wizard that auto-creates the five fields required
for sync.

This phase is the foundation for Phase 3a-3 (state publisher), 3a-4 (Team
Activity sidebar), and Phase 3b (Jira).

## Out of scope for 3a-2

The following Bitable fields exist in the long-term schema but are **not** read
or written by the Lark provider in this phase. The schema wizard does not create
them either — each later phase extends the registry when it needs them.

| Field                                                                                                   | Owner phase |
| ------------------------------------------------------------------------------------------------------- | ----------- |
| `ansambel_status`, `last_activity_at`, `last_message_preview`, `pr_url`, `assignee_machine`, `priority` | 3a-3        |
| `private`                                                                                               | 3a-7        |
| `blocked_question`                                                                                      | 3a-6        |
| `handoff_target`, `handoff_bundle`, `handoff_at`                                                        | 3a-8        |

Team Activity polling and the read-only watcher view also live in later phases
(3a-4); 3a-2 ships pull-on-focus refresh only.

## Decisions locked during brainstorm

1. **Sync model:** optimistic local mirror + write-through to provider. Frontend
   updates UI immediately on mutation, calls backend, reverts with toast on
   failure.
2. **Provider scope:** app-level. One setting governs all repos. Lark filtering
   by `repo_id` happens server-side via Bitable filter expression.
3. **Refresh triggers:** startup hydrate, manual "Refresh" button on kanban
   header, `window.focus` debounced 2s. No periodic background polling in 3a-2 —
   that arrives with the 3a-4 Team Activity sidebar.
4. **Schema setup:** auto-create via wizard. New backend command
   `verify_lark_schema` lists existing fields and creates only the missing ones
   (idempotent). Type mismatches are surfaced to the user, not auto-fixed.

## Architecture

```
                 ┌──────────────────────────────────────┐
                 │ Settings (Tauri-managed JSON)        │
                 │  app_settings.task_source: local|lark│
                 └──────────────────────────────────────┘
                                  │ (read on startup +
                                  │  set_task_source command)
                                  ▼
                 ┌──────────────────────────────────────┐
                 │ TaskProviderHandle                   │
                 │  = Arc<RwLock<Arc<dyn TaskProvider>>>│
                 └──────────────────────────────────────┘
                                  │
                  ┌───────────────┴────────────────┐
                  ▼                                ▼
        ┌──────────────────┐            ┌────────────────────┐
        │ LocalProvider    │            │ LarkProvider       │
        │  data_dir,       │            │  client (Arc),     │
        │  inner_lock      │            │  app_token, table  │
        │                  │            │  cached field_map  │
        └──────────────────┘            └────────────────────┘
                  │                                │
                  ▼                                ▼
        ┌──────────────────┐            ┌────────────────────┐
        │ tasks.json       │            │ Lark Bitable       │
        │ (atomic writes)  │            │ (HTTPS, rate-      │
        │                  │            │  limited 200/min)  │
        └──────────────────┘            └────────────────────┘

           ┌──────────────────────────────────────────┐
           │ AppState.tasks: HashMap<id, Task>        │
           │  ── in-memory mirror, written by command │
           │     layer AFTER provider call succeeds   │
           └──────────────────────────────────────────┘
                                  ▲
                                  │ api.task.list reads mirror
                                  │ api.task.{add,move,…} mutate via provider
                                  ▼
                 ┌──────────────────────────────────────┐
                 │ Frontend tasks store                 │
                 │  optimistic UI + revert-on-error     │
                 └──────────────────────────────────────┘
```

## Component design

### `task_provider/mod.rs`

```rust
#[async_trait::async_trait]
pub trait TaskProvider: Send + Sync + std::fmt::Debug {
    /// Pull tasks for one repo (or all when None). Used for initial
    /// hydrate and on-demand refresh. Implementations decide ordering;
    /// command layer trusts the order returned.
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>>;

    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task>;

    /// Partial update. Fields absent from `patch` are left unchanged.
    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task>;

    /// Combined column + order mutation (the only drag-and-drop op).
    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task>;

    async fn delete_task(&self, id: &str) -> Result<()>;
}

pub struct CreateTaskArgs {
    pub repo_id: String,
    pub title: String,
    pub description: String,
    pub column: Option<KanbanColumn>, // defaults to Todo
}
```

`async-trait = "0.1"` is added to `Cargo.toml` so the trait is `dyn`-compatible
despite holding async methods.

### `state.rs` additions

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Local,
    Lark,
}

// AppSettings gains:
//   #[serde(default = "default_task_source")]
//   pub task_source: TaskSource,

pub type TaskProviderHandle = Arc<tokio::sync::RwLock<Arc<dyn TaskProvider>>>;
```

The provider handle is registered as Tauri-managed state separately from
`AppState`, so async provider calls do not need to hold the `AppState` lock.

### `task_provider/local.rs`

Wraps the existing tasks.json behavior. Each trait method mirrors what
`commands/task.rs::*_inner` does today, minus the AppState mutation (the command
layer owns that). Internal `Mutex<()>` serializes read-modify-write on the JSON
file. IDs continue to be `tk_<nanoid>`.

### `task_provider/lark.rs`

Wraps `LarkClient` for Bitable CRUD. Key choices:

- **Task.id = Bitable `record_id`** (format `rec...`) when Lark mode is active.
  Local `tk_<nanoid>` and Lark `rec...` never coexist in `AppState.tasks`
  because hydrate is single-provider.
- **Workspace_id is purely local.** Not stored in Bitable. After hydrate,
  `Task.workspace_id = None`; the UI joins with the workspaces store to surface
  "this task has a worktree on my machine".
- **Order field** `order_within_column` (Number type) is added to the Bitable
  schema and updated on every `move_task`. `list_tasks` sorts client-side DESC
  within each column bucket. Without this field, Bitable returns rows in
  arbitrary order.

Bitable → Task field mapping:

| Bitable field          | Task field    | Notes                          |
| ---------------------- | ------------- | ------------------------------ |
| `record_id` (meta)     | `id`          | String, `rec...`               |
| `title` (text)         | `title`       | Required                       |
| `description` (text)   | `description` | Default `""` if missing        |
| `kanban_column` (enum) | `column`      | Required; single-select values |
| `repo_id` (text)       | `repo_id`     | Required                       |
| `order_within_column`  | `order`       | Default `0` if missing         |
| `created_time` (meta)  | `created_at`  | ms-since-epoch → seconds       |
| `last_modified_time`   | `updated_at`  | ms-since-epoch → seconds       |

`list_tasks(repo_filter)` passes `CurrentValue.[repo_id]="<id>"` as the Bitable
filter expression so server-side filtering kicks in (avoids pulling all repos'
rows just to drop them client-side).

### `task_provider/schema.rs`

```rust
pub struct RequiredField {
    pub name: &'static str,
    pub field_type: u32,           // Lark field type code (verified at impl)
    pub property: Option<serde_json::Value>,
}

pub fn required_fields_phase_3a2() -> Vec<RequiredField> { /* 5 fields */ }

pub struct SchemaCheckResult {
    pub ok: bool,
    pub created: Vec<String>,
    pub already_present: Vec<String>,
    pub type_mismatches: Vec<String>,
}
```

Five fields for 3a-2: `title` (text), `description` (text), `kanban_column`
(single-select with options `todo`/`in_progress`/ `review`/`done`), `repo_id`
(text), `order_within_column` (number).

The exact numeric type codes Lark uses are verified during implementation
against the Open Platform docs and pinned via tests. Working assumption:
`1=Text, 2=Number, 3=SingleSelect, 5=DateTime, 7=Checkbox, 15=URL, 17=Attachment`.

### `platform/lark_client.rs` additions

```rust
impl LarkClient {
    pub async fn bitable_list_fields(
        &self, app_token: &str, table_id: &str,
    ) -> Result<Vec<BitableField>>;

    pub async fn bitable_create_field(
        &self, app_token: &str, table_id: &str,
        field_name: &str, field_type: u32,
        property: Option<serde_json::Value>,
    ) -> Result<BitableField>;
}

pub struct BitableField {
    pub field_id: String,
    pub field_name: String,
    pub field_type: u32,
    pub property: Option<serde_json::Value>,
}
```

Both go through `send_with_retry` (existing rate-limit + 429 retry infra).

### `commands/task.rs` refactor

Existing `_inner` functions take an extra `provider: Arc<dyn TaskProvider>`
parameter. Lock discipline:

```rust
pub async fn move_task_inner(...) -> Result<Task> {
    // 1. Acquire provider Arc (read lock is brief and async-aware).
    let provider = provider_handle.read().await.clone();
    // 2. Async network call — no AppState lock held here.
    let updated = provider.move_task(&id, column, order).await?;
    // 3. Re-acquire AppState briefly to update mirror.
    { let mut st = state.lock()?; st.tasks.insert(updated.id.clone(), updated.clone()); }
    Ok(updated)
}
```

Existing tests stay green because the LocalProvider behavior matches the
pre-refactor logic byte-for-byte.

### New commands

- `set_task_source(source: TaskSource)` — persists to `app_settings.json`,
  builds the chosen provider, swaps the handle via write lock, clears
  `AppState.tasks`, re-hydrates via `provider.list_tasks(None)`, emits Tauri
  event `tasks-rehydrated` for the frontend store. Rejects with a descriptive
  error when switching to `Lark` while `lark_settings.json` is incomplete or the
  keyring is empty.
- `get_task_source()` — returns the current `TaskSource`.
- `refresh_tasks(repo_id: Option<String>)` — runs
  `provider.list_tasks(repo_filter)`, replaces the matching subset of
  `AppState.tasks`, returns the new list.
- `verify_lark_schema()` — runs the wizard against the configured Lark table,
  returns `SchemaCheckResult`.

## Data flow

### Optimistic mutation (drag-to-move)

1. User drags a task card. Frontend `tasks.svelte.ts::move(id, col, order)`:
   1. Snapshot the current task: `prev = this.tasks.get(id)`.
   2. Mutate the SvelteMap in place:
      `this.tasks.set(id, { ...prev, column, order })`. Kanban re-renders
      instantly.
   3. `await api.task.move(id, col, order)`.
2. Backend `move_task_inner`:
   1. Reads `TaskProviderHandle`, clones inner Arc.
   2. Calls `provider.move_task(id, col, order)` — LarkProvider issues a single
      `bitable_update_record` with `{ kanban_column, order_within_column }`.
   3. Lark returns the updated row metadata → mapped to a fresh `Task`.
   4. Backend updates `AppState.tasks` mirror with the canonical Task.
   5. Returns the canonical Task to the frontend.
3. Frontend reconciles:
   - On success: `this.tasks.set(updated.id, updated)` — overwrites optimistic
     state with backend's authoritative version (fresh timestamps, etc).
   - On failure: `this.tasks.set(id, prev)` (revert) + toast.

### Startup hydrate

`lib.rs::setup()`:

1. Load `app_settings.json` → `task_source`.
2. Build `LocalProvider` or `LarkProvider` depending on `task_source`. (Lark
   build requires `lark_settings.json` + keyring; missing → fall back to
   `LocalProvider` with a warning toast at first render.)
3. Wrap as `Arc<dyn TaskProvider>` and register `Arc::new(RwLock::new(...))`.
4. Spawn a Tokio task: call `provider.list_tasks(None)`, populate
   `AppState.tasks`. Errors surface as a banner — kanban renders empty until the
   user fixes Lark connectivity and clicks Refresh.

### Refresh (manual button + window focus)

Both call `refresh_tasks(repo_id)`. The window-focus listener debounces to 2s so
alt-tab bursts collapse to one fetch. Local mode skips the focus listener
entirely — re-reading tasks.json on every focus has zero benefit.

### Provider hot-swap

`set_task_source` flow:

1. Validate target source (Lark requires configured credentials).
2. Acquire write lock on `TaskProviderHandle`, swap inner Arc.
3. Clear `AppState.tasks`.
4. `await new_provider.list_tasks(None)` → populate mirror.
5. Emit `tasks-rehydrated`. Frontend store listens, clears + reloads its local
   Map for currently-selected repo.

In-flight mutations during the swap complete with the old provider (they hold an
Arc clone of the old `dyn TaskProvider`). No mid-call race.

## Error handling & offline behavior

- **Lark unreachable during mutation:** provider returns `AppError::Lark(...)`.
  Frontend reverts optimistic state + toasts. No retry — user re-issues the
  action.
- **Lark unreachable during refresh:** provider returns error. Backend keeps the
  existing `AppState.tasks` mirror untouched. Frontend shows a banner ("Last
  sync 5m ago — retry?").
- **Lark schema incomplete on first list_tasks:** rows missing required fields
  surface as `AppError::Lark("...missing field 'kanban_column'...")`. Frontend
  banners "Initialize schema in Settings".
- **No disk fallback in Lark mode.** When Lark is the source, we do not write
  tasks to `tasks.json` — that would create a divergent shadow store. The mirror
  lives only in memory; restart re-hydrates from Lark.
- **Two-step concurrency between engineers:** last-write-wins at Bitable level
  (no CAS / etag). A's next focus-refresh catches up to B's edits.

## Frontend changes

### `src/lib/stores/tasks.svelte.ts`

- `move(id, col, order)`: optimistic + revert pattern (above).
- `add(args)`: optimistic insert with a temporary `tmp_<nanoid>` id; replace
  with backend-returned id on success; delete temp on failure.
- `remove(id, force?)`: optimistic delete (cache `prev` for revert).
- `refreshSelected()`: calls `api.task.refresh(selectedRepoId)` and replaces the
  Map subset for that repo.
- Window-focus listener wired in `App.svelte` calls `refreshSelected` debounced
  2s. Skipped when `task_source === 'local'`.

### Settings UI

`SettingsDialog.svelte` gets a new top section before the Lark credentials
section:

```
┌─ Task source ──────────────────────────────────┐
│ Where Ansambel stores and syncs your kanban.   │
│                                                │
│  ( ) Local (tasks.json on this machine)        │
│  (•) Lark Bitable (shared with team)           │
└────────────────────────────────────────────────┘
```

Toggling fires `api.task.setSource(source)`. Switching to `lark` while
credentials are missing reverts the toggle and shows an inline error nudging the
user to fill the Lark credentials form below.

`LarkSettings.svelte` gets a "Bitable schema" section below the existing
buttons:

```
┌─ Bitable schema ─────────────────────────────────┐
│ 5 fields required for kanban sync.               │
│                                                  │
│ [Status pill]                                    │
│ [Verify / Initialize schema]                     │
│                                                  │
│ Last result:                                     │
│   ✓ already present: title, repo_id              │
│   + created: kanban_column, description, …       │
│   ✗ type mismatch: (none)                        │
└──────────────────────────────────────────────────┘
```

Section is disabled until `status.configured === true`. Re-runnable; each click
re-checks against current Bitable schema.

### `src/lib/ipc.ts`

```ts
api.task.refresh: (repoId?: string): Promise<Task[]> =>
  invoke('refresh_tasks', { repoId }),
api.task.setSource: (source: 'local' | 'lark'): Promise<void> =>
  invoke('set_task_source', { source }),
api.task.getSource: (): Promise<'local' | 'lark'> =>
  invoke('get_task_source'),
api.lark.verifySchema: (): Promise<SchemaCheckResult> =>
  invoke('verify_lark_schema'),
```

### `src/lib/types.ts`

```ts
export type TaskSource = 'local' | 'lark';

export type SchemaCheckResult = {
  ok: boolean;
  created: string[];
  already_present: string[];
  type_mismatches: string[];
};
```

## Testing strategy

### Rust unit tests

`task_provider/local.rs` (5):

- `local_provider_round_trip_via_tasks_json`
- `local_provider_list_filters_by_repo`
- `local_provider_create_assigns_tk_prefix_id`
- `local_provider_move_updates_column_and_order`
- `local_provider_delete_removes_from_disk`

`task_provider/lark.rs` (8, all via wiremock):

- `lark_provider_list_maps_record_id_to_task_id`
- `lark_provider_list_sorts_by_order_within_column_desc`
- `lark_provider_list_passes_repo_filter_via_lark_filter_expr`
- `lark_provider_create_returns_task_with_record_id`
- `lark_provider_move_sends_kanban_column_and_order_only`
- `lark_provider_update_sends_partial_fields`
- `lark_provider_delete_calls_bitable_delete`
- `lark_provider_surfaces_missing_field_error_clearly`

`task_provider/schema.rs` (4, via wiremock):

- `verify_schema_creates_all_missing`
- `verify_schema_skips_present_fields`
- `verify_schema_surfaces_type_mismatch`
- `verify_schema_idempotent_on_rerun`

`commands/task.rs` refactor: existing tests must stay green. New:

- `set_task_source_persists_and_rehydrates`
- `set_task_source_lark_rejects_when_credentials_missing`
- `refresh_tasks_replaces_mirror_subset_for_repo`

`platform/lark_client.rs`: 2 new tests for `bitable_list_fields` and
`bitable_create_field` request shapes.

### Frontend tests (Vitest)

`stores/tasks.svelte.ts`:

- `tasks_move_optimistic_updates_then_reconciles_with_backend_response`
- `tasks_move_revert_on_error_restores_previous_state_and_toasts`
- `tasks_refresh_replaces_map_for_selected_repo`
- `tasks_window_focus_debounces_refresh_calls_to_one`

`LarkSettings.svelte` schema section:

- `schema_section_disabled_when_credentials_not_saved`
- `verify_schema_renders_created_and_already_present_lists`
- `verify_schema_renders_type_mismatch_warning`

`SettingsDialog.svelte` task source toggle:

- `task_source_toggle_persists_selection`
- `task_source_switch_to_lark_blocked_when_unconfigured_with_inline_error`

### E2E (Playwright, env-gated)

`tests/e2e/phase-3a-2-lark-sync.spec.ts`: skip-on-missing-creds (same pattern as
`tests/lark_smoke.rs`). Flow: configure Lark → switch task source → verify
schema → kanban populates → drag task to Done → restart app → verify Bitable
state persisted.

### Coverage gate

Same 95% line + branch + function on changed files. New providers go through the
same regex-exclusion review as existing thin commands (the trait dispatch glue
may need exclusion if its branches are exhaustively covered through subclasses'
tests).

## Files summary

**Create:**

- `src-tauri/src/task_provider/{mod.rs, local.rs, lark.rs, schema.rs}`
- `tests/e2e/phase-3a-2-lark-sync.spec.ts`

**Modify:**

- `src-tauri/src/state.rs` — `TaskSource`, `AppSettings.task_source`,
  `TaskProviderHandle` typedef
- `src-tauri/src/commands/task.rs` — refactor `_inner` fns to call provider; add
  `set_task_source`, `get_task_source`, `refresh_tasks`
- `src-tauri/src/commands/lark_auth.rs` — add `verify_lark_schema` command
  (delegates to `task_provider::schema`)
- `src-tauri/src/platform/lark_client.rs` — add `bitable_list_fields`,
  `bitable_create_field` + `BitableField` struct
- `src-tauri/src/lib.rs` — provider init in `setup()`, register new commands
- `src-tauri/Cargo.toml` — `async-trait = "0.1"`
- `src/lib/ipc.ts` — new task + lark wrappers
- `src/lib/types.ts` — `TaskSource`, `SchemaCheckResult`
- `src/lib/stores/tasks.svelte.ts` — optimistic + revert + refresh
- `src/lib/components/SettingsDialog.svelte` — task source section
- `src/lib/components/lark/LarkSettings.svelte` — Bitable schema section

## Open questions resolved during brainstorm

| Question                              | Decision                                                                                         |
| ------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Sync semantics for human kanban moves | Optimistic local mirror + write-through; revert on backend failure                               |
| Per-repo or app-level provider        | App-level; Lark filters by `repo_id` server-side                                                 |
| Refresh cadence                       | Startup + manual button + on-focus (2s debounce). No periodic poll until 3a-4.                   |
| Schema initialization                 | Auto-create via wizard; idempotent; surfaces type mismatches without auto-fix                    |
| Task.id format under Lark             | Use Lark `record_id` directly; drop `tk_` prefix when Lark mode active                           |
| Row ordering in Bitable               | Add `order_within_column` (Number) to required schema; client-side sort DESC within each column  |
| Workspace_id propagation              | Local-only — not stored in Bitable. Re-joined client-side from `workspaces.json`.                |
| Schema fields owned by later phases   | Wizard does **not** create them in 3a-2; later phases extend the `required_fields_*()` registry. |
