# Phase 3a-4 — View-Aware Lark Binding

**Status:** Approved — ready for plan. **Phase predecessor:**
[3a-3 Per-Repo Lark Binding](2026-05-15-phase-3a-3-per-repo-lark-binding-design.md)
(merged via PR #26). **Goal:** Ansambel honors per-view filters configured in
Lark Bitable. Each binding can scope to a single Bitable view so the user's
existing "Current Sprint" / "Backlog" / "My Open Tasks" filter is reflected in
the kanban — instead of dumping the entire table.

## Why

Phase 3a-3 shipped per-repo bindings but explicitly deferred view-level
filtering with this line in _What's NOT covered_: _"Lark Bitable view-level
filtering (per-view config) — entire table is the unit"_. Reality caught up
fast. During manual testing on 2026-05-16, the bound Bitable held 667 records
across all sprints. Ansambel's kanban showed 186 cards in `To Do`; Lark's
"Current Sprint" view showed 11. Same data, very different signal.

Lark already gives users a powerful per-view filter UI inside Bitable. The
filters live on the view, not the table. The Lark REST API natively supports a
`view_id` query parameter on the records-list endpoint that applies a view's
filter server-side. Ansambel just doesn't pass it. The fix is small in scope but
high in user value: one optional field on the binding, one wizard step, one
query-param wire change. No client-side filter grammar; no parallel filter DSL.

The user's mental model is _"I configure filters in Lark, Ansambel reflects
them"_. Phase 3a-4 makes that true.

## Goals

- Each `BitableBinding` can optionally scope to one Bitable view (`view_id`).
- When set, Ansambel's read path honors that view's filter (server-side).
- Wizard exposes the view picker as a new Step 1.5 between table selection and
  field mapping.
- Legacy bindings (no `view_id` in JSON) migrate transparently to `None`,
  preserving today's "fetch all records" behavior.
- View deletion in Lark degrades gracefully: auto-fallback to all records,
  surface a banner so the user can reconfigure.
- Writes (create / update / move / delete) target the table directly, unaffected
  by view scope.

## Non-goals

- Multi-view bindings (one binding → at most one view).
- Client-side filter expression input (rejected: opaque grammar, poor UX).
- Custom Ansambel-defined filter DSL (rejected: YAGNI, duplicates Lark).
- View creation / editing from inside Ansambel (Lark owns view config).
- Per-workspace view override (binding is per-repo; views are too).

## Architecture overview

A single optional field — `view_id: Option<String>` — flows through three
layers:

1. **Wire** (`platform/lark_client.rs`): `bitable_list_records` gains an
   optional `view_id` parameter that becomes `?view_id=...` in the query string.
   New helper `bitable_list_views(app_token, table_id)` powers the wizard
   dropdown.
2. **Provider** (`task_provider/lark.rs`): `LarkProvider` stores
   `view_id: Option<String>`, passes it to every `list_tasks` call. Writes never
   use the view_id — they target the table.
3. **Persistence + UI** (`state.rs`, wizard, settings dialog): `BitableBinding`
   gains `view_id`, defaults `None`. Wizard gains Step 1.5 "Scope this binding";
   settings dialog gains a "View:" row with [Change view…] button.

No new persisted state beyond the one optional field. The view list is fetched
live each time the wizard opens, matching how Phase 3a-3 fetches the field list.

## Components

### New types

```rust
// platform/lark_client.rs
pub struct BitableView {
    pub view_id: String,
    pub view_name: String,
    pub view_type: String,  // "grid", "kanban", "form", "gantt", "gallery", ...
}
```

### Modified types

```rust
// state.rs
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    #[serde(default)]                  // legacy bindings → None
    pub view_id: Option<String>,        // NEW
    pub field_mapping: FieldMapping,
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}
```

### New / modified API surface

```rust
// LarkClient
pub async fn bitable_list_views(
    &self,
    app_token: &str,
    table_id: &str,
) -> Result<Vec<BitableView>>;

// Signature changed — added `view_id`
pub async fn bitable_list_records(
    &self,
    app_token: &str,
    table_id: &str,
    filter: Option<&str>,
    view_id: Option<&str>,   // NEW
) -> Result<Vec<BitableRecord>>;
```

```rust
// Tauri command — new
#[tauri::command]
pub async fn list_lark_views(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<BitableView>, String>;
```

`LarkProvider::from_binding` already takes a `BitableBinding`; it stores
`view_id` alongside `app_token`/`table_id` and threads it through every
`list_tasks` call.

## Data flow

### Read path

```
Kanban refresh (focus event, manual reload, etc.)
  → list_tasks(repo_filter)
  → LarkProvider::list_tasks
  → client.bitable_list_records(app_token, table_id, None, view_id.as_deref())
  → Lark applies view filter (server-side) → returns scoped record set
  → record_to_task per row → kanban renders
```

### Wizard flow

```
Step 1: enter app_token + table_id → click [Detect]
  → backend: detect_lark_schema (existing, returns field list + suggested
    mapping)
  → backend: list_lark_views (NEW, fetches view list in parallel)
  → both responses populate wizard state; step = 1.5
Step 1.5: "Scope this binding"
  → dropdown: "All records (no view filter)" (first entry, maps to None)
  → followed by each BitableView { view_id, view_name, view_type }
  → user picks; click [Continue] → step = 2
Step 2: field mapping (existing)
Step 3: status values (existing)
Save: BitableBinding includes view_id; persisted via existing
      set_lark_repo_binding command
```

### View-deletion error path

```
list_tasks → bitable_list_records(..., view_id=Some("vw_X")) → Lark 404
  → LarkProvider matches "view not found" error sentinel
  → retries bitable_list_records(..., view_id=None)
  → emits Tauri event "lark-view-missing" { repo_id, view_id }
  → returns the unfiltered records (board keeps working)
Frontend banner store listens for the event → TitleBar shows persistent banner
  "The Lark view bound to {repo_name} no longer exists. Showing all records.
   [Reconfigure binding] [Dismiss]"
```

The banner is session-local (no persisted dismissal). It re-appears on the next
list call if the view is still missing.

## Wizard UI sketch

```
Step 1: Connection
  app_token: [bascn___]
  table_id:  [tbl_____]
  [Detect]

Step 1.5: Scope this binding to:                  ← NEW
  [▾ All records (no view filter)]
      • All records (no view filter)
      • Current Sprint        (grid)
      • Backlog Sprint        (grid)
      • Kanban Current Sprint (kanban)
      • Grid view             (grid)
  [Back] [Continue]

Step 2: Field mapping  (unchanged)
Step 3: Status values  (unchanged)
```

Each view row shows view_name + a small badge indicating `view_type`. No visual
indicator of whether a view has a filter — Lark's API doesn't expose that fact,
and view_name conventions are user-defined.

## Settings dialog

`RepoSettingsDialog.svelte` "Lark Sync" section gains one row above the
field-mapping summary:

```
View:  Current Sprint                         ← from views API at open time
       [Change view…]                          → reopens wizard at Step 1.5
```

When no view bound:

```
View:  All records (no filter)
       [Change view…]
```

[Change view…] reuses the existing `editingBinding` machinery to reopen
`LarkBindingWizard`, jumping to Step 1.5 with `app_token`/`table_id` pre-filled
(read-only on this entry path).

## Migration

Same pattern as Phase 3a-3's field_mapping migration:

```rust
// persistence/lark_repo_bindings.rs
// On load: a binding JSON without a `view_id` key deserializes as None via
// #[serde(default)] on Option<String>. No file rewrite required.

// schema_version bumped from 1 → 2 in lark_repo_bindings.rs's default_schema_version().
// migrate_lark_repo_bindings_v1_to_v2 is a no-op semantically (no field
// values change) but stamps the version so future migrations have a clear
// baseline. Triggered on first load that finds schema_version=1.
```

Existing bindings continue with `view_id = None` — zero user-visible change
until the user reconfigures. No banner shown for v1→v2 migration (unlike the
3a-3 legacy-file migration which created a new binding from
`lark_settings.json`).

## Error handling

- **`bitable_list_views` request fails:** propagates as today's
  `AppError::Lark`. Wizard shows the error toast and offers retry. User can
  proceed with "All records" as a fallback (no view list needed).
- **View 404 on records list:** `LarkProvider` detects the sentinel error text,
  retries without `view_id`, emits `lark-view-missing` event. List call returns
  the unfiltered records — kanban does not block.
- **View 404 on `bitable_list_views` (impossible — that's a table call):** not a
  real case; the views endpoint operates on the table, not a specific view.
- **`view_id` saved but Lark returns 0 records (view filter matches nothing):**
  not an error; kanban renders empty columns. User sees the same view in Lark.
- **Other Lark errors:** propagate normally; no fallback.

## Testing

### Unit tests

- `lark_client.rs`:
  - `bitable_list_records_with_view_id_passes_query_param`
  - `bitable_list_records_with_no_view_id_omits_query_param`
  - `bitable_list_views_paginates`
  - `bitable_list_views_handles_empty_table`
- `task_provider/lark.rs`:
  - `lark_provider_list_tasks_passes_view_id_when_set`
  - `lark_provider_list_tasks_omits_view_id_when_none`
  - `lark_provider_falls_back_when_view_not_found`
  - `lark_provider_view_unrelated_error_propagates`
- `commands/lark_repo_binding.rs`:
  - `list_lark_views_returns_view_list`
  - `set_binding_with_view_id_persists_and_round_trips`
  - `set_binding_with_view_id_none_is_default`
- `persistence/lark_repo_bindings.rs`:
  - `legacy_binding_without_view_id_loads_as_none`
  - `binding_with_view_id_some_serializes_correctly`
- `persistence/lark_repo_bindings.rs` (additional):
  - `migrate_v1_to_v2_preserves_existing_bindings_with_no_view_id`
  - `migrate_v1_to_v2_is_idempotent`

### Integration tests (wiremock)

- `wizard_save_with_view_id_persists_and_provider_uses_it`
- `view_deleted_after_binding_falls_back_and_continues`

### Frontend component tests (Vitest + Testing Library)

- `LarkBindingWizard.svelte`:
  - Step 1.5 renders dropdown with "All records (no view filter)" first.
  - Selecting a view stores its id; Continue advances to Step 2.
  - Empty view list still shows "All records" entry and lets user proceed.
  - Editing existing binding with view_id pre-selects it in the dropdown.
- `RepoSettingsDialog.svelte`:
  - "View" row shows view name when bound, "All records" when not.
  - [Change view…] opens wizard at Step 1.5.
- `TitleBar.svelte`:
  - Banner appears on `lark-view-missing` event.
  - [Reconfigure] opens wizard at Step 1.5.

### E2E (Playwright)

Lark API mocked via `ANSAMBEL_MOCK_LARK=1` with a fixture serving views +
filtered records.

- `step_1_5_lists_views_and_defaults_to_all_records`
- `step_1_5_selecting_view_scopes_kanban_after_save`
- `step_1_5_change_view_from_settings_reopens_wizard_at_correct_step`
- `view_deleted_in_lark_shows_banner_and_falls_back`
- `step_1_5_empty_view_list_still_lets_user_proceed`

### Coverage gate

95% line / branch / function on changed files per project rule. No `#[ignore]`
without a linked issue.

## Decision log

| Decision                         | Choice                                             | Why                                                                                                |
| -------------------------------- | -------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| View-binding relationship        | Optional `view_id: Option<String>`                 | None = today's behavior, zero migration friction; explicit opt-in is honest about scope            |
| Wizard placement                 | New Step 1.5 between table & mapping               | Clean separation: connect → scope → map; lets field detection run after view is known              |
| View-deletion handling           | Auto-fallback to all records + banner              | Workspaces keep working; user gets clear signal + reconfigure path                                 |
| Write-side scope                 | No gating — writes always go to the table          | Mirrors Lark's own behavior; client-side filter eval is fragile and high-effort for limited value  |
| View list freshness              | Fetched live each wizard open                      | Matches 3a-3's field-list pattern; views change rarely but staleness is the worst kind of bug here |
| Migration strategy               | No-op v1→v2 schema bump, `#[serde(default)]` field | Mechanically safe; legacy bindings work identically until reconfigured                             |
| Filter mechanism                 | Lark's native `view_id` query param                | Server-side; reuses user's existing Lark filter UI; zero parallel filter logic                     |
| Frontend signal for missing view | Tauri event + Svelte store                         | Decouples banner from list call; multiple components can react if needed                           |

## What's NOT covered

- Multi-view-per-binding (intersect / union scopes) — YAGNI.
- View creation / editing from Ansambel — Lark owns view config.
- Per-workspace view override — bindings are repo-scoped.
- Auto-detecting which view a user "probably" wants — picker is explicit.
- Honoring view sort order — task ordering still follows Ansambel's column rank
  logic; view sort affects only which rows return, not their order in the
  kanban.

## Open questions (none blocking)

All design questions resolved during brainstorm.

- "What if multiple repos bind to the same table with different views?" → Each
  binding stores its own view_id independently; no shared state.
- "What if a view's filter changes after binding?" → Filter is server-side; next
  list call reflects the new filter automatically. No client action needed.
- "Can Lark add new view types we don't recognize?" → Yes; the badge falls back
  to displaying `view_type` verbatim. Filtering is unaffected.
