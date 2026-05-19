# Journal — 2026-05-19 — Phase 3a-3.1 filter-aware Lark binding + PIC cards

## What shipped

Phase 3a-3.1 ships server-side filtering against Lark Bitables driven by an
in-app filter bar that mirrors Lark's filter popover. This supersedes the
view-scope approach from the closed PR #27: the app now generates a
`records/search` filter expression directly instead of relying on a
pre-configured Lark view, so filters can be toggled per session, support AND/OR
with multiple conditions, and never require a Lark admin to create a view first.
The branch (`feat/phase-3a-3-1-filter-aware-lark-binding`) landed 60 commits and
was iterated against a live Bitable in tight smoke- test loops with the user.

The last commit (a4d01c8) replaces the relative-date footer on task cards with a
PIC (person-in-charge) label, because Lark-imported records arrive with
`created_at ≈ 0` so every card was showing nonsense like "20592d ago".

## Backend

- `state.rs`: new `FilterOperator` enum (`is`, `isNot`, `contains`,
  `doesNotContain`, `isEmpty`, `isNotEmpty`, `isGreater`/`Equal`,
  `isLess`/`Equal`) serialising to Lark's camelCase operators.
  `FilterConjunction` (and/or).
  `FilterCondition { field_id, field_name, operator, value: Vec<String> }`.
  `FilterSpec` joins them with one top-level conjunction.
  `BitableBinding.filters: FilterSpec` (serde default).
- `persistence/lark_repo_bindings.rs`: schema bumped from v1 to v3 (no v2 ever
  shipped). `migrate_v1_to_v3` runs idempotently at setup and injects an empty
  `filters` block + status-value-mapping default for legacy bindings.
- `platform/lark_client.rs`: new `bitable_search_records` calling
  `bitable/v1/apps/{app}/tables/{table}/records/search` with the filter
  envelope. Strips `field_id` from outgoing condition bodies (Lark rejects it)
  and falls back to refreshing field names from the schema cache when a
  `FieldName invalid` error returns. Honours `user_id_type=open_id` for Person
  fields. Logs full response body on `InvalidFilter` (1254018) so the next
  iteration can see what Lark didn't like.
- `task_provider/lark.rs`: `LarkProvider` now holds the active `FilterSpec` + a
  `tokio::sync::OnceCell<HashMap<field_id, field_name>>` cache used to rewrite
  stale `field_name`s before each search. `list_tasks` routes to
  `bitable_search_records` when filters are present and the existing list
  endpoint otherwise. `strip_empty_conditions` drops conditions with
  whitespace-only values so partially-filled filter UI doesn't trip
  `InvalidFilter`.
- `task_provider/lark_field_resolver.rs`: `extract_single_select` now unwraps
  Lark's Lookup wrapper shape `{type, value: [<inner>]}` and the segmented-text
  array shape from the search endpoint. `resolve_status` recovers option ids
  from names when the search endpoint omits ids, then falls through to
  case-insensitive entry matching for legacy bindings. Status options
  pre-fetched eagerly on the provider so the recover path has data on first
  call. New `resolve_pic` reads the optional PIC field, handling Person arrays,
  single user objects, plain Text (split on `,;/&` and `" and "`), and segmented
  text from the search endpoint. `record_to_task` populates `pic_names`.
- `state.rs`: `FieldMapping.pic: Option<FieldRef>` and
  `Task.pic_names: Vec<String>`, both `serde(default)` so persisted data
  deserialises unchanged. 29 test fixtures patched across the crate to satisfy
  the new mandatory fields in non-test code.
- 3 new Tauri commands: `list_lark_fields` (used by the FilterBar to populate
  the field picker), `list_lark_person_options` (Person field dropdown for
  filter values), `list_lark_lookup_options` (follows a Lookup field's chain to
  its source single-select and returns the options — `target_table` lives under
  the nested `filter_info` key, not at the top level, which cost ~3 iterations
  to discover).

## Frontend

- `lark-binding-filters.svelte.ts`: filter store with debounced persist (300ms)
  and optimistic update. Preserves the true baseline across rapid edits so
  cancelling reverts to the actually-saved state, not whatever was in-flight.
- `FilterBar.svelte`: Lark-style popover-driven filter chip row. Empty state
  shows a "+ Add filter" button; each condition is a removable chip with the
  field name and value. Field picker hides fields already mapped to
  `title`/`description`/`status`/`order` (matched by id with name as a fallback
  for bindings whose `field_id` drifted). Operator list narrows per field type.
  Value editor renders:
  - SingleSelect: native `<select>` of the field's options.
  - Person: user-picker dropdown populated from `list_lark_person_options`.
  - Lookup: chain-resolved dropdown via `list_lark_lookup_options`; the
    `<option value>` is the option **name**, not the option id, because Lark
    filters Lookup fields by name even though records store option ids.
  - Text/Number/Date: plain input.
  - Empty input + non-unary operator: condition is stripped before send. Loading
    placeholders ("Loading fields…", "Loading options…") render while the IPC
    call is in flight.
