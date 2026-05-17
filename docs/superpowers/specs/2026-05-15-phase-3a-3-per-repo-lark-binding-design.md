# Phase 3a-3 — Per-Repo Lark Binding + Field Mapping Wizard

**Status:** Approved — ready for plan. **Phase predecessor:**
[3a-2 TaskProvider abstraction](2026-05-13-phase-3a-2-task-provider-design.md)
(merged via PR #25). **Goal:** Ansambel adapts to any Bitable structure. Each
repo binds to one Bitable via an explicit, user-confirmed field mapping — no
more hardcoded `kanban_column`/`title`/`repo_id` schema requirement.

## Why

Phase 3a-2 shipped a working Lark TaskProvider but with a rigid contract: the
Bitable must expose 5 canonical fields (`title`, `description`, `kanban_column`,
`repo_id`, `order_within_column`). Real-world Bitables don't match — the user's
own Board Coba uses `Task name` for title, `Task Status` for kanban status, and
has no concept of `repo_id`. The runtime fallback chain (primary-column
fallback, fuzzy parser, status field auto-detect) papers over this for one
Bitable, but breaks down when:

- Same user wants different Bitables for different repos
- Bitable uses non-English field names (Spanish "Estado", Chinese 状态)
- Bitable has no status field at all
- Multiple status-like fields exist (auto-detect picks one arbitrarily)
- Field gets renamed in Bitable (name-based lookup silently fails)

Phase 3a-3 inverts the model: instead of forcing the Bitable to match Ansambel's
schema, Ansambel learns the Bitable's schema via a wizard, persists the mapping,
and uses field IDs for stable lookup across renames.

## Scope

In scope:

- Per-repo Bitable binding with field mapping
- 3-step wizard (Connect → Field map → Status value map) with auto-detect
  initial guess
- Field-ID-stable lookup (with `field_name` cached for display)
- Auto-migration from Phase 3a-2 global config to per-repo binding
- Schema verify wizard from Phase 3a-2 deprecated and removed
- `task_source` enum deprecated (derived from binding presence)
- Per-repo provider handle (replaces app-global provider)

Out of scope (deferred):

- Real-time push from Lark (still focus-refresh based)
- Multi-Bitable per repo (1:1 only)
- Conflict resolution beyond last-write-wins
- Other providers (Jira, Linear) — TaskProvider trait remains generic, but
  config UI is Lark-specific
- Schema migration assistant for option-list changes

## Architecture overview

```
Settings UI (Svelte)
  └─ RepoSettingsDialog (NEW per-repo settings)
     └─ Lark Sync section
        └─ LarkBindingWizard (NEW)
           - Step 1: Connect (app_token + table_id)
           - Step 2: Field mapping (auto-detected + editable)
           - Step 3: Status value mapping (single-select only)

IPC layer (Tauri commands)
  - get/set/delete_lark_repo_binding(repo_id, ...)
  - detect_lark_schema(app_token, table_id) → ProposedMapping
  - list_lark_repo_bindings()

Backend (Rust)
  - persistence::lark_repo_bindings — file I/O (NEW)
  - commands::lark_repo_binding — Tauri shims (NEW)
  - task_provider::lark::LarkProvider — accepts FieldMapping in constructor (REFACTOR)
  - task_provider::lark::lark_field_resolver — pure logic (NEW)
  - task_provider::lark::BitableSchemaDetector — proposes mapping (NEW)
  - task_provider::schema (DELETED — schema verify deprecated)
  - state::TaskProviderHandle now Arc<RwLock<HashMap<RepoId, Arc<dyn TaskProvider>>>>

Storage (app data dir)
  - lark_settings.json — shrinks to { app_id, base_url } only (global identity)
  - lark_repo_bindings.json — { schema_version, bindings: { repo_id → BitableBinding } } (NEW)
  - app_settings.task_source — DELETED (derived from binding presence)
  - keyring app_secret — unchanged
```

**Boundary principles:**

- `FieldMapping` is data only — no logic
- `lark_field_resolver` is pure logic — easy to unit-test with arbitrary
  mappings
- `BitableSchemaDetector` does I/O — separate from runtime resolver
- `LarkProvider` is a thin wrapper — delegates field resolution
- Wizard component is UX only — talks to IPC, doesn't know mapping internals

## Data model

### `BitableBinding` (the per-repo binding)

```rust
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    pub field_mapping: FieldMapping,
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}
```

### `FieldMapping`

```rust
pub struct FieldMapping {
    pub title: FieldRef,                   // required
    pub description: Option<FieldRef>,     // optional → empty string
    pub status: Option<FieldRef>,          // optional → default_column applied to all rows
    pub order: Option<FieldRef>,           // optional → created_time DESC sort
}

pub struct FieldRef {
    pub field_id: String,        // stable primary lookup key (Lark API stable)
    pub field_name: String,      // cached for UI display, lazily refreshed
}
```

Notes:

- `repo_id` is NOT a Bitable field. The binding context (which repo owns this
  binding) provides it. At hydrate, all rows from this Bitable are tagged with
  the owning repo's id.
- Field-ID is stable across renames; field-name cache is refreshed on each
  `bitable_list_fields` call (during wizard open or via background heal).

### `StatusValueMapping`

```rust
pub struct StatusValueMapping {
    /// Maps Bitable single-select option_id (or lowercased text value if status
    /// is a Text field) → KanbanColumn.
    pub entries: HashMap<String, KanbanColumn>,

    /// Default for values present in Bitable but missing from `entries`, and
    /// for values that don't match via fuzzy parser fallback.
    pub default_column: KanbanColumn,
}
```

When status field is **single-select**: keys are `option_id`. Stable across
option-label renames. When status field is **text**: keys are lowercased
option-text. Fuzzy parser remains as runtime fallback for unrecognized values.

### `lark_repo_bindings.json` format

```json
{
  "schema_version": 1,
  "bindings": {
    "repo_xxx": {
      "app_token": "bascntest",
      "table_id": "tbltest",
      "field_mapping": {
        "title": { "field_id": "fld_pri", "field_name": "Task name" },
        "description": null,
        "status": { "field_id": "fld_status", "field_name": "Task Status" },
        "order": null
      },
      "status_value_mapping": {
        "entries": {
          "option_aaa": "todo",
          "option_bbb": "in_progress",
          "option_ccc": "review",
          "option_ddd": "in_progress",
          "option_eee": "done"
        },
        "default_column": "todo"
      },
      "created_at": 1747200000,
      "updated_at": 1747200000
    }
  }
}
```

### `lark_settings.json` (shrunk to global identity)

```json
{
  "app_id": "cli_xxxxxxxxxxxxxxxx",
  "base_url": "https://open.larksuite.com"
}
```

`app_secret` remains in OS keyring. `app_token` and `table_id` removed (now
per-repo).

### `AppSettings.task_source` — DELETED

The presence of a binding for the selected repo determines provider mode at
runtime. Frontend derives:

```ts
const isLarkSyncEnabled = $derived(larkBindings.has(selectedRepo.id));
```

## Component design

### Backend (Rust)

```
src-tauri/src/
├── persistence/
│   └── lark_repo_bindings.rs       NEW
│       ├── load_bindings(data_dir) -> Result<BindingsFile>
│       ├── save_bindings(data_dir, &BindingsFile) -> Result<()>
│       ├── get_binding(data_dir, repo_id) helper
│       └── set/delete_binding(data_dir, repo_id) helpers
│
├── commands/
│   ├── lark_repo_binding.rs        NEW (replaces parts of lark_auth)
│   │   ├── get_lark_repo_binding(repo_id)
│   │   ├── set_lark_repo_binding(repo_id, BitableBinding)
│   │   ├── delete_lark_repo_binding(repo_id)
│   │   ├── list_lark_repo_bindings()
│   │   └── detect_lark_schema(app_token, table_id)
│   │       — uses keyring app_secret + global app_id
│   │
│   ├── lark_auth.rs                SHRINKS
│   │   — keeps app_id/secret/base_url commands; drops app_token/table_id
│   │
│   └── task.rs                     REFACTOR
│       — provider lookup by repo_id (multi-provider map)
│
├── task_provider/
│   ├── lark.rs                     REFACTOR
│   │   └── LarkProvider::new(client, app_token, table_id, mapping, status_values)
│   │
│   ├── lark_field_resolver.rs      NEW
│   │   ├── resolve_title(record, mapping, primary_field_id) -> Result<String>
│   │   ├── resolve_status(record, mapping, values) -> KanbanColumn
│   │   ├── resolve_order(record, mapping) -> i32
│   │   └── BitableSchemaDetector
│   │       └── propose_mapping(client, app_token, table_id) -> ProposedMapping
│   │
│   └── schema.rs                   DELETED
│
├── state.rs                        CHANGE
│   ├── TaskProviderHandle = Arc<RwLock<HashMap<RepoId, Arc<dyn TaskProvider>>>>
│   └── AppSettings.task_source field — DELETED (+ skip on serde load for backward-compat)
│
└── lib.rs                          CHANGE
    — startup: load bindings, build provider per-repo, run migration if applicable
```

`ProposedMapping` (returned by `detect_lark_schema`):

```rust
pub struct ProposedMapping {
    pub fields: Vec<BitableField>,          // all fields, for dropdown population
    pub suggested: FieldMapping,            // auto-detected initial values
    pub status_options: Option<Vec<BitableOption>>,  // only if status is single-select
    pub suggested_status_values: StatusValueMapping, // fuzzy-parser output for initial fill
}
```

### Frontend (Svelte)

```
src/lib/
├── components/
│   ├── SettingsDialog.svelte        CHANGE
│   │   — removes "Task source" radio
│   │   — removes embedded LarkSettings
│   │   — becomes hub: General · Lark Open Platform (global creds) · Help
│   │
│   ├── lark/
│   │   ├── LarkSettings.svelte      RENAME → LarkGlobalSettings.svelte
│   │   │   — keeps app_id/secret/base_url form; drops app_token/table_id/schema verify
│   │   │
│   │   └── LarkBindingWizard.svelte NEW
│   │       — 3 internal steps via {#if step === 1}...{:else if step === 2}
│   │       — auto-detect via IPC on Step 1 "Detect" click
│   │
│   ├── repo/
│   │   └── RepoSettingsDialog.svelte NEW
│   │       └─ Lark Sync section:
│   │          - "Not connected" → [Connect to Lark Bitable] → wizard
│   │          - "Connected: <table_name>" → [Edit] [Disconnect]
│   │
│   └── kanban/
│       └── KanbanBoard.svelte        unchanged
│
├── stores/
│   ├── lark-bindings.svelte.ts      NEW
│   │   — SvelteMap<repo_id, BitableBinding>
│   │   — listens for `binding-updated` event
│   │   — optimistic update + revert (mirrors tasks.svelte.ts pattern)
│   │
│   └── tasks.svelte.ts              CHANGE
│       — refresh on `binding-updated` for selected repo
│
└── ipc.ts                            CHANGE
    — api.lark.getRepoBinding(repoId) / setRepoBinding / deleteRepoBinding
    — api.lark.detectSchema(appToken, tableId)
    — api.lark.listRepoBindings
    — removes api.task.getSource/setSource
    — removes api.lark.verifySchema
```

### IPC mapping (old → new)

| Phase 3a-2 IPC                        | Phase 3a-3 IPC                                                                                    | Notes                                      |
| ------------------------------------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `get_lark_status()`                   | `get_lark_repo_binding(repo_id)`                                                                  | Per-repo                                   |
| `set_lark_credentials`                | Split: global creds via `set_lark_global_credentials` (kept), binding via `set_lark_repo_binding` | Separation                                 |
| `test_lark_connection`                | `detect_lark_schema(app_token, table_id)`                                                         | Combined: validates creds + fetches schema |
| `verify_lark_schema`                  | **DELETED**                                                                                       |                                            |
| `get_task_source` / `set_task_source` | **DELETED**                                                                                       | Derived from binding                       |
| `refresh_tasks(repoId?)`              | `refresh_tasks(repoId?)`                                                                          | Unchanged                                  |

## UX flow

### Entry point: per-repo settings

Right-click a repo in the sidebar → context menu → **Settings** → opens
`RepoSettingsDialog` with sections:

- General (default branch, gh profile)
- Scripts
- Lark Sync

The Lark Sync section state:

- **Not connected:** card showing "Sync your kanban with a Lark Bitable" +
  `[Connect to Lark Bitable]` button → opens wizard at Step 1
- **Connected:** card showing `✓ Connected: <table_name>` +
  `<bitable_token...>/<table_id...>` + last refresh timestamp +
  `[Edit mapping] [Disconnect]` buttons

### Wizard: Step 1 — Connect

Inputs: `App Token`, `Table ID`. Info: "App ID & secret use global config".

`[Detect →]` button calls `detect_lark_schema(app_token, table_id)`. On error:

- 91402 → inline: "Bitable not found. Check app_token/table_id."
- 91403 → inline: "Permission denied. Add bot 'Ensemble App' as collaborator in
  Bitable. [Help link]"
- Network → inline: "Connection timed out. Retry?"
- Auth → inline: "Lark auth failed: {code}. Verify global app_id/secret."

Stay on Step 1 until success.

### Wizard: Step 2 — Field mapping

Pre-filled with `suggested` from `ProposedMapping`. Each field shown as a
dropdown of all Bitable fields:

```
Title* [Task name ▾]        (auto-detected: primary column)
Description [(none) ▾]
Status [Task Status ▾]      (auto-detected: single-select)
  ⚠ Without this, all tasks default to Todo
Order [(none) ▾]
  ℹ Default sort: created_time DESC
```

Title is required (red asterisk, disable Continue until set). If Status field is
a single-select, Continue advances to Step 3; otherwise saves directly (Step 3
skipped).

### Wizard: Step 3 — Status value mapping

Shown only if Status is single-select. Lists all options of that field with a
column dropdown for each:

```
"To Do"           → [Todo         ▾]
"In Progress"     → [In Progress  ▾]
"Waiting Review"  → [Review       ▾]
"In Review"       → [Review       ▾]
"Waiting Fix"     → [In Progress  ▾]
"Waiting Deploy"  → [Done         ▾]
"Delivered"       → [Done         ▾]

Default for unmapped values: [Todo  ▾]
```

Initial values come from `suggested_status_values` (fuzzy parser output). User
confirms/edits.

`[Save & Sync]` → backend:

1. Persist `BitableBinding` to `lark_repo_bindings.json`
2. Construct `LarkProvider(mapping)` and insert into `provider_handle[repo_id]`
3. Trigger `list_tasks(Some(repo_id))` (initial hydrate)
4. Emit `binding-updated` event
5. Toast: "✓ Connected & loaded N tasks"

### Edit binding

`[Edit mapping]` opens wizard with existing values, jumps to Step 2 (Step 1
skipped — creds proven valid since binding exists). User adjusts, saves.

### Disconnect

`[Disconnect]` → confirm modal: "Tasks in kanban will be replaced by local
tasks.json. Continue?" → backend:

1. Delete entry from bindings file
2. Swap `provider_handle[repo_id]` to `LocalProvider`
3. Re-hydrate from `tasks.json`
4. Emit `binding-updated` → UI refresh

### Migration on first launch after Phase 3a-3 upgrade

On startup, after loading state:

```
if no binding for selected_repo
AND old lark_settings.json has app_token + table_id
AND old app_settings.task_source was "lark"
AND selected_repo is set:

    show non-modal banner: "Migrating Lark config to per-repo binding..."
    auto-call detect_lark_schema(old.app_token, old.table_id)
    if success:
        save BitableBinding for selected_repo with proposed mapping
        remove app_token + table_id from lark_settings.json (atomic rewrite)
        remove task_source field from app_settings.json
        toast: "Lark migrated to <repo_name>. Click to review mapping."
        click toast → opens RepoSettingsDialog at Lark Sync section
    if fail:
        log warn, leave old config intact (will be cleaned up once user
        successfully creates a binding via wizard)
        no toast (don't bother user with pre-emptive error)
```

Idempotent: if binding already exists, skip silently. If old config absent,
no-op.

## Migration plan

### Files affected on first run

1. **Read** `lark_settings.json` (old format, has app_token + table_id)
2. **Read** `app_settings.json` (has `task_source` field)
3. **Write** `lark_repo_bindings.json` (new file with one entry)
4. **Rewrite** `lark_settings.json` removing app_token/table_id
5. **Rewrite** `app_settings.json` removing `task_source`

All writes are atomic (existing `.tmp + rename` pattern from Phase 1).

### Code locations needing serde tolerance

- `commands::lark_auth::LarkSettings` struct — drop `app_token`/`table_id`
  fields, but use `#[serde(default)]` on reload for backward compat
- `state::AppSettings` — drop `task_source` field, ensure deserialize ignores
  unknown fields (existing serde default behavior)

### Backward-compatibility horizon

- New format: schema_version = 1 in bindings file
- Old format: detect via missing bindings file + presence of app_token in
  lark_settings.json
- After migration: old fields cleared, future upgrades start from schema_version
  = 1
- If user downgrades to Phase 3a-2: bindings file ignored (Phase 3a-2 doesn't
  know about it), `app_token`/`table_id` missing from `lark_settings.json` →
  user re-enters via old UI. Acceptable cost; we don't formally support
  downgrade.

## Error handling

### Wizard time — inline, retryable

Surface every failure mode inline in the step that triggered it. Don't dismiss
the wizard. Don't toast (toasts are for runtime; wizard errors are immediate
context).

