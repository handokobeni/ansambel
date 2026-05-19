# Phase 3a-3.1 (v2) — Filter-Aware Lark Binding

**Status:** Approved — ready for plan. **Phase predecessor:**
[3a-3 Per-Repo Lark Binding](2026-05-15-phase-3a-3-per-repo-lark-binding-design.md)
(merged via PR #26). **Supersedes:**
[2026-05-16 view-aware Lark binding spec](2026-05-16-phase-3a-3-1-view-aware-lark-binding-design.md)
(PR #27 closed without merge). **Goal:** Ansambel exposes a Lark-style filter UI
directly above the kanban; user filters by any Bitable column real-time, and
Lark applies the filter server-side via the `bitable/v1/.../records/search`
endpoint.

## Why

Phase 3a-3 shipped per-repo bindings but Ansambel fetched the entire bound table
— 667 records in a real test, while Lark's "Current Sprint" view showed only 11.
PR #27 attempted to fix this by binding to a specific Lark view (server-side
filter), but the approach has three serious limitations:

1. **Requires pre-setup in Lark.** Users must build named views first.
2. **Locked at binding time.** Switching scope ("show only my open tasks") means
   re-running the wizard.
3. **One filter per binding.** No way to combine "Sprint A AND Assignee me"
   without creating a dedicated Lark view for that combination.

Lark itself solves this with a per-table filter UI (top-right dropdown):
real-time, multi-condition AND/OR, no pre-setup. The right answer for Ansambel
is to mirror that UX and use Lark's `records/search` endpoint, which accepts a
JSON-structured filter body (conjunction + conditions list) with typed operators
(`is`, `isNot`, `contains`, `isEmpty`, etc.).

Result: zero pre-setup, real-time toggling, robust to view/column changes, and
the filtering work stays on Lark's server — no client-side scale cliff.

## Goals

- `BitableBinding` gains an optional `filters: FilterSpec` field — persisted per
  binding, defaults to empty (no conditions).
- New kanban-top "FilterBar" component with chip-based UI mirroring Lark's
  filter dropdown.
- `LarkProvider::list_tasks` routes: empty filter → existing
  `bitable_list_records` (GET) endpoint; non-empty → new
  `bitable_search_records` (POST) endpoint.
- Real-time UX: 300 ms debounce after a chip change → persist + refresh.
- Multi-condition AND/OR toggle at the top of the FilterBar.
- Field-rename robust: `field_id` is the stable key; `field_name` (required by
  Lark's search API in conditions) is refreshed from cache before send.
- Legacy bindings load with default empty filter — zero behavior change for
  users who don't reconfigure.

## Non-goals

- View-scope binding (deferred to / superseded by this spec — `view_id` is not
  on the new `BitableBinding`).
- Nested filter groups (Lark's search API doesn't support nesting; flat
  top-level conjunction matches Lark's UI).
- Client-side filtering (defeats the scale benefit).
- Filter expressions in the GET `list_records` `filter` query param (string
  grammar is less expressive + 2000-char limit; we use POST `search` instead).
- Per-workspace filter overrides — filter is per-binding (per-repo).
- Filter persistence across teammates — single-user assumption from Ansambel
  project rules.
- Field types beyond Common 6 (Text, SingleSelect, MultiSelect, DateTime,
  Number, Person). Checkbox, URL, Attachment, LinkedRecord, Lookup, Formula
  fields appear in the picker as "(not supported)" — easy to extend later.

## Architecture overview

```
┌──────────────────────────────────────────────────────────────┐
│ Frontend: FilterBar.svelte (sticky atas kanban)              │
│                                                              │
│  Meeting [all▾] of the conditions                            │
│  [Sprint Status: In Progress ×] [Assignee: Beni ×] [+ Add]   │
└──────────────────────────────────────────────────────────────┘
                       │ user edit chip → debounce 300ms
                       ▼
┌──────────────────────────────────────────────────────────────┐
│ Svelte store: lark-binding-filters.svelte.ts                 │
│  • optimistic local update                                   │
│  • invoke('set_lark_repo_binding', { ...binding, filters })  │
│  • invoke('refresh_tasks', { repoId })                       │
└──────────────────────────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────┐
│ Backend: LarkProvider::list_tasks                            │
│  if binding.filters.is_empty():                              │
│    bitable_list_records(app, table)         (existing)       │
│  else:                                                       │
│    bitable_search_records(app, table, &filters)  (NEW)       │
└──────────────────────────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────┐
│ Lark server: applies filter, returns scoped records          │
└──────────────────────────────────────────────────────────────┘
```

**Layering principle:** filter UI lives in the kanban surface, not in the
wizard. Wizard remains 3 steps (connection → field mapping → status mapping) —
purely binding setup. Filters are runtime config, edited inline.

## Components

### New types (Rust — `state.rs`)

```rust
/// One filter condition matching Lark's
/// `bitable/v1/.../records/search` condition schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FilterCondition {
    /// Bitable field id (stable lookup key — survives renames).
    pub field_id: String,
    /// Cached field name for UI display + outgoing API
    /// (Lark `search` uses field_name in conditions, not field_id).
    /// Refreshed from LarkProvider's field cache before each send.
    pub field_name: String,
    pub operator: FilterOperator,
    /// Per-type value (string for text, option name(s) for select,
    /// ISO-8601 for date, number as string, email or display name
    /// for person). Empty Vec for unary operators.
    pub value: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Is, IsNot, Contains, DoesNotContain,
    IsEmpty, IsNotEmpty,
    IsGreater, IsGreaterEqual, IsLess, IsLessEqual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterConjunction { And, Or }

impl Default for FilterConjunction {
    fn default() -> Self { Self::And }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FilterSpec {
    #[serde(default)]
    pub conjunction: FilterConjunction,
    #[serde(default)]
    pub conditions: Vec<FilterCondition>,
}

impl FilterSpec {
    pub fn is_empty(&self) -> bool { self.conditions.is_empty() }
}
```

### Modified types

```rust
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    /// Optional filter applied at the Lark server side via the
    /// `records/search` endpoint. Empty (default) → fetch all records
    /// via the existing list endpoint.
    #[serde(default)]
    pub filters: FilterSpec,                  // NEW
    pub field_mapping: FieldMapping,
    #[serde(default)]
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}
```

### New API surface

```rust
// LarkClient
pub async fn bitable_search_records(
    &self,
    app_token: &str,
    table_id: &str,
    filter: &FilterSpec,
) -> Result<Vec<BitableRecord>>;
```

POSTs to
`/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/records/search` with
body:

```json
{
  "filter": {
    "conjunction": "and",
    "conditions": [
      {
        "field_name": "Sprint Status",
        "operator": "is",
        "value": ["In Progress"]
      }
    ]
  }
}
```

Returns same `Vec<BitableRecord>` shape — `record_to_task` works unchanged.
Pagination via `page_token`/`has_more` same as existing list endpoint.

```rust
// commands/lark_repo_binding.rs — thin schema-fetch helper
pub(crate) async fn list_lark_fields_inner(
    app_token: &str,
    table_id: &str,
    data_dir: &Path,
    store: &dyn SecretStore,
) -> Result<Vec<BitableField>>;

#[tauri::command]
pub async fn list_lark_fields(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<BitableField>, String>;
```

Used by FilterBar's "+ Add filter" picker to enumerate columns. Mirrors the
`detect_lark_schema_inner` pattern (load cfg → swap tokens → delegate).

### Frontend types (TS)

```ts
export type FilterOperator =
  | 'is'
  | 'isNot'
  | 'contains'
  | 'doesNotContain'
  | 'isEmpty'
  | 'isNotEmpty'
  | 'isGreater'
  | 'isGreaterEqual'
  | 'isLess'
  | 'isLessEqual';

export type FilterConjunction = 'and' | 'or';

export type FilterCondition = {
  field_id: string;
  field_name: string;
  operator: FilterOperator;
  value: string[];
};

export type FilterSpec = {
  conjunction: FilterConjunction;
  conditions: FilterCondition[];
};

export type BitableBinding = {
  app_token: string;
  table_id: string;
  filters: FilterSpec; // NEW
  field_mapping: FieldMapping;
  status_value_mapping: StatusValueMapping;
  created_at: number;
  updated_at: number;
};
```

### Operator × field type matrix (UI)

| Field type       | Operators                                                                                      | Value picker                           |
| ---------------- | ---------------------------------------------------------------------------------------------- | -------------------------------------- |
| Text (1)         | `is`, `isNot`, `contains`, `doesNotContain`, `isEmpty`, `isNotEmpty`                           | `<input type="text">`                  |
| Number (2)       | `is`, `isNot`, `isGreater`, `isGreaterEqual`, `isLess`, `isLessEqual`, `isEmpty`, `isNotEmpty` | `<input type="number">`                |
| SingleSelect (3) | `is`, `isNot`, `isEmpty`, `isNotEmpty`                                                         | `<select>` populated from options      |
| MultiSelect (4)  | `contains`, `doesNotContain`, `isEmpty`, `isNotEmpty`                                          | Multi-select chips                     |
| DateTime (5)     | `is`, `isNot`, `isGreater`, `isGreaterEqual`, `isLess`, `isLessEqual`, `isEmpty`, `isNotEmpty` | `<input type="date">` (ISO yyyy-mm-dd) |
| Person (11)      | `is`, `isNot`, `contains`, `doesNotContain`, `isEmpty`, `isNotEmpty`                           | Free text (email / display name)       |

Other field types (Checkbox, URL, Attachment, LinkedRecord, Lookup, Formula)
appear in the picker greyed-out with "(not supported)" label.

## Data flow

### Read path (with active filter)

```
1. App starts OR binding loads:
   FilterBar reads binding.filters from larkBindings store → renders chips.

2. User clicks "+ Add filter":
   FilterBar fetches fields via api.lark.listFields(app, table) (lazy, cached
   per binding). Picker shows column list. User picks a column.
   → Operator dropdown populated per column's field_type.
   → Value picker rendered per column's field_type.

3. User edits value:
   → 300 ms debounce → filterStore.update(repoId, newSpec).
   → Optimistic: larkBindings.set(repoId, { ...binding, filters: newSpec }).
   → invoke('set_lark_repo_binding', { repoId, binding }).
   → invoke('refresh_tasks', { repoId }).

4. LarkProvider::list_tasks called:
   if binding.filters.is_empty():
     bitable_list_records(app, table)
   else:
     // Refresh field_name from cache before send (rename safety).
     let canonical = self.field_name_by_id().await;
     let refreshed_spec = refresh_field_names(&binding.filters, &canonical);
     bitable_search_records(app, table, &refreshed_spec)
   → returns Vec<BitableRecord>
   → record_to_task per row (unchanged)
   → AppState.tasks mirror updated
   → 'tasks-updated' event → kanban re-renders.
```

### Write path (create / move / update / delete task)

**Unchanged.** Filter is a read-only scoping concept. Writes always target the
table directly via `bitable_create_record` / `bitable_update_record` /
`bitable_delete_record`. A newly-created task may fall outside the active filter
— that's intentional (mirrors Lark UI: add a row in a filtered view, the row
exists but you don't see it until the filter clears).

### Field schema fetch (for FilterBar picker)

FilterBar needs the field list to populate the "+ Add filter" dropdown. New thin
helper `list_lark_fields` returns just `Vec<BitableField>` (lighter than the
existing `detect_lark_schema` which returns full `ProposedMapping`). Cached in
component-local state per binding; refresh button in the picker if user added a
field in Lark and doesn't see it.

### Filter store (Svelte 5 runes)

```ts
// src/lib/stores/lark-binding-filters.svelte.ts (NEW)

export class FilterStore {
  private timeouts = new Map<string, number>();

  async update(repoId: string, spec: FilterSpec): Promise<void> {
    const current = larkBindings.get(repoId);
    if (!current) return;

    // Optimistic local update.
    larkBindings.bindings.set(repoId, { ...current, filters: spec });

    // Debounced persist + refresh.
    clearTimeout(this.timeouts.get(repoId));
    this.timeouts.set(
      repoId,
      window.setTimeout(async () => {
        try {
          await api.lark.setRepoBinding(repoId, { ...current, filters: spec });
          await api.task.refresh(repoId);
        } catch (err) {
          // Revert optimistic update.
          larkBindings.bindings.set(repoId, current);
          addToast(`Filter save failed: ${err}`, 'error');
        }
      }, 300)
    );
  }
}

export const filterStore = new FilterStore();
```

## Migration

Same idempotent no-op pattern as Phase 3a-3.1 v1's view-scope migration:

```rust
// persistence/lark_repo_bindings.rs
fn default_schema_version() -> u32 { 3 }     // bumped from 1 (no v2 ever shipped)

pub(crate) fn migrate_v1_to_v3(data_dir: &Path) -> Result<u32> {
    let mut file = load_bindings(data_dir)?;
    if file.schema_version >= 3 {
        return Ok(file.schema_version);
    }
    file.schema_version = 3;
    save_bindings(data_dir, &file)?;
    Ok(3)
}
```

**Note:** PR #27 introduced a v2 migrator (`migrate_v1_to_v2` + `view_id` field)
but never merged. Since the only schema in production is v1, we jump straight to
v3 and skip the v2 stop. `migrate_v1_to_v3` is non-fatal in `setup()`
(warn-on-error + continue), called immediately after data_dir resolves.

`filters: FilterSpec` field uses `#[serde(default)]` so legacy v1 bindings
deserialize with empty filters → behavior identical to today until user
configures one.

## Error handling

- **Filter expression rejected by Lark** (wrong field type, malformed value):
  Lark returns `1254xxx` with descriptive `msg`. LarkProvider propagates as
  `AppError::Lark`. Frontend toast surfaces the message; FilterBar marks the
  offending chip with a red border + tooltip.
- **Field deleted in Lark** (field_id no longer in schema): on the next schema
  cache refresh (binding save or app restart), FilterBar detects the missing
  field_id, marks the chip stale + offers "Remove broken filter". Auto-removal
  NOT done — user awareness is more important.
- **Field renamed in Lark** (field_id same, field_name changed): LarkProvider
  caches `field_name_by_id` from `bitable_list_fields` (via `OnceCell`,
  refreshed per LarkProvider lifetime). Before building the search body, each
  `FilterCondition.field_name` is overwritten with the canonical value from the
  cache. Saving the binding from FilterBar rebuilds the LarkProvider with a
  fresh cache.
- **Network 5xx / rate limit**: existing `send_with_retry` handles 429. Other
  failures surface as `AppError::Lark` → toast; FilterBar stays open with
  current chips; user retries by toggling.
- **Empty result from search**: not an error; kanban renders all 4 columns
  empty.
- **Operator/value mismatch** (e.g. `isGreater` on Text): enforced in FilterBar
  UI per-type matrix; backend trusts the conditions array.

## Removed from previous (PR #27) design

PR #27 (closed without merge) introduced view-scope. The following artifacts
from that design are **NOT** in this v2:

- `BitableView` type, `bitable_list_views` helper, `list_lark_views` Tauri
  command.
- `view_id: Option<String>` on `BitableBinding`.
- Wizard Step 1.5 view picker.
- `ViewMissingSink` trait + `TauriViewMissingSink` adapter + `lark-view-missing`
  Tauri event + view-missing Svelte store + TitleBar banner.
- `is_view_missing_error` matcher + view-404 fallback logic in
  `LarkProvider::list_tasks`.

Carryover from main (already shipped via PR #26, not view-specific):

- `bitable_list_records(app, table)` — kept as-is for the empty-filter fast
  path. No signature change.
- `BitableField` types + `detect_lark_schema` infra — re-used by the new
  `list_lark_fields_inner` helper and FilterBar picker.

PR #27 itself contributed no code to main (closed unmerged). Implementers should
branch off main, not PR #27.

## Testing

### Unit tests (Rust)

**`lark_client.rs` — `bitable_search_records`:**

- `bitable_search_records_posts_filter_body_with_and_conjunction`
- `bitable_search_records_posts_or_conjunction`
- `bitable_search_records_paginates`
- `bitable_search_records_surfaces_non_zero_code_as_error`
- `bitable_search_records_handles_empty_conditions`

**`state.rs` — `FilterSpec` serialization:**

- `filter_spec_default_is_and_with_empty_conditions`
- `filter_spec_roundtrips_through_json`
- `filter_condition_serializes_operator_as_lark_camel_case`
- `legacy_binding_without_filters_loads_as_default_empty`

**`persistence/lark_repo_bindings.rs` — v1→v3 migration:**

- `migrate_v1_to_v3_bumps_version_and_preserves_bindings`
- `migrate_v1_to_v3_is_idempotent`
- `migrate_v1_to_v3_no_op_when_file_absent`
- `default_schema_version_is_3`

**`task_provider/lark.rs` — `LarkProvider::list_tasks` routing:**

- `lark_provider_uses_list_endpoint_when_filters_empty`
- `lark_provider_uses_search_endpoint_when_filters_non_empty`
- `lark_provider_search_caches_field_name_by_id`
- `lark_provider_search_refreshes_field_name_from_cache`

**`commands/lark_repo_binding.rs` — `list_lark_fields`:**

- `list_lark_fields_inner_returns_fields_via_lark_client`
- `list_lark_fields_inner_errors_when_creds_missing`

### Integration tests (wiremock in `lib.rs`)

- `filter_aware_binding_uses_search_endpoint_end_to_end`
- `binding_with_empty_filters_uses_list_endpoint_end_to_end`

### Frontend component tests (Vitest + Testing Library)

**`FilterBar.svelte`:**

- renders empty state with "+ Add filter" button when no filters
- "+ Add filter" populates column dropdown from `list_lark_fields`
- selecting a column shows correct operators for its field type
- selecting value triggers debounced `setRepoBinding` + refresh after 300 ms
- chip × removes condition and refreshes
- AND/OR toggle changes conjunction in saved spec
- broken-field chip (id not in current schema) shows red border + Remove

**`lark-binding-filters.svelte.test.ts`:**

- optimistic update lands immediately, persisted after debounce
- failed persist reverts the optimistic update and toasts

### E2E (Playwright, env-gated)

`tests/e2e/phase-3a-3-1/phase-3a-3-1-filter-bar.spec.ts`:

1. `filter bar adds Sprint Status filter and kanban narrows`
2. `removing all filters restores all records`

### Coverage gate

95 % line + branch + function on changed files per project rule.
`commands/lark_repo_binding.rs` stays in the CI ignore-regex; `_inner` helpers
tested, Tauri wrappers covered by E2E.

## Decision log

| Decision                | Choice                                                               | Why                                                                           |
| ----------------------- | -------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Filter mechanism        | Lark `records/search` POST endpoint                                  | JSON body — no escaping; typed operators match UI 1:1; no 2000-char limit     |
| Filter persistence      | In `BitableBinding` (file-persisted)                                 | User's typical workflow is stable focus on one slice for days                 |
| Conjunction             | Toggle AND / OR                                                      | Match Lark UI; both supported by `search` API                                 |
| Field type coverage     | Common 6 (Text, Number, SingleSelect, MultiSelect, DateTime, Person) | YAGNI; covers ~90 % of kanban filter use; easy to extend later                |
| UI placement            | Sticky chip-bar above kanban                                         | Always visible; matches Lark UX; minimum clicks for the always-on filter case |
| Apply mode              | Real-time, 300 ms debounce                                           | Mirrors Lark; instant feedback; debounce bounds API call rate                 |
| Empty filter            | Fetch all records via list endpoint                                  | Zero behavior change for users who don't configure filters                    |
| Field-rename robustness | OnceCell cache `field_name_by_id`; refresh before send               | Lark search API uses field_name in conditions; survives rename                |
| Wizard interaction      | Wizard unchanged (3 steps)                                           | Filter is runtime config, not binding setup; clean separation                 |
| Schema migration        | v1 → v3 (skip v2)                                                    | PR #27's v2 never shipped; no installs at v2 in the wild                      |

## Open questions (none blocking)

- "What if user picks a Lookup or Formula field?" → Picker shows the field as
  "(not supported)" — disabled option. Future phase can add per-type support if
  demand surfaces.
- "What if Lark adds a new operator we don't know about?" → Picker only surfaces
  operators in our local enum; safe extension path.
- "Can FilterBar be hidden/collapsed?" → Not in v1; chip bar always shows even
  when empty (just shows "+ Add filter"). Collapse can be a future preference.

## What's NOT covered

- View-scope binding (deferred indefinitely; this design supersedes).
- Saved filter presets ("My filters" sidebar — pick from a saved list of named
  filters). YAGNI for v1 since binding-level persistence already covers the
  common case.
- Cross-table joins / linked-record filters — Lark API limitation.
- Filter sharing across teammates — single-user Ansambel scope.
- Filter export / import to/from Lark URL — speculative.