- `LarkBindingWizard.svelte`: step 2 gains a PIC picker filtered to Person (11),
  Text (1), Created By (1003), Modified By (1004) field types. Existing PIC
  selection is preserved across re-detect. Inline progress text ("Detecting
  schema…", "Saving binding…") visible during IPC calls.
- `KanbanBoard.svelte`: while `tasksStore.isLoading(repoId)` is true and the
  repo has a Lark binding, columns render a loading placeholder ("Loading
  filtered view…" when a filter is active, "Loading tasks…" otherwise) instead
  of "No tasks". Local-mode repos behave unchanged.
- `TaskCard.svelte`: footer renders the PIC label — `"—"` empty, the single name
  when there's one PIC, `"Alice +N"` for multiple with the full comma-joined
  list in the `title` tooltip. The relative-date display is removed.
- `RepoSettingsDialog.svelte`: `handleSaveBinding` calls
  `tasks.loadForRepo(repoId)` after `setBinding` returns, so a new field mapping
  (PIC especially) takes effect without an app restart. The backend already
  rebuilds the `LarkProvider` on save; this just clears the stale cached rows in
  the frontend store.

## Theme polish along the way

The work surfaced a Tailwind/theme mismatch: the codebase uses CSS custom
properties on `document.documentElement` via `data-mode` /
`ThemeStore. applyCssVars`, but several new popovers had been written with
Tailwind's `dark:` variants (which read `prefers-color-scheme`, the OS-level
setting, not the in-app theme). All filter UI was rewritten to read
`var(--bg-card)`, `var(--bg-base)`, `var(--text-primary)`, etc. directly.
`ThemeStore.applyCssVars` also sets
`document.documentElement.style. colorScheme = mode` so native `<select>` /
`<option>` painted by the browser engine (Linux WebKit) follow the theme.

`var(--bg-hover)` (the lighter hover-state colour) was misused as the input
background in early iterations and looked low-contrast; replaced with
`var(--bg-base)` (sunken).

## Bug hunt highlights

- **Person filter InvalidFilter (1254018)** — 4 iterations. Final fix:
  `is`/`isNot` operator + `open_id` in the value array + `user_id_type= open_id`
  query param. `contains` rejected by Lark for Person; `is` with display name
  also rejected.
- **All tasks landing in Todo when filtering** — caused by status resolution
  failing because the search endpoint returns SingleSelect as
  `{type:3, value:["Done"]}` (Lookup wrapper) rather than the plain object the
  list endpoint returns. `extract_single_select` now unwraps both.
- **Sprint Status options empty in the filter dropdown** — Lookup field's
  `target_table` property key lives under `filter_info`, not at the top level.
  Discovered by logging the raw property JSON, then read both paths in the
  resolver (`prop.target_table` || `prop.filter_info. target_table`).
- **Lookup filter rejected after the dropdown finally rendered options** — Lark
  accepts option **name** for Lookup `is`, not option id, even though records
  store ids. Changed `<option value>` to render `opt.name`.
- **InvalidFilter on add-condition** — backend sent `field_id` in the outgoing
  condition object; Lark search rejects it. Stripped at the serialiser.
- **Empty-value condition tripping InvalidFilter** — the moment a condition was
  created (with no value yet), it shipped to Lark and failed.
  `strip_empty_conditions` filters before send.

## Tests

- Rust: 663 unit tests passing. New tests cover the search endpoint routing, all
  `extract_single_select` shapes, `resolve_status` recovery paths, every
  `resolve_pic` shape (Person array, single object, en_name / text fallback,
  plain text, comma/semicolon/slash/ampersand split, " and " split, segmented
  text, whitespace skip), v1→v3 migration idempotency, and
  `strip_empty_conditions`.
- Frontend: 791 vitest cases passing. New tests for `FilterBar` (popover
  open/close, per-type operator/value rendering, conjunction toggle, chip
  remove, loading placeholders), `KanbanBoard` loading-state rendering,
  `LarkBindingWizard` PIC picker, and `TaskCard` PIC display variants.
- E2E: env-gated Playwright smoke (`tests/e2e/phase-3a-3-1-filter-bar. spec.ts`)
  drives the wizard, opens FilterBar, adds a condition, and asserts the kanban
  list narrows. Runs locally with `ANSAMBEL_LARK_FIXTURE=1`.

## Aftermath

PR #28 is open against `main`. PR #27 (view-scope approach) was closed without
merge.

There's no production logging of filter-roundtrip metrics yet — the diagnostic
`info!` logs added during debugging were removed before shipping. If we need to
debug filter behaviour against a customer's Bitable later, those logs are in the
git history (commits 9046501, d482ff4) and can be re-enabled selectively.

A follow-up the user has mentioned but not requested implementation for yet:
clicking a task card to open a Lark-style detail panel (read-only or editable)
with all fields, not just the four mapped ones. Three options sketched out in
conversation; design + plan still to write.