| Failure                               | Detection                    | UX                                                                     |
| ------------------------------------- | ---------------------------- | ---------------------------------------------------------------------- |
| Wrong app_token/table_id              | 91402 Bitable not found      | Step 1 inline banner                                                   |
| Bot not shared                        | 91403 Forbidden              | Step 1 inline banner with help link                                    |
| Network timeout                       | reqwest timeout              | Step 1 inline banner, retry button                                     |
| Auth fails                            | tenant_access_token non-zero | Step 1 inline banner pointing to global Settings                       |
| Title required missing                | Frontend validation          | Step 2 Continue disabled + red asterisk                                |
| Field structure changed between steps | Detected during Step 3 fetch | Auto-back to Step 2 with toast: "Re-pick status field"                 |
| Save IPC fails                        | Backend error                | Toast: "Save failed: {err}". Wizard stays open. Binding NOT persisted. |

### Runtime — graceful degradation + summary log

Inherits Phase 3a-2 lenient pattern (filter_map + summary `tracing::warn!`):

| Failure                                               | Behavior                                                                                                               |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Mapped field_id deleted from Bitable                  | Skip row, count as malformed. Summary: "N rows had outdated field mappings. Re-open Lark Sync wizard."                 |
| Mapped field_name diverged (renamed)                  | Resolve by field_id (stable). Lazily update cached name on next schema fetch (via wizard or `detect_lark_schema` call) |
| Status option value not in mapping AND no fuzzy match | Use `default_column` from mapping                                                                                      |
| Bitable unreachable during hydrate                    | Inherit existing rate-limit + retry; after retries fail, banner "Bitable unreachable. Working from cache."             |

