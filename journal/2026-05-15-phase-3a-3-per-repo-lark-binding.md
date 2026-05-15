# Journal — 2026-05-15 — Phase 3a-3 per-repo Lark binding + field mapping wizard

## What shipped

20 plan tasks executed via subagent-driven-development. Each task: implementer
subagent → spec compliance review → code quality review → fix-up commit if
needed. 29 commits on `feat/phase-3a-3-per-repo-lark-binding` branch.

**Backend (Tasks 1-10):**

- `BitableBinding` / `FieldMapping` / `FieldRef` / `StatusValueMapping` /
  `ProposedMapping` types in `state.rs`.
- `persistence::lark_repo_bindings` module — atomic load/save/get/set/delete
  over `lark_repo_bindings.json`.
- `lark_field_resolver` — pure resolver functions (`resolve_title`,
  `resolve_description`, `resolve_status`, `resolve_order`) with layered
  fallbacks. Status resolution handles single-select object, plain text, missing
  field, unknown values.
- `BitableSchemaDetector::propose_mapping` — auto-detects title (primary) +
  status (keyword-match "status"/"stage"/"phase"/"kanban") + populates initial
  value mapping by running option names through `parse_kanban_column`.
- `LarkProvider` refactored: takes `FieldMapping` + `StatusValueMapping`.
  `from_binding()` convenience constructor. Lazy primary-field-name cache via
  `tokio::sync::OnceCell`. `list_tasks` uses `filter_map` + summary
  `tracing::warn!` to stay lenient on malformed rows.
- `TaskProviderHandle` reshaped from `Arc<RwLock<Arc<dyn TaskProvider>>>` to
  `Arc<RwLock<HashMap<RepoId, Arc<dyn TaskProvider>>>>`. `provider_for_repo`
  helper centralises lookup-with-default-local-fallback.
- `AppSettings.task_source` enum + `get_task_source`/`set_task_source` commands
  removed entirely. Lark mode is now derived per-repo from binding presence.
- 5 new Tauri commands: `get_lark_repo_binding`, `set_lark_repo_binding`,
  `delete_lark_repo_binding`, `list_lark_repo_bindings`, `detect_lark_schema`.
- `lark_auth.rs` shrunk: `LarkSettings` is now `{app_id, base_url}`;
  `LarkStatus` drops `app_token`/`table_id`; `verify_lark_schema` deleted.
- `task_provider/schema.rs` deleted entirely (replaced by resolver).
- Auto-migration in `lib.rs::setup()`: legacy `lark_settings.json` (3a-2 shape)
  → per-repo binding with `PENDING_RESOLVE` placeholder field IDs (wizard
  refreshes on first open). Idempotent. Emits `lark-migrated` event.

**Frontend (Tasks 11-18):**

- New TS types matching backend. IPC wrappers
  `api.lark.{getRepoBinding, setRepoBinding, deleteRepoBinding, listRepoBindings, detectSchema}`.
- `larkBindings` Svelte store (`SvelteMap<repo_id, BitableBinding>`) with
  optimistic upsert + revert-on-error.
- `LarkBindingWizard.svelte` — 3-step wizard. Step 1: app_token + table_id +
  Detect. Step 2: field mapping with auto-detected pre-fill. Step 3: status
  option mapping (when status field is single-select).
- `RepoSettingsDialog.svelte` — per-repo settings with embedded Lark Sync
  section. Connect / Edit mapping / Disconnect (with confirm).
- Sidebar repo right-click opens `RepoSettingsDialog`.
- `LarkSettings.svelte` renamed to `LarkGlobalSettings.svelte`, shrunk ~60%
  (only app_id/secret/base_url). Test Connection moved to wizard's Detect step.
- `SettingsDialog.svelte` lost the "Task source" radio entirely.
- `App.svelte` derives Lark mode per-repo from binding presence; listens for
  `lark-migrated` event with info toast.

**E2E (Task 19):**

- Env-gated Playwright smoke (`tests/e2e/phase-3a-3/`) covering wizard golden
  path. Skipped without `LARK_*` env vars.

## Decisions

- **Field-ID hybrid identity:** stable lookup by `field_id`, cached `field_name`
  for UI display. Survives Bitable field renames.
- **Auto-detect + confirm wizard, not opaque magic:** detector proposes, user
  reviews and saves. Always-show-wizard on connect.
- **1 repo → 1 Bitable:** keep architecture simple; multi-Bitable per repo
  deferred to a future phase.
- **Deprecate schema-verify wizard:** the new field-mapping wizard subsumes
  schema verification by construction (you map whatever exists rather than
  asserting required fields).
- **Big-bang single phase:** mirrored Phase 3a-2 cadence (~20 commits on a
  feature branch, merged via PR after gates pass) rather than splitting into
  3a-3a/3a-3b.
- **TaskProviderHandle as `HashMap<RepoId, Arc<dyn TaskProvider>>`:** local
  default fallback when no binding exists for a repo. Caller sees a unified
  provider interface regardless of mode.
- **Status resolution is layered:** option_id-in-entries → text-fuzzy → default.
  Tolerant of malformed/missing records. Per-record skips logged at `debug!`;
  aggregate counts at `warn!`.

## Surprises

- `KanbanColumn` didn't derive `Copy` — Task 1 needed `.clone()` throughout
  resolver call sites, not raw deref.
- `serde_json::Value` doesn't impl `Eq` → `ProposedMapping` could only derive
  `PartialEq` (Task 4 review caught spec typo).
- `parse_kanban_column` started as `pub(super)` — sibling module access required
  tightening to `pub(crate)` (Task 3 quality review).
- `i32` overflow in `resolve_order` for post-2038 timestamps — added
  `.clamp(i32::MIN as i64, i32::MAX as i64)` defensively (Task 3 quality
  review). The existing `lark.rs` had the same latent bug; left untouched.
- Task 5 couldn't delete `schema.rs` cleanly —
  `lark_auth.rs::verify_lark_schema` still referenced it. Deletion deferred to
  Task 9 where the command was also removed.
- The Sidebar doesn't have discrete per-repo rows (single selected repo shown at
  a time). Task 16's context menu attached to the sidebar header area.
- Several reviewer-flagged code-quality issues required follow-up commits:
  redundant double-clone, mid-file `use` declarations, doc-comment stutter, dead
  `tasks.retain` after the per-repo shape change. Caught by the spec→quality
  review cadence.
- Frontend coverage dipped to 93.19% after Tasks 13-15 added new
  components/store with limited test surface. Task 20 lifted to 95.06% by adding
  37 tests across 10 files — most concentrated on `RepoSettingsDialog` (ESC
  handlers, error paths) and `LarkBindingWizard` (Cancel, save-error toast,
  existing-binding pre-fill).

## Deferrals

- Multi-Bitable per repo (e.g., one Bitable per workspace) — design path open.
- Real-time bidirectional push — still focus-refresh based.
- Generic mapping abstraction reusable for Jira / Linear / Notion — Phase 3b+.
- Conflict resolution beyond last-write-wins.
- `BitableSchemaDetector` keyword list as a named const — kept inline; doc
  comment lists the four keywords.

## Gates at merge time

- `cargo test --lib`: 560 passed
- `cargo clippy --lib --all-targets -- -D warnings`: clean
- `cargo fmt --all -- --check`: clean
- `bun run check`: 0 errors (3 minor warnings about `existing` prop reference in
  wizard — intentional initial-value capture)
- `bun run lint`: clean
- `bun run test --run`: 702 passed across 48 files
- `bun run test:coverage`: 95.06% branches (above 95% threshold)
- E2E lists 1 spec, skips without env vars