### Schema-completely-changed detection

Heuristic: if `list_tasks` returns 0 records AND `detect_lark_schema(...)` shows
fields don't match cached `field_id`s, surface toast: "Bitable schema may have
changed. Re-confirm mapping." Otherwise silent.

### Migration failures — soft fail

| Failure                | Behavior                                                                                                    |
| ---------------------- | ----------------------------------------------------------------------------------------------------------- |
| Old creds invalid      | Skip migration, log warn, old config untouched                                                              |
| Schema detection fails | Create binding with title-only mapping, toast: "Lark migration partial. Open Settings to complete mapping." |
| No selected repo       | Skip migration. Retry on next launch.                                                                       |

### Concurrency — file write safety

- `lark_repo_bindings.json` uses atomic write (existing
  `persistence::atomic_write`)
- `TaskProviderHandle = Arc<RwLock<HashMap<RepoId, Arc<dyn TaskProvider>>>>`:
  - Read locks during normal task ops (cheap, parallel)
  - Write lock only during binding add/delete/edit (rare)
  - Per-entry `Arc<dyn TaskProvider>` cloned out under read lock — IPC ops don't
    hold lock across await

### Auth boundary

- `app_secret` only in OS keyring (Phase 3a-1b hard rule maintained)
- `app_token` in binding file is fine (it's a Bitable resource ID, not a
  credential)
- Test mocks use `InMemorySecretStore` (existing trait)
- Schema detection responses never logged at WARN level (field names may carry
  user data — log `field_id` only at DEBUG)

## Testing strategy

### Rust unit tests

`persistence::lark_repo_bindings`:

- Round-trip save/load preserves all fields
- Atomic write (no mid-write corruption)
- Migration: old single-Bitable config → multi-repo bindings map
- Schema version present + validated on load
- Missing file returns empty map

`task_provider::lark::lark_field_resolver` (pure logic):

- `resolve_title`: explicit field → use; field missing → primary fallback; both
  missing → Err
- `resolve_status`: explicit option_id mapping → KanbanColumn; unknown option →
  fuzzy fallback; fuzzy fail → default_column
- `resolve_order`: order field present → value; absent → created_time DESC sort
- Field renamed: lookup by field_id works with stale name cache
- Field deleted: lookup returns None → caller skips row

`task_provider::lark::BitableSchemaDetector` (wiremock):

- Bitable with Task Status single-select → proposes title=primary, status=that
  field, status options auto-mapped
- Bitable without status-like field → status=None
- Bitable with multiple status-like fields → priority match (first by alphabetic
  name, deterministic)
- Empty Bitable → title=primary, all others None
- 403 from API → preserved Err

`task_provider::lark::LarkProvider`:

- `LarkProvider::new(client, app_token, table_id, mapping, status_values)`
  accepts mapping
- `list_tasks` uses mapping (not hardcoded names)
- `create_task` writes to mapped field names (Bitable API uses names for writes)
- `update_task`/`move_task` use reverse value mapping (KanbanColumn → option_id)
- Mapping with status=None: skips status write on move/create

`commands::lark_repo_binding`:

- `get_lark_repo_binding(repo_id)` returns binding or null
- `set_lark_repo_binding` validates (non-empty fields, title present), swaps
  provider_handle entry
- `delete_lark_repo_binding` swaps to LocalProvider, deletes file entry
- `detect_lark_schema` uses keyring app_secret + global app_id
- Multi-provider HashMap mutations write-locked correctly (no deadlock)

Migration logic in `lib.rs`:

- Old config with app_token+table_id + task_source=lark + selected_repo →
  binding created, old fields cleared
- Old config without selected_repo → migration deferred
- Already migrated (binding exists) → no-op
- Old config with invalid creds → migration skipped, old config preserved

### Rust integration tests (env-gated, real API)

`tests/lark_smoke.rs` additions:

- `smoke_detect_schema_proposes_mapping` — call against real tenant, assert
  response shape

### Frontend tests

`stores/lark-bindings.svelte.ts`:

- Add/update/delete propagates via SvelteMap
- Listens to `binding-updated` event
- Optimistic update + revert (mirrors tasks store pattern)

`LarkBindingWizard.svelte`:

- Step 1: Detect button disabled until both fields filled
- Step 1: API failure shows banner, stays on step
- Step 2: Title required validation
- Step 2: Auto-detected values pre-fill
- Step 2 → 3 transition only if status is single-select
- Step 3: All options shown with fuzzy auto-fill defaults
- Save IPC failure → toast + wizard stays open

`RepoSettingsDialog.svelte`:

- Renders "Not connected" state by default
- Renders connected state with current binding info
- Edit button opens wizard with existing values
- Disconnect button shows confirm dialog

### E2E test

`tests/e2e/phase-3a-3/` env-gated against real Lark tenant:

- Open repo settings → Connect → enter creds → detect → confirm mapping → save →
  board hydrates
- Edit mapping (swap status field) → verify column distribution changes
- Disconnect → board reverts to Local

### Coverage gates

- Rust: `cargo llvm-cov --lib` ≥ 95% on changed files (extend
  `--ignore-filename-regex` for `commands::lark_repo_binding` if mostly IPC
  wiring)
- Frontend: `bun run test:coverage` ≥ 95% branches global
- Per CLAUDE.md hard rule

### Test fixtures

`tests/fixtures/`:

- `bitable_fields_full.json` — primary + status single-select + text fields
- `bitable_fields_minimal.json` — only primary column
- `bitable_fields_status_text.json` — status as Text type
- `binding_v1.json` — saved binding for migration tests

## Decisions log (for future readers)

| Decision                   | Choice                                              | Reason                                                                                          |
| -------------------------- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Repo ↔ Bitable cardinality | 1:1                                                 | Simple model, matches kanban-board convention. Multi-binding can be revisited if proven needed. |
| Field identity             | field_id primary, field_name cached                 | Stable across Bitable renames; field-name still shown in UI                                     |
| Wizard UX                  | Auto-detect + confirm                               | Best UX vs blank manual mapping or JSON editor                                                  |
| Schema verify wizard       | Deprecated                                          | Field mapping replaces hardcoded-schema concept                                                 |
| Migration                  | Auto with non-modal banner + post-toast             | Smooth upgrade, user can review after                                                           |
| Implementation strategy    | Big-bang single phase                               | Mirrors Phase 3a-2 cadence, faster to ship                                                      |
| Storage                    | Separate `lark_repo_bindings.json` keyed by repo_id | Keeps repos.json generic; isolation per concern                                                 |
| Sort field requirement     | Optional with `created_time` DESC default           | YAGNI; most Bitables don't have explicit order                                                  |
| Per-repo task_source enum  | Derived from binding presence                       | Eliminates redundant state                                                                      |

## What's NOT covered

- Multi-tenant or multi-user — single-user assumption from Ansambel project
  rules
- Provider-generic config (Jira, Linear, etc) — config schema is Lark-specific;
  trait remains generic
- Lark Bitable view-level filtering (per-view config) — entire table is the unit
- Auto-create field for `repo_id` in Bitable — repo_id is binding context, not
  field
- Real-time bidirectional sync — focus-refresh debounced reads only

## Open questions (none blocking)

All design questions resolved during brainstorm. Concerns flagged but accepted
as YAGNI:

- "What if user wants Bitable A for repo X AND repo Y with different mappings?"
  → Each repo configures independently (some duplication; acceptable for
  single-user single-tenant model)
- "What if Bitable field type changes (single-select → text)?" → Detected at
  runtime, banner prompts re-confirm
- "What if Lark API adds a new field type we don't know about?" → Field appears
  in dropdown as `[name] (unknown)`, runtime uses fuzzy parser fallback

---

**Plan handoff:** After user reviews and approves this spec, transition to
`superpowers:writing-plans` to produce the bite-sized task plan in
`docs/superpowers/plans/2026-05-15-ansambel-phase-3a-3-per-repo-lark-binding.md`.
