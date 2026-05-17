# Phase 3a-3.1 (v2) — Filter-Aware Lark Binding — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Spec:**
[`docs/superpowers/specs/2026-05-17-phase-3a-3-1-filter-aware-lark-binding-design.md`](../specs/2026-05-17-phase-3a-3-1-filter-aware-lark-binding-design.md)

**Goal:** Mount a Lark-style filter chip-bar above the kanban so users can
filter Bitable records by any column in real-time; backend routes to Lark's
`records/search` POST endpoint when filters are active.

**Architecture:** New `FilterSpec` field on `BitableBinding` (legacy default =
empty). New `bitable_search_records` on `LarkClient`. New `list_lark_fields`
Tauri command. `LarkProvider::list_tasks` chooses list vs search endpoint based
on `filters.is_empty()`. Frontend store debounces filter edits (300 ms),
persists to binding, triggers `refresh_tasks`.

**Tech Stack:** Rust + Tauri 2 + Svelte 5 runes + Bun + reqwest + wiremock +
Vitest + Playwright.

**Branch:** `feat/phase-3a-3-1-filter-aware-lark-binding` (off main `b6919bc`,
contains only the spec commit `28ec562`).

**Coverage gate:** 95 % line+branch+function on changed files.
`commands/lark_repo_binding.rs` already in `.github/workflows/ci.yml`
ignore-regex.

---

## File map

### Created

- `src-tauri/src/state.rs` — new types added inline (no new file).
- `src-tauri/src/task_provider/lark.rs` — `refresh_field_names` helper added
  inline.
- `src/lib/stores/lark-binding-filters.svelte.ts` — new filter store.
- `src/lib/components/kanban/FilterBar.svelte` — new filter chip-bar component.
- `src/lib/components/kanban/FilterBar.test.ts` — Vitest component tests.
- `src/lib/stores/lark-binding-filters.svelte.test.ts` — store tests.
- `tests/e2e/phase-3a-3-1/phase-3a-3-1-filter-bar.spec.ts` — Playwright E2E.

### Modified

- `src-tauri/src/state.rs` — add `FilterCondition`, `FilterOperator`,
  `FilterConjunction`, `FilterSpec`; add `filters` field to `BitableBinding`.
- `src-tauri/src/platform/lark_client.rs` — add `bitable_search_records`.
- `src-tauri/src/task_provider/lark.rs` — add `filters` field +
  `field_name_by_id` OnceCell to `LarkProvider`; route `list_tasks`; refresh
  field names before search; update constructor.
- `src-tauri/src/persistence/lark_repo_bindings.rs` — bump
  `default_schema_version` 1→3; add `migrate_v1_to_v3`.
- `src-tauri/src/commands/lark_repo_binding.rs` — add `list_lark_fields_inner` +
  `#[tauri::command] list_lark_fields`; update `LarkProvider` construction call
  sites to pass `filters`.
- `src-tauri/src/lib.rs` — call `migrate_v1_to_v3` in `setup()`; register
  `list_lark_fields` in `invoke_handler!`; update spawn-init blocks that build
  `LarkProvider`.
- `src/lib/types.ts` — add `FilterOperator`, `FilterConjunction`,
  `FilterCondition`, `FilterSpec`; add `filters` to `BitableBinding`.
- `src/lib/ipc.ts` — add `api.lark.listFields(appToken, tableId)`.
- `src/lib/stores/lark-bindings.svelte.ts` — ensure new `filters` field is
  preserved through `setBinding` round-trips (no logic change if it uses
  spread).
- `src/lib/components/kanban/KanbanBoard.svelte` — mount `<FilterBar>` above the
  column row.

---

## Task ordering

Backend → wiring → frontend → integration → E2E. Each task ends with a green
test + commit. Conventional commit prefixes: `feat`, `test`, `refactor`, `docs`,
`chore`.

---

### Task 1: `FilterOperator` + `FilterConjunction` enums

**Files:**

- Modify: `src-tauri/src/state.rs` (append near `BitableBinding` cluster,
  ~line 80)

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/state.rs` inside the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn filter_operator_serializes_as_lark_camel_case() {
    use serde_json::json;
    assert_eq!(serde_json::to_value(FilterOperator::Is).unwrap(), json!("is"));
    assert_eq!(serde_json::to_value(FilterOperator::IsNot).unwrap(), json!("isNot"));
    assert_eq!(serde_json::to_value(FilterOperator::Contains).unwrap(), json!("contains"));
    assert_eq!(
        serde_json::to_value(FilterOperator::DoesNotContain).unwrap(),
        json!("doesNotContain")
    );
    assert_eq!(serde_json::to_value(FilterOperator::IsEmpty).unwrap(), json!("isEmpty"));
    assert_eq!(serde_json::to_value(FilterOperator::IsNotEmpty).unwrap(), json!("isNotEmpty"));
    assert_eq!(serde_json::to_value(FilterOperator::IsGreater).unwrap(), json!("isGreater"));
    assert_eq!(
        serde_json::to_value(FilterOperator::IsGreaterEqual).unwrap(),
        json!("isGreaterEqual")
    );
    assert_eq!(serde_json::to_value(FilterOperator::IsLess).unwrap(), json!("isLess"));
    assert_eq!(
        serde_json::to_value(FilterOperator::IsLessEqual).unwrap(),
        json!("isLessEqual")
    );
}

#[test]
fn filter_conjunction_default_is_and_lowercase() {
    use serde_json::json;
    assert_eq!(FilterConjunction::default(), FilterConjunction::And);
    assert_eq!(serde_json::to_value(FilterConjunction::And).unwrap(), json!("and"));
    assert_eq!(serde_json::to_value(FilterConjunction::Or).unwrap(), json!("or"));
}
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cd src-tauri && cargo test --lib state::tests::filter_ -- --nocapture`
Expected: FAIL with `cannot find type 'FilterOperator' in this scope`.

- [ ] **Step 3: Add the enums**

Add to `src-tauri/src/state.rs` immediately above `pub struct BitableBinding`
(current line 71):

```rust
/// Operator for a Bitable filter condition. Serializes to Lark's
/// `records/search` operator string (camelCase).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Is,
    IsNot,
    Contains,
    DoesNotContain,
    IsEmpty,
    IsNotEmpty,
    IsGreater,
    IsGreaterEqual,
    IsLess,
    IsLessEqual,
}

/// Conjunction joining multiple filter conditions (AND / OR).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterConjunction {
    And,
    Or,
}

impl Default for FilterConjunction {
    fn default() -> Self {
        Self::And
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd src-tauri && cargo test --lib state::tests::filter_ -- --nocapture`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat(phase-3a-3-1): add FilterOperator and FilterConjunction enums"
```

---

### Task 2: `FilterCondition` + `FilterSpec` structs

**Files:**

- Modify: `src-tauri/src/state.rs` (append below enums from Task 1)

- [ ] **Step 1: Write failing tests**

Add to the `tests` mod in `src-tauri/src/state.rs`:

```rust
#[test]
fn filter_spec_default_is_and_with_empty_conditions() {
    let spec = FilterSpec::default();
    assert_eq!(spec.conjunction, FilterConjunction::And);
    assert!(spec.conditions.is_empty());
    assert!(spec.is_empty());
}

#[test]
fn filter_spec_is_empty_false_when_has_condition() {
    let spec = FilterSpec {
        conjunction: FilterConjunction::And,
        conditions: vec![FilterCondition {
            field_id: "fld123".into(),
            field_name: "Status".into(),
            operator: FilterOperator::Is,
            value: vec!["Done".into()],
        }],
    };
    assert!(!spec.is_empty());
}

#[test]
fn filter_spec_roundtrips_through_json() {
    let spec = FilterSpec {
        conjunction: FilterConjunction::Or,
        conditions: vec![
            FilterCondition {
                field_id: "fld1".into(),
                field_name: "Sprint".into(),
                operator: FilterOperator::Is,
                value: vec!["S1".into()],
            },
            FilterCondition {
                field_id: "fld2".into(),
                field_name: "Owner".into(),
                operator: FilterOperator::IsEmpty,
                value: vec![],
            },
        ],
    };
    let json = serde_json::to_string(&spec).unwrap();
    let back: FilterSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(spec, back);
}
```

- [ ] **Step 2: Run tests to verify fail**

Run:
`cd src-tauri && cargo test --lib state::tests::filter_spec_ -- --nocapture`
Expected: FAIL — `FilterSpec`, `FilterCondition` unknown.

- [ ] **Step 3: Add the structs**

Add to `src-tauri/src/state.rs` immediately below the enums from Task 1:

```rust
/// One filter condition matching Lark `records/search` schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FilterCondition {
    /// Bitable field id (stable lookup key — survives renames).
    pub field_id: String,
    /// Cached field name (UI display + outgoing API body). Refreshed from
    /// the LarkProvider field cache before each send.
    pub field_name: String,
    pub operator: FilterOperator,
    /// Per-type value (string for text, option name(s) for select,
    /// ISO-8601 for date, number-as-string, email/display for person).
    /// Empty Vec for unary operators (`isEmpty` / `isNotEmpty`).
    pub value: Vec<String>,
}

/// Set of filter conditions joined by a single top-level conjunction.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FilterSpec {
    #[serde(default)]
    pub conjunction: FilterConjunction,
    #[serde(default)]
    pub conditions: Vec<FilterCondition>,
}

impl FilterSpec {
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run:
`cd src-tauri && cargo test --lib state::tests::filter_spec_ -- --nocapture`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat(phase-3a-3-1): add FilterCondition and FilterSpec structs"
```

---

### Task 3: Add `filters` to `BitableBinding` (backward-compat)

**Files:**

- Modify: `src-tauri/src/state.rs:71-79`

- [ ] **Step 1: Write failing test**

Add to `tests` mod in `src-tauri/src/state.rs`:

```rust
#[test]
fn legacy_binding_without_filters_loads_as_default_empty() {
    let legacy_json = r#"{
        "app_token": "appXYZ",
        "table_id": "tblABC",
        "field_mapping": {
            "title": { "field_id": "fld1", "field_name": "Title" }
        },
        "status_value_mapping": { "entries": {}, "default_column": "Todo" },
        "created_at": 1700000000,
        "updated_at": 1700000000
    }"#;
    let binding: BitableBinding = serde_json::from_str(legacy_json).unwrap();
    assert!(binding.filters.is_empty());
    assert_eq!(binding.filters.conjunction, FilterConjunction::And);
}

#[test]
fn binding_with_filters_roundtrips() {
    let binding = BitableBinding {
        app_token: "appXYZ".into(),
        table_id: "tblABC".into(),
        filters: FilterSpec {
            conjunction: FilterConjunction::And,
            conditions: vec![FilterCondition {
                field_id: "fld1".into(),
                field_name: "Status".into(),
                operator: FilterOperator::Is,
                value: vec!["Done".into()],
            }],
        },
        field_mapping: FieldMapping {
            title: FieldRef { field_id: "fld1".into(), field_name: "Title".into() },
            description: None,
            status: None,
            order: None,
        },
        status_value_mapping: StatusValueMapping::default(),
        created_at: 1700000000,
        updated_at: 1700000000,
    };
    let json = serde_json::to_string(&binding).unwrap();
    let back: BitableBinding = serde_json::from_str(&json).unwrap();
    assert_eq!(binding, back);
}
```

- [ ] **Step 2: Run test to verify fail**

Run:
`cd src-tauri && cargo test --lib state::tests::legacy_binding_without_filters -- --nocapture`
Expected: FAIL — `BitableBinding` has no field `filters` or extra-field error.

- [ ] **Step 3: Add `filters` field**

Edit `src-tauri/src/state.rs:71-79` (current struct body). Insert one new field
between `table_id` and `field_mapping`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    /// Optional filter applied at the Lark server side via the
    /// `records/search` endpoint. Empty (default) → fetch all records
    /// via the existing list endpoint.
    #[serde(default)]
    pub filters: FilterSpec,
    pub field_mapping: FieldMapping,
    #[serde(default)]
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}
```

Ensure `#[derive(PartialEq, Eq)]` is present (needed for the round-trip
assertion). If existing derive list omits them, add them.

- [ ] **Step 4: Run all state tests**

Run: `cd src-tauri && cargo test --lib state::tests -- --nocapture` Expected:
PASS — pre-existing tests still pass, 2 new tests pass.

- [ ] **Step 5: Build check**

Run: `cd src-tauri && cargo check --all-targets` Expected: PASS. If anything
breaks where a `BitableBinding` is constructed inline in production code, add
`filters: FilterSpec::default(),` to that literal. Common construction sites:
`commands/lark_repo_binding.rs::set_lark_repo_binding_inner`,
`lib.rs::maybe_migrate_to_per_repo_binding`, integration tests in `lib.rs`. Fix
each compile error.

- [ ] **Step 6: Re-run full lib tests**

Run: `cd src-tauri && cargo test --lib -- --nocapture` Expected: PASS — entire
suite green.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/lark_repo_binding.rs src-tauri/src/lib.rs
git commit -m "feat(phase-3a-3-1): add filters field to BitableBinding with serde default"
```

(Stage only files actually modified by the compile fix; adjust the `git add`
accordingly.)

---

### Task 4: Schema migration v1 → v3

**Files:**

- Modify: `src-tauri/src/persistence/lark_repo_bindings.rs:17-44`

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/persistence/lark_repo_bindings.rs` inside the existing
`#[cfg(test)] mod tests` block (or create one if absent):

```rust
#[test]
fn default_schema_version_is_3() {
    assert_eq!(default_schema_version(), 3);
}

#[test]
fn migrate_v1_to_v3_no_op_when_file_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let result = migrate_v1_to_v3(tmp.path()).expect("should succeed");
    assert_eq!(result, 3);
    assert!(!lark_repo_bindings_file(tmp.path()).exists());
}

#[test]
fn migrate_v1_to_v3_bumps_version_and_preserves_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    let path = lark_repo_bindings_file(tmp.path());
    let legacy = r#"{"schema_version":1,"bindings":{"repo-1":{
        "app_token":"appA","table_id":"tblA",
        "field_mapping":{"title":{"field_id":"fld1","field_name":"Title"}},
        "status_value_mapping":{"entries":{},"default_column":"Todo"},
        "created_at":1700000000,"updated_at":1700000000
    }}}"#;
    std::fs::write(&path, legacy).unwrap();

    let version = migrate_v1_to_v3(tmp.path()).expect("should succeed");
    assert_eq!(version, 3);

    let reloaded = load_bindings(tmp.path()).unwrap();
    assert_eq!(reloaded.schema_version, 3);
    assert_eq!(reloaded.bindings.len(), 1);
    let binding = reloaded.bindings.get("repo-1").expect("binding present");
    assert!(binding.filters.is_empty());
}

#[test]
fn migrate_v1_to_v3_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let path = lark_repo_bindings_file(tmp.path());
    let v3 = r#"{"schema_version":3,"bindings":{}}"#;
    std::fs::write(&path, v3).unwrap();

    let v1 = migrate_v1_to_v3(tmp.path()).expect("first call");
    let v2 = migrate_v1_to_v3(tmp.path()).expect("second call");
    assert_eq!(v1, 3);
    assert_eq!(v2, 3);
}
```

- [ ] **Step 2: Run tests to verify fail**

Run:
`cd src-tauri && cargo test --lib persistence::lark_repo_bindings::tests -- --nocapture`
Expected: FAIL — function `migrate_v1_to_v3` not found, `default_schema_version`
returns 1 not 3.

- [ ] **Step 3: Bump default and add migrator**

Edit `src-tauri/src/persistence/lark_repo_bindings.rs`. Find the
`default_schema_version` function (current line 24-26):

```rust
fn default_schema_version() -> u32 {
    3
}
```

Add (anywhere below `save_bindings`) the migrator and re-export it:

```rust
/// Migrate the on-disk bindings file from schema v1 to v3.
/// Skips v2 entirely (the view-aware PR #27 schema never shipped).
/// Idempotent: no-op when already at v3 or higher; no-op when file is absent.
pub fn migrate_v1_to_v3(data_dir: &Path) -> Result<u32, String> {
    let path = lark_repo_bindings_file(data_dir);
    if !path.exists() {
        return Ok(3);
    }
    let mut file = load_bindings(data_dir).map_err(|e| e.to_string())?;
    if file.schema_version >= 3 {
        return Ok(file.schema_version);
    }
    file.schema_version = 3;
    save_bindings(data_dir, &file).map_err(|e| e.to_string())?;
    Ok(3)
}
```

If `tempfile` isn't already a dev-dep, add to `src-tauri/Cargo.toml` under
`[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 4: Run tests to verify pass**

Run:
`cd src-tauri && cargo test --lib persistence::lark_repo_bindings::tests -- --nocapture`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/persistence/lark_repo_bindings.rs src-tauri/Cargo.toml
git commit -m "feat(phase-3a-3-1): bump schema to v3 with migrate_v1_to_v3"
```

---

### Task 5: Call migration in `lib.rs::setup()`

**Files:**

- Modify: `src-tauri/src/lib.rs:79-211` (around the existing
  `maybe_migrate_to_per_repo_binding` call)

- [ ] **Step 1: Write failing integration test**

Add to the migration-tests block in `src-tauri/src/lib.rs` (near the existing
migration tests at lines 407-763):

```rust
#[test]
fn setup_bumps_schema_to_v3_on_legacy_v1_file() {
    use crate::persistence::lark_repo_bindings::{
        lark_repo_bindings_file, load_bindings, migrate_v1_to_v3,
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = lark_repo_bindings_file(tmp.path());
    std::fs::write(
        &path,
        r#"{"schema_version":1,"bindings":{}}"#,
    )
    .unwrap();

    let v = migrate_v1_to_v3(tmp.path()).unwrap();
    assert_eq!(v, 3);
    assert_eq!(load_bindings(tmp.path()).unwrap().schema_version, 3);
}
```

(This test exercises the migrator directly. The actual call-site in `setup()` is
wired in Step 3 and is not unit-testable without a full Tauri context — coverage
comes from E2E + the unit test above.)

- [ ] **Step 2: Run test to verify pass**

Run: `cd src-tauri && cargo test --lib setup_bumps_schema_to_v3 -- --nocapture`
Expected: PASS (the test only exercises the already-implemented
`migrate_v1_to_v3`).

- [ ] **Step 3: Wire migration into `setup()`**

Find the existing call to `maybe_migrate_to_per_repo_binding` inside `setup()`
in `src-tauri/src/lib.rs` (around line 90-100). Immediately after the data_dir
resolves and BEFORE bindings are loaded into the provider handle, add:

```rust
// Bump lark binding schema v1 → v3 (no v2 ever shipped).
if let Err(e) = persistence::lark_repo_bindings::migrate_v1_to_v3(&data_dir) {
    tracing::warn!("lark binding schema migration failed: {e}");
}
```

Ensure `tracing` import is in scope (it is — file uses `tracing` elsewhere).

- [ ] **Step 4: Build + run all lib tests**

Run:
`cd src-tauri && cargo check --all-targets && cargo test --lib -- --nocapture`
Expected: PASS — entire suite green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(phase-3a-3-1): run v1→v3 binding migration during setup"
```

---

### Task 6: `bitable_search_records` on `LarkClient`

**Files:**

- Modify: `src-tauri/src/platform/lark_client.rs` (add new method near
  `bitable_list_records` ~line 475-540)

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/platform/lark_client.rs` inside its
`#[cfg(test)] mod tests` block:

```rust
#[tokio::test]
async fn bitable_search_records_posts_filter_body_with_and_conjunction() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use wiremock::matchers::{body_json, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;

    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .and(header_exists("Authorization"))
        .and(body_json(serde_json::json!({
            "filter": {
                "conjunction": "and",
                "conditions": [
                    { "field_name": "Status", "operator": "is", "value": ["Done"] }
                ]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "msg": "success",
            "data": {
                "items": [
                    { "record_id": "rec1", "fields": { "Title": "T1" } }
                ],
                "has_more": false,
                "page_token": null,
                "total": 1
            }
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = LarkClient::new(LarkConfig {
        app_id: "id".into(),
        app_secret: "sec".into(),
        app_token: "appA".into(),
        table_id: "tblA".into(),
        base_url: mock.uri(),
    })
    .unwrap();

    let spec = FilterSpec {
        conjunction: FilterConjunction::And,
        conditions: vec![FilterCondition {
            field_id: "fld1".into(),
            field_name: "Status".into(),
            operator: FilterOperator::Is,
            value: vec!["Done".into()],
        }],
    };
    let records = client
        .bitable_search_records("appA", "tblA", &spec)
        .await
        .expect("search ok");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].record_id, "rec1");
}

#[tokio::test]
async fn bitable_search_records_posts_or_conjunction() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .and(body_partial_json(serde_json::json!({
            "filter": { "conjunction": "or" }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [], "has_more": false, "page_token": null, "total": 0 }
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = LarkClient::new(LarkConfig {
        app_id: "id".into(), app_secret: "sec".into(),
        app_token: "appA".into(), table_id: "tblA".into(),
        base_url: mock.uri(),
    }).unwrap();

    let spec = FilterSpec {
        conjunction: FilterConjunction::Or,
        conditions: vec![FilterCondition {
            field_id: "fld1".into(), field_name: "A".into(),
            operator: FilterOperator::IsEmpty, value: vec![],
        }],
    };
    let r = client.bitable_search_records("appA", "tblA", &spec).await.unwrap();
    assert!(r.is_empty());
}

#[tokio::test]
async fn bitable_search_records_paginates() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;

    // Page 1: has_more=true, returns page_token "tok2"
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .and(query_param("page_size", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": {
                "items": [{ "record_id": "r1", "fields": {} }],
                "has_more": true, "page_token": "tok2", "total": 2
            }
        })))
        .up_to_n_times(1)
        .mount(&mock)
        .await;

    // Page 2: page_token=tok2, returns last item
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .and(query_param("page_token", "tok2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": {
                "items": [{ "record_id": "r2", "fields": {} }],
                "has_more": false, "page_token": null, "total": 2
            }
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = LarkClient::new(LarkConfig {
        app_id: "id".into(), app_secret: "sec".into(),
        app_token: "appA".into(), table_id: "tblA".into(),
        base_url: mock.uri(),
    }).unwrap();

    let spec = FilterSpec {
        conjunction: FilterConjunction::And,
        conditions: vec![FilterCondition {
            field_id: "fld1".into(), field_name: "A".into(),
            operator: FilterOperator::IsNotEmpty, value: vec![],
        }],
    };
    let r = client.bitable_search_records("appA", "tblA", &spec).await.unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].record_id, "r1");
    assert_eq!(r[1].record_id, "r2");
}

#[tokio::test]
async fn bitable_search_records_surfaces_non_zero_code_as_error() {
    use crate::state::{FilterConjunction, FilterSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 1254100, "msg": "invalid filter field_name", "data": {}
        })))
        .mount(&mock)
        .await;

    let client = LarkClient::new(LarkConfig {
        app_id: "id".into(), app_secret: "sec".into(),
        app_token: "appA".into(), table_id: "tblA".into(),
        base_url: mock.uri(),
    }).unwrap();

    let spec = FilterSpec { conjunction: FilterConjunction::And, conditions: vec![] };
    let err = client.bitable_search_records("appA", "tblA", &spec).await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("1254100"), "expected code in error, got: {msg}");
}
```

- [ ] **Step 2: Run tests to verify fail**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_search_records_ -- --nocapture`
Expected: FAIL — method `bitable_search_records` not found.

- [ ] **Step 3: Implement `bitable_search_records`**

Add to `src-tauri/src/platform/lark_client.rs` immediately below
`bitable_list_records`:

```rust
/// Search Bitable records using the POST `records/search` endpoint with a
/// typed filter body. Paginated identically to `bitable_list_records`.
pub async fn bitable_search_records(
    &self,
    app_token: &str,
    table_id: &str,
    filter: &crate::state::FilterSpec,
) -> Result<Vec<BitableRecord>> {
    let mut out: Vec<BitableRecord> = Vec::new();
    let mut page_token: Option<String> = None;

    for _ in 0..100 {
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/search",
            self.config.base_url, app_token, table_id
        );
        let token = self.tenant_access_token().await?;
        let body = serde_json::json!({ "filter": filter });
        let mut req = self
            .http
            .post(&url)
            .bearer_auth(token)
            .query(&[("page_size", "100")])
            .json(&body);
        if let Some(t) = &page_token {
            req = req.query(&[("page_token", t.as_str())]);
        }

        #[derive(Deserialize)]
        struct Envelope {
            code: i64,
            msg: String,
            data: Option<PageData>,
        }
        #[derive(Deserialize)]
        struct PageData {
            items: Option<Vec<BitableRecord>>,
            has_more: Option<bool>,
            page_token: Option<String>,
        }

        let resp = self.send_with_retry(req).await?;
        let env: Envelope = resp.json().await.map_err(|e| AppError::Lark(e.to_string()))?;
        if env.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_search_records code {} msg {}",
                env.code, env.msg
            )));
        }
        let data = env.data.ok_or_else(|| AppError::Lark("missing data".into()))?;
        out.extend(data.items.unwrap_or_default());
        if !data.has_more.unwrap_or(false) {
            return Ok(out);
        }
        page_token = data.page_token;
        if page_token.is_none() {
            return Ok(out);
        }
    }
    Err(AppError::Lark("bitable_search_records exceeded 100 pages".into()))
}
```

Adjust the local `Envelope`/`PageData` types if `bitable_list_records` already
defines reusable envelope helpers; reuse them and remove the duplicated structs.

- [ ] **Step 4: Run tests to verify pass**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_search_records_ -- --nocapture`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/lark_client.rs
git commit -m "feat(phase-3a-3-1): add bitable_search_records to LarkClient"
```

---

### Task 7: `LarkProvider` carries `filters` + field-name cache

**Files:**

- Modify: `src-tauri/src/task_provider/lark.rs:21-59` (LarkProvider struct +
  constructor)

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/task_provider/lark.rs` in its `#[cfg(test)] mod tests`
block:

```rust
#[tokio::test]
async fn lark_provider_caches_field_name_by_id() {
    use crate::state::{FilterSpec, FilterConjunction};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [
                { "field_id": "fld1", "field_name": "Renamed Status", "type": 3,
                  "is_primary": false, "property": null }
            ], "has_more": false, "page_token": null }
        })))
        .expect(1)  // proves OnceCell only calls once
        .mount(&mock)
        .await;

    let client = std::sync::Arc::new(
        LarkClient::new(LarkConfig {
            app_id: "id".into(), app_secret: "sec".into(),
            app_token: "appA".into(), table_id: "tblA".into(),
            base_url: mock.uri(),
        }).unwrap()
    );
    let provider = LarkProvider::new(
        client,
        "appA".into(), "tblA".into(),
        FilterSpec { conjunction: FilterConjunction::And, conditions: vec![] },
        FieldMapping {
            title: FieldRef { field_id: "fld1".into(), field_name: "Title".into() },
            description: None, status: None, order: None,
        },
        StatusValueMapping::default(),
    );

    let cache1 = provider.field_name_by_id().await.unwrap().clone();
    let cache2 = provider.field_name_by_id().await.unwrap().clone();
    assert_eq!(cache1.get("fld1").map(String::as_str), Some("Renamed Status"));
    assert_eq!(cache1, cache2);
}

#[test]
fn refresh_field_names_overwrites_from_canonical() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use std::collections::HashMap;

    let spec = FilterSpec {
        conjunction: FilterConjunction::And,
        conditions: vec![FilterCondition {
            field_id: "fld1".into(),
            field_name: "Old Name".into(),
            operator: FilterOperator::Is,
            value: vec!["x".into()],
        }],
    };
    let mut canonical = HashMap::new();
    canonical.insert("fld1".to_string(), "New Name".to_string());

    let refreshed = refresh_field_names(&spec, &canonical);
    assert_eq!(refreshed.conditions[0].field_name, "New Name");
    assert_eq!(refreshed.conditions[0].field_id, "fld1");
    assert_eq!(refreshed.conditions[0].value, vec!["x".to_string()]);
}

#[test]
fn refresh_field_names_keeps_old_name_when_id_missing_from_cache() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use std::collections::HashMap;

    let spec = FilterSpec {
        conjunction: FilterConjunction::And,
        conditions: vec![FilterCondition {
            field_id: "fld_deleted".into(),
            field_name: "Was Status".into(),
            operator: FilterOperator::Is,
            value: vec!["x".into()],
        }],
    };
    let canonical: HashMap<String, String> = HashMap::new();
    let refreshed = refresh_field_names(&spec, &canonical);
    // Deleted field: fall back to the persisted name; server will error,
    // surface to user via the chip "broken filter" UI.
    assert_eq!(refreshed.conditions[0].field_name, "Was Status");
}
```

- [ ] **Step 2: Run tests to verify fail**

Run:
`cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_caches_field_name -- --nocapture`
Expected: FAIL — `field_name_by_id` method + `refresh_field_names` function
unknown; `LarkProvider::new` signature wrong.

- [ ] **Step 3: Update `LarkProvider`**

Edit `src-tauri/src/task_provider/lark.rs:21-59`. Add new field + new cache,
update constructor:

```rust
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
    filters: FilterSpec,                              // NEW
    field_mapping: FieldMapping,
    status_value_mapping: StatusValueMapping,
    primary_field_name: OnceCell<String>,
    status_options: OnceCell<Vec<BitableOption>>,
    field_name_by_id: OnceCell<HashMap<String, String>>, // NEW
}

impl LarkProvider {
    pub fn new(
        client: Arc<LarkClient>,
        app_token: String,
        table_id: String,
        filters: FilterSpec,                         // NEW arg
        field_mapping: FieldMapping,
        status_value_mapping: StatusValueMapping,
    ) -> Self {
        Self {
            client,
            app_token,
            table_id,
            filters,
            field_mapping,
            status_value_mapping,
            primary_field_name: OnceCell::new(),
            status_options: OnceCell::new(),
            field_name_by_id: OnceCell::new(),
        }
    }

    /// Lazily fetch + cache `{field_id → canonical field_name}` from the
    /// bound table's schema. Rebuilt only on a fresh LarkProvider instance
    /// (i.e. after the binding is saved from the wizard or FilterBar).
    pub(crate) async fn field_name_by_id(&self) -> Result<&HashMap<String, String>> {
        self.field_name_by_id
            .get_or_try_init(|| async {
                let fields = self
                    .client
                    .bitable_list_fields(&self.app_token, &self.table_id)
                    .await?;
                Ok::<_, AppError>(
                    fields
                        .into_iter()
                        .map(|f| (f.field_id, f.field_name))
                        .collect::<HashMap<_, _>>(),
                )
            })
            .await
    }
}
```

Add a module-level helper near the top of the same file:

```rust
/// Rebuild a FilterSpec with each condition's `field_name` overwritten
/// from a canonical `field_id → field_name` map. Missing ids fall through
/// with the persisted name (server will surface an error chip-side).
pub(crate) fn refresh_field_names(
    spec: &FilterSpec,
    canonical: &HashMap<String, String>,
) -> FilterSpec {
    let conditions = spec
        .conditions
        .iter()
        .map(|c| {
            let canonical_name = canonical.get(&c.field_id).cloned().unwrap_or_else(|| c.field_name.clone());
            FilterCondition {
                field_id: c.field_id.clone(),
                field_name: canonical_name,
                operator: c.operator.clone(),
                value: c.value.clone(),
            }
        })
        .collect();
    FilterSpec {
        conjunction: spec.conjunction.clone(),
        conditions,
    }
}
```

Make sure imports include `FilterSpec`, `FilterCondition` from `crate::state`,
and `HashMap` from `std::collections`.

- [ ] **Step 4: Fix all `LarkProvider::new` call sites**

Build will fail at every existing call to `LarkProvider::new`. Grep:

```bash
cd src-tauri && rg 'LarkProvider::new' src/
```

Each call site needs `binding.filters.clone()` (or `FilterSpec::default()` for
tests that don't care). Common sites:

- `src/commands/lark_repo_binding.rs::set_lark_repo_binding_inner`
- `src/lib.rs::setup()` provider-init block (lines ~118-151)
- `src/lib.rs` migration tests (lines ~660-720)
- `src/task_provider/lark.rs` internal tests

For each, add `binding.filters.clone(),` after `table_id` arg (or
`FilterSpec::default(),` in tests).

- [ ] **Step 5: Run tests to verify pass**

Run:
`cd src-tauri && cargo check --all-targets && cargo test --lib task_provider::lark::tests -- --nocapture`
Expected: PASS — including 3 new tests.

- [ ] **Step 6: Full lib test sanity**

Run: `cd src-tauri && cargo test --lib -- --nocapture` Expected: PASS — entire
suite green (some call-site adjustments may have surfaced).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/task_provider/lark.rs src-tauri/src/commands/lark_repo_binding.rs src-tauri/src/lib.rs
git commit -m "feat(phase-3a-3-1): LarkProvider holds filters + field_name_by_id cache"
```

---

### Task 8: Route `list_tasks` to list vs search

**Files:**

- Modify: `src-tauri/src/task_provider/lark.rs:278-327` (`list_tasks`)

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/task_provider/lark.rs` `tests` mod:

```rust
#[tokio::test]
async fn lark_provider_uses_list_endpoint_when_filters_empty() {
    use crate::state::{FilterConjunction, FilterSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    // List endpoint (GET) must be called.
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [], "has_more": false, "page_token": null, "total": 0 }
        })))
        .expect(1)
        .mount(&mock).await;
    // Search endpoint must NOT be called.
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock).await;

    let provider = build_provider_with_filter(
        &mock,
        FilterSpec { conjunction: FilterConjunction::And, conditions: vec![] },
    );
    let _ = provider.list_tasks("repo-1").await.unwrap();
}

#[tokio::test]
async fn lark_provider_uses_search_endpoint_when_filters_non_empty() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    // Schema fetch (for field-name cache).
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [
                { "field_id": "fld1", "field_name": "Status", "type": 3,
                  "is_primary": false, "property": null }
            ], "has_more": false, "page_token": null }
        })))
        .mount(&mock).await;
    // List endpoint must NOT be called.
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock).await;
    // Search endpoint MUST be called.
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [], "has_more": false, "page_token": null, "total": 0 }
        })))
        .expect(1)
        .mount(&mock).await;

    let provider = build_provider_with_filter(
        &mock,
        FilterSpec {
            conjunction: FilterConjunction::And,
            conditions: vec![FilterCondition {
                field_id: "fld1".into(),
                field_name: "Old Name".into(),
                operator: FilterOperator::Is,
                value: vec!["Done".into()],
            }],
        },
    );
    let _ = provider.list_tasks("repo-1").await.unwrap();
}
```

Add the test helper at the bottom of the same `tests` mod:

```rust
fn build_provider_with_filter(
    mock: &wiremock::MockServer,
    filters: crate::state::FilterSpec,
) -> LarkProvider {
    let client = std::sync::Arc::new(
        LarkClient::new(LarkConfig {
            app_id: "id".into(), app_secret: "sec".into(),
            app_token: "appA".into(), table_id: "tblA".into(),
            base_url: mock.uri(),
        }).unwrap()
    );
    LarkProvider::new(
        client, "appA".into(), "tblA".into(),
        filters,
        FieldMapping {
            title: FieldRef { field_id: "fld1".into(), field_name: "Title".into() },
            description: None, status: None, order: None,
        },
        StatusValueMapping::default(),
    )
}
```

- [ ] **Step 2: Run tests to verify fail**

Run:
`cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_uses_ -- --nocapture`
Expected: FAIL — list_tasks still uses list endpoint unconditionally.

- [ ] **Step 3: Route in `list_tasks`**

Edit `src-tauri/src/task_provider/lark.rs:278-327`. Find where
`bitable_list_records` is called inside `list_tasks` and replace with a branch:

```rust
let records: Vec<BitableRecord> = if self.filters.is_empty() {
    self.client
        .bitable_list_records(&self.app_token, &self.table_id)
        .await?
} else {
    let canonical = self.field_name_by_id().await?;
    let refreshed = refresh_field_names(&self.filters, canonical);
    self.client
        .bitable_search_records(&self.app_token, &self.table_id, &refreshed)
        .await?
};
```

- [ ] **Step 4: Run tests to verify pass**

Run:
`cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_uses_ -- --nocapture`
Expected: PASS — 2 tests.

- [ ] **Step 5: Run full provider tests**

Run: `cd src-tauri && cargo test --lib task_provider::lark -- --nocapture`
Expected: PASS — all provider tests green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/task_provider/lark.rs
git commit -m "feat(phase-3a-3-1): route list_tasks to search endpoint when filters active"
```

---

### Task 9: `list_lark_fields` Tauri command

**Files:**

- Modify: `src-tauri/src/commands/lark_repo_binding.rs` (add new fn + wrapper
  near `detect_lark_schema_inner` ~line 121-134)
- Modify: `src-tauri/src/lib.rs` (register in `invoke_handler!`)

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/commands/lark_repo_binding.rs` `tests` mod:

```rust
#[tokio::test]
async fn list_lark_fields_inner_returns_fields_via_lark_client() {
    use crate::platform::secret_store::tests::InMemorySecretStore;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [
                { "field_id": "fld1", "field_name": "Title", "type": 1,
                  "is_primary": true, "property": null },
                { "field_id": "fld2", "field_name": "Status", "type": 3,
                  "is_primary": false, "property": {
                    "options": [{"id":"o1","name":"Todo"},{"id":"o2","name":"Done"}]
                  }
                }
            ], "has_more": false, "page_token": null }
        })))
        .mount(&mock).await;

    let tmp = tempfile::tempdir().unwrap();
    write_lark_config(tmp.path(), &mock.uri()).expect("write config");
    let store = InMemorySecretStore::with_secret("app_secret", "sec");

    let fields = list_lark_fields_inner("appA", "tblA", tmp.path(), &store)
        .await
        .expect("ok");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].field_name, "Title");
    assert_eq!(fields[1].field_name, "Status");
}

#[tokio::test]
async fn list_lark_fields_inner_errors_when_creds_missing() {
    use crate::platform::secret_store::tests::InMemorySecretStore;

    let tmp = tempfile::tempdir().unwrap();
    // No lark config written.
    let store = InMemorySecretStore::default();
    let err = list_lark_fields_inner("appA", "tblA", tmp.path(), &store)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("config") || msg.to_lowercase().contains("creds"),
        "expected creds/config error, got: {msg}"
    );
}
```

Re-use the existing `mount_token` and `write_lark_config` helpers in this file
(they should already be there from Task 9 of Phase 3a-3 — if `write_lark_config`
is named differently, use the existing equivalent).

- [ ] **Step 2: Run tests to verify fail**

Run:
`cd src-tauri && cargo test --lib commands::lark_repo_binding::tests::list_lark_fields_inner -- --nocapture`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement**

Add to `src-tauri/src/commands/lark_repo_binding.rs` near
`detect_lark_schema_inner`:

```rust
pub(crate) async fn list_lark_fields_inner(
    app_token: &str,
    table_id: &str,
    data_dir: &Path,
    store: &dyn SecretStore,
) -> Result<Vec<crate::platform::lark_client::BitableField>, String> {
    let mut cfg = load_lark_config(data_dir)
        .map_err(|e| format!("load lark config: {e}"))?
        .ok_or_else(|| "lark config missing".to_string())?;
    let secret = store
        .get_secret("app_secret")
        .map_err(|e| format!("load app_secret: {e}"))?
        .ok_or_else(|| "lark app_secret missing from creds store".to_string())?;
    cfg.app_secret = secret;
    cfg.app_token = app_token.to_string();
    cfg.table_id = table_id.to_string();
    let client = LarkClient::new(cfg).map_err(|e| format!("build client: {e}"))?;
    client
        .bitable_list_fields(app_token, table_id)
        .await
        .map_err(|e| format!("list fields: {e}"))
}

#[tauri::command]
pub async fn list_lark_fields(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<crate::platform::lark_client::BitableField>, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve data_dir: {e}"))?;
    let store = platform::secret_store::system_secret_store()
        .map_err(|e| format!("secret store: {e}"))?;
    list_lark_fields_inner(&app_token, &table_id, &data_dir, store.as_ref()).await
}
```

Imports: `use std::path::Path;`,
`use crate::platform::{self, secret_store::SecretStore, lark_client::LarkClient};`,
`use tauri::Manager;` (use whichever already-present aliases the file follows).

- [ ] **Step 4: Run tests to verify pass**

Run:
`cd src-tauri && cargo test --lib commands::lark_repo_binding::tests::list_lark_fields_inner -- --nocapture`
Expected: PASS — 2 tests.

- [ ] **Step 5: Register command in `invoke_handler!`**

Edit `src-tauri/src/lib.rs` around line 212-255. In the `invoke_handler!` list,
add `commands::lark_repo_binding::list_lark_fields,` adjacent to the existing
`detect_lark_schema` entry.

- [ ] **Step 6: Build check**

Run: `cd src-tauri && cargo check --all-targets` Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/lark_repo_binding.rs src-tauri/src/lib.rs
git commit -m "feat(phase-3a-3-1): add list_lark_fields Tauri command"
```

---

### Task 10: Integration test — search endpoint end-to-end

**Files:**

- Modify: `src-tauri/src/lib.rs` (add to existing `migration_tests` module ~line
  407-763)

- [ ] **Step 1: Write failing test**

Add to the `migration_tests` mod:

```rust
#[tokio::test]
async fn binding_with_filters_hydrates_via_search_endpoint() {
    use crate::state::{FilterCondition, FilterConjunction, FilterOperator, FilterSpec};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;
    mount_fields(&mock).await; // existing helper

    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .and(body_partial_json(serde_json::json!({
            "filter": {
                "conjunction": "and",
                "conditions": [
                    { "field_name": "Status", "operator": "is", "value": ["Done"] }
                ]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": {
                "items": [
                    { "record_id": "rec_done_1",
                      "fields": { "Title": "Task A", "Status": "Done" } }
                ],
                "has_more": false, "page_token": null, "total": 1
            }
        })))
        .expect(1)
        .mount(&mock).await;

    let client = std::sync::Arc::new(
        platform::lark_client::LarkClient::new(platform::lark_client::LarkConfig {
            app_id: "id".into(), app_secret: "sec".into(),
            app_token: "appA".into(), table_id: "tblA".into(),
            base_url: mock.uri(),
        }).unwrap()
    );
    let provider = task_provider::lark::LarkProvider::new(
        client, "appA".into(), "tblA".into(),
        FilterSpec {
            conjunction: FilterConjunction::And,
            conditions: vec![FilterCondition {
                field_id: "fld_status".into(),
                field_name: "Status".into(),
                operator: FilterOperator::Is,
                value: vec!["Done".into()],
            }],
        },
        state::FieldMapping {
            title: state::FieldRef { field_id: "fld_title".into(), field_name: "Title".into() },
            description: None,
            status: Some(state::FieldRef { field_id: "fld_status".into(), field_name: "Status".into() }),
            order: None,
        },
        state::StatusValueMapping::default(),
    );

    let tasks = provider.list_tasks("repo-x").await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Task A");
}

#[tokio::test]
async fn binding_with_empty_filters_uses_list_endpoint() {
    use crate::state::{FilterConjunction, FilterSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    mount_token(&mock).await;

    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok",
            "data": { "items": [
                { "record_id": "rec1", "fields": { "Title": "Any" } }
            ], "has_more": false, "page_token": null, "total": 1 }
        })))
        .expect(1)
        .mount(&mock).await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/records/search"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock).await;

    let client = std::sync::Arc::new(
        platform::lark_client::LarkClient::new(platform::lark_client::LarkConfig {
            app_id: "id".into(), app_secret: "sec".into(),
            app_token: "appA".into(), table_id: "tblA".into(),
            base_url: mock.uri(),
        }).unwrap()
    );
    let provider = task_provider::lark::LarkProvider::new(
        client, "appA".into(), "tblA".into(),
        FilterSpec { conjunction: FilterConjunction::And, conditions: vec![] },
        state::FieldMapping {
            title: state::FieldRef { field_id: "fld_title".into(), field_name: "Title".into() },
            description: None, status: None, order: None,
        },
        state::StatusValueMapping::default(),
    );

    let tasks = provider.list_tasks("repo-x").await.unwrap();
    assert_eq!(tasks.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify pass**

Run:
`cd src-tauri && cargo test --lib binding_with_filters_hydrates_via_search_endpoint binding_with_empty_filters_uses_list_endpoint -- --nocapture`
Expected: PASS — 2 tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "test(phase-3a-3-1): end-to-end search/list endpoint routing via LarkProvider"
```

---

### Task 11: TS types for filters

**Files:**

- Modify: `src/lib/types.ts:284-291` (`BitableBinding`) + append new exports
  near line 268

- [ ] **Step 1: Write failing test**

Create `src/lib/types.test.ts` (or append if exists):

```ts
import { describe, expect, it } from 'vitest';
import type { BitableBinding, FilterSpec } from './types';

describe('FilterSpec types', () => {
  it('BitableBinding includes filters field with default empty spec shape', () => {
    const empty: FilterSpec = { conjunction: 'and', conditions: [] };
    const b: BitableBinding = {
      app_token: 'app',
      table_id: 'tbl',
      filters: empty,
      field_mapping: { title: { field_id: 'f', field_name: 'F' } },
      status_value_mapping: { entries: {}, default_column: 'Todo' },
      created_at: 0,
      updated_at: 0,
    };
    expect(b.filters.conditions).toEqual([]);
    expect(b.filters.conjunction).toBe('and');
  });

  it('FilterOperator literal type accepts all 10 operators', () => {
    const ops: import('./types').FilterOperator[] = [
      'is',
      'isNot',
      'contains',
      'doesNotContain',
      'isEmpty',
      'isNotEmpty',
      'isGreater',
      'isGreaterEqual',
      'isLess',
      'isLessEqual',
    ];
    expect(ops).toHaveLength(10);
  });
});
```

- [ ] **Step 2: Run test to verify fail**

Run: `bun run test src/lib/types.test.ts` Expected: FAIL — types missing.

- [ ] **Step 3: Add types**

Append to `src/lib/types.ts` near existing FilterRef block (~line 268, before
`FieldMapping`):

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
```

Edit `BitableBinding` at line 284-291 — insert `filters: FilterSpec;` between
`table_id` and `field_mapping`:

```ts
export type BitableBinding = {
  app_token: string;
  table_id: string;
  filters: FilterSpec;
  field_mapping: FieldMapping;
  status_value_mapping: StatusValueMapping;
  created_at: number;
  updated_at: number;
};
```

- [ ] **Step 4: Run test to verify pass**

Run: `bun run test src/lib/types.test.ts && bun run check` Expected: PASS —
type-check green.

- [ ] **Step 5: Fix any TS call-site breaks**

`bun run check` may surface BitableBinding literal construction without
`filters`. Add `filters: { conjunction: 'and', conditions: [] }` to each. Likely
sites: wizard step files in `src/lib/components/lark/`, store seeds.

- [ ] **Step 6: Commit**

```bash
git add src/lib/types.ts src/lib/types.test.ts $(git ls-files -m src/)
git commit -m "feat(phase-3a-3-1): add FilterSpec TS types and filters field on BitableBinding"
```

---

### Task 12: IPC wrapper `api.lark.listFields`

**Files:**

- Modify: `src/lib/ipc.ts:219-236` (extend `api.lark` namespace)

- [ ] **Step 1: Write failing test**

Create `src/lib/ipc.test.ts` (or append):

```ts
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args: unknown) => {
    if (cmd === 'list_lark_fields') {
      expect(args).toEqual({ appToken: 'appA', tableId: 'tblA' });
      return [
        {
          field_id: 'fld1',
          field_name: 'Title',
          type: 1,
          is_primary: true,
          property: null,
        },
      ];
    }
    throw new Error(`unmocked invoke: ${cmd}`);
  }),
}));

const { api } = await import('./ipc');

describe('api.lark.listFields', () => {
  it('invokes list_lark_fields with camelCase params and returns BitableField[]', async () => {
    const result = await api.lark.listFields('appA', 'tblA');
    expect(result).toHaveLength(1);
    expect(result[0].field_name).toBe('Title');
  });
});
```

- [ ] **Step 2: Run test to verify fail**

Run: `bun run test src/lib/ipc.test.ts` Expected: FAIL —
`api.lark.listFields is not a function`.

- [ ] **Step 3: Add wrapper**

Edit `src/lib/ipc.ts:230-236`. Add a new method to the `api.lark` object:

```ts
async listFields(appToken: string, tableId: string): Promise<BitableField[]> {
  return invoke('list_lark_fields', { appToken, tableId });
},
```

(Ensure `BitableField` is imported at top of file.)

- [ ] **Step 4: Run test to verify pass**

Run: `bun run test src/lib/ipc.test.ts && bun run check` Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/ipc.ts src/lib/ipc.test.ts
git commit -m "feat(phase-3a-3-1): add api.lark.listFields IPC wrapper"
```

---

### Task 13: `filterStore` (Svelte 5 runes)

**Files:**

- Create: `src/lib/stores/lark-binding-filters.svelte.ts`
- Create: `src/lib/stores/lark-binding-filters.svelte.test.ts`

- [ ] **Step 1: Write failing tests**

Create `src/lib/stores/lark-binding-filters.svelte.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach } from 'vitest';

const setRepoBinding = vi.fn();
const refresh = vi.fn();
const addToast = vi.fn();

vi.mock('$lib/ipc', () => ({
  api: {
    lark: { setRepoBinding: (...a: unknown[]) => setRepoBinding(...a) },
    task: { refresh: (...a: unknown[]) => refresh(...a) },
  },
}));
vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: (...a: unknown[]) => addToast(...a),
}));

// Mock larkBindings store with a tiny SvelteMap-like impl.
const bindings = new Map<string, any>();
const baseBinding = {
  app_token: 'appA',
  table_id: 'tblA',
  filters: { conjunction: 'and' as const, conditions: [] },
  field_mapping: { title: { field_id: 'f', field_name: 'F' } },
  status_value_mapping: { entries: {}, default_column: 'Todo' as const },
  created_at: 0,
  updated_at: 0,
};
bindings.set('repo-1', { ...baseBinding });

vi.mock('$lib/stores/lark-bindings.svelte', () => ({
  larkBindings: {
    get: (repoId: string) => bindings.get(repoId),
    bindings: { set: (k: string, v: unknown) => bindings.set(k, v) },
  },
}));

beforeEach(() => {
  setRepoBinding.mockReset();
  refresh.mockReset();
  addToast.mockReset();
  bindings.set('repo-1', { ...baseBinding });
  vi.useFakeTimers();
});

describe('filterStore.update', () => {
  it('lands optimistic update immediately then persists after 300 ms debounce', async () => {
    const { filterStore } = await import('./lark-binding-filters.svelte');
    setRepoBinding.mockResolvedValue(undefined);
    refresh.mockResolvedValue(undefined);

    const next = {
      conjunction: 'and' as const,
      conditions: [
        {
          field_id: 'f1',
          field_name: 'F1',
          operator: 'is' as const,
          value: ['x'],
        },
      ],
    };
    await filterStore.update('repo-1', next);

    expect(bindings.get('repo-1').filters).toEqual(next);
    expect(setRepoBinding).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(300);
    expect(setRepoBinding).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('reverts optimistic update and toasts when persist fails', async () => {
    const { filterStore } = await import('./lark-binding-filters.svelte');
    setRepoBinding.mockRejectedValue(new Error('disk full'));

    const next = { conjunction: 'or' as const, conditions: [] };
    await filterStore.update('repo-1', next);
    await vi.advanceTimersByTimeAsync(300);
    await vi.runAllTimersAsync();

    expect(bindings.get('repo-1').filters).toEqual(baseBinding.filters);
    expect(addToast).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run tests to verify fail**

Run: `bun run test src/lib/stores/lark-binding-filters.svelte.test.ts` Expected:
FAIL — store does not exist.

- [ ] **Step 3: Create the store**

Create `src/lib/stores/lark-binding-filters.svelte.ts`:

```ts
import { api } from '$lib/ipc';
import { larkBindings } from '$lib/stores/lark-bindings.svelte';
import { addToast } from '$lib/stores/toasts.svelte';
import type { FilterSpec } from '$lib/types';

const DEBOUNCE_MS = 300;

class FilterStore {
  private timeouts = new Map<string, ReturnType<typeof setTimeout>>();

  async update(repoId: string, spec: FilterSpec): Promise<void> {
    const current = larkBindings.get(repoId);
    if (!current) return;
    const previous = { ...current };

    // Optimistic local update.
    larkBindings.bindings.set(repoId, { ...current, filters: spec });

    const existing = this.timeouts.get(repoId);
    if (existing) clearTimeout(existing);

    this.timeouts.set(
      repoId,
      setTimeout(async () => {
        try {
          await api.lark.setRepoBinding(repoId, { ...current, filters: spec });
          await api.task.refresh(repoId);
        } catch (err) {
          // Revert optimistic update.
          larkBindings.bindings.set(repoId, previous);
          addToast(
            `Filter save failed: ${err instanceof Error ? err.message : err}`,
            'error'
          );
        }
      }, DEBOUNCE_MS)
    );
  }
}

export const filterStore = new FilterStore();
```

If `api.task.refresh` and `addToast` import paths differ in this codebase,
adjust (e.g. `$lib/stores/toasts.svelte.ts` vs `.svelte`). The test mocks must
match the production paths exactly — fix both sides in lockstep.

- [ ] **Step 4: Run tests to verify pass**

Run: `bun run test src/lib/stores/lark-binding-filters.svelte.test.ts` Expected:
PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/lark-binding-filters.svelte.ts src/lib/stores/lark-binding-filters.svelte.test.ts
git commit -m "feat(phase-3a-3-1): add filterStore with debounced persist + optimistic update"
```

---

### Task 14: `FilterBar.svelte` — empty state + Add filter button

**Files:**

- Create: `src/lib/components/kanban/FilterBar.svelte`
- Create: `src/lib/components/kanban/FilterBar.test.ts`

- [ ] **Step 1: Write failing test**

Create `src/lib/components/kanban/FilterBar.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import FilterBar from './FilterBar.svelte';

vi.mock('$lib/ipc', () => ({
  api: { lark: { listFields: vi.fn(async () => []) } },
}));

describe('FilterBar — empty state', () => {
  it('renders + Add filter button when no conditions present', () => {
    render(FilterBar, {
      props: {
        repoId: 'repo-1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    expect(
      screen.getByRole('button', { name: /add filter/i })
    ).toBeInTheDocument();
  });

  it('does not show conjunction toggle when 0 or 1 conditions', () => {
    render(FilterBar, {
      props: {
        repoId: 'repo-1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    expect(
      screen.queryByRole('combobox', { name: /conjunction/i })
    ).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to verify fail**

Run: `bun run test src/lib/components/kanban/FilterBar.test.ts` Expected: FAIL —
component file not found.

- [ ] **Step 3: Create minimal `FilterBar.svelte`**

Create `src/lib/components/kanban/FilterBar.svelte`:

```svelte
<script lang="ts">
  import type { FilterSpec, BitableField } from '$lib/types';
  import { api } from '$lib/ipc';

  type Props = {
    repoId: string;
    appToken: string;
    tableId: string;
    filters: FilterSpec;
  };

  let { repoId, appToken, tableId, filters }: Props = $props();

  let fields = $state<BitableField[]>([]);
  let pickerOpen = $state(false);

  async function openPicker() {
    pickerOpen = true;
    if (fields.length === 0) {
      try {
        fields = await api.lark.listFields(appToken, tableId);
      } catch (err) {
        fields = [];
      }
    }
  }
</script>

<div class="filter-bar" data-repo-id={repoId}>
  {#if filters.conditions.length >= 2}
    <select aria-label="conjunction">
      <option value="and" selected={filters.conjunction === 'and'}>all</option>
      <option value="or" selected={filters.conjunction === 'or'}>any</option>
    </select>
  {/if}

  <button type="button" onclick={openPicker}>+ Add filter</button>

  {#if pickerOpen}
    <div role="dialog" aria-label="Pick column">
      {#each fields as field (field.field_id)}
        <button type="button">{field.field_name}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .filter-bar {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--border, #e0e0e0);
  }
</style>
```

- [ ] **Step 4: Run test to verify pass**

Run: `bun run test src/lib/components/kanban/FilterBar.test.ts` Expected: PASS —
2 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/kanban/FilterBar.svelte src/lib/components/kanban/FilterBar.test.ts
git commit -m "feat(phase-3a-3-1): FilterBar empty state and column picker scaffolding"
```

---

### Task 15: `FilterBar.svelte` — add filter flow, operator + value pickers

**Files:**

- Modify: `src/lib/components/kanban/FilterBar.svelte`
- Modify: `src/lib/components/kanban/FilterBar.test.ts`

- [ ] **Step 1: Write failing tests**

Append to `src/lib/components/kanban/FilterBar.test.ts`:

```ts
import { fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

const listFieldsMock = vi.fn();
vi.mocked(await import('$lib/ipc')).api.lark.listFields = listFieldsMock;

describe('FilterBar — add condition flow', () => {
  it('clicking column populates operator list for SingleSelect (type 3)', async () => {
    listFieldsMock.mockResolvedValue([
      {
        field_id: 'fldStat',
        field_name: 'Status',
        type: 3,
        is_primary: false,
        property: {
          options: [
            { id: 'o1', name: 'Todo' },
            { id: 'o2', name: 'Done' },
          ],
        },
      },
    ]);

    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await tick();
    await fireEvent.click(await screen.findByText('Status'));
    await tick();
    const ops = await screen.findAllByRole('option');
    const labels = ops.map((o) => o.textContent?.trim()).filter(Boolean);
    expect(labels).toEqual(
      expect.arrayContaining(['is', 'isNot', 'isEmpty', 'isNotEmpty'])
    );
    expect(labels).not.toEqual(expect.arrayContaining(['contains'])); // not a single-select op
  });

  it('Text (type 1) shows is/isNot/contains/doesNotContain/isEmpty/isNotEmpty', async () => {
    listFieldsMock.mockResolvedValue([
      {
        field_id: 'fldT',
        field_name: 'Title',
        type: 1,
        is_primary: true,
        property: null,
      },
    ]);
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await fireEvent.click(await screen.findByText('Title'));
    const ops = await screen.findAllByRole('option');
    const labels = ops.map((o) => o.textContent?.trim());
    expect(labels).toEqual([
      'is',
      'isNot',
      'contains',
      'doesNotContain',
      'isEmpty',
      'isNotEmpty',
    ]);
  });
});
```

- [ ] **Step 2: Run tests to verify fail**

Run: `bun run test src/lib/components/kanban/FilterBar.test.ts` Expected: FAIL —
column click doesn't yet render operator list.

- [ ] **Step 3: Extend `FilterBar.svelte`**

Replace the contents of `src/lib/components/kanban/FilterBar.svelte` with:

```svelte
<script lang="ts">
  import type {
    FilterSpec,
    FilterCondition,
    FilterOperator,
    BitableField,
  } from '$lib/types';
  import { api } from '$lib/ipc';
  import { filterStore } from '$lib/stores/lark-binding-filters.svelte';

  type Props = {
    repoId: string;
    appToken: string;
    tableId: string;
    filters: FilterSpec;
  };
  let { repoId, appToken, tableId, filters }: Props = $props();

  let fields = $state<BitableField[]>([]);
  let pickerOpen = $state(false);
  let pickedField = $state<BitableField | null>(null);

  const OPS_BY_TYPE: Record<number, FilterOperator[]> = {
    1: ['is', 'isNot', 'contains', 'doesNotContain', 'isEmpty', 'isNotEmpty'], // Text
    2: [
      'is',
      'isNot',
      'isGreater',
      'isGreaterEqual',
      'isLess',
      'isLessEqual',
      'isEmpty',
      'isNotEmpty',
    ], // Number
    3: ['is', 'isNot', 'isEmpty', 'isNotEmpty'], // SingleSelect
    4: ['contains', 'doesNotContain', 'isEmpty', 'isNotEmpty'], // MultiSelect
    5: [
      'is',
      'isNot',
      'isGreater',
      'isGreaterEqual',
      'isLess',
      'isLessEqual',
      'isEmpty',
      'isNotEmpty',
    ], // DateTime
    11: ['is', 'isNot', 'contains', 'doesNotContain', 'isEmpty', 'isNotEmpty'], // Person
  };

  const UNARY: ReadonlyArray<FilterOperator> = ['isEmpty', 'isNotEmpty'];

  function isSupported(t: number): boolean {
    return t in OPS_BY_TYPE;
  }

  async function openPicker() {
    pickerOpen = true;
    if (fields.length === 0) {
      try {
        fields = await api.lark.listFields(appToken, tableId);
      } catch (err) {
        fields = [];
      }
    }
  }

  function pickField(f: BitableField) {
    if (!isSupported(f.type)) return;
    pickedField = f;
  }

  async function addCondition(op: FilterOperator) {
    if (!pickedField) return;
    const cond: FilterCondition = {
      field_id: pickedField.field_id,
      field_name: pickedField.field_name,
      operator: op,
      value: UNARY.includes(op) ? [] : [''],
    };
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: [...filters.conditions, cond],
    };
    pickerOpen = false;
    pickedField = null;
    await filterStore.update(repoId, next);
  }

  async function removeCondition(idx: number) {
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.filter((_, i) => i !== idx),
    };
    await filterStore.update(repoId, next);
  }

  async function setConjunction(c: 'and' | 'or') {
    await filterStore.update(repoId, { ...filters, conjunction: c });
  }

  async function setValue(idx: number, value: string) {
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.map((c, i) =>
        i === idx ? { ...c, value: [value] } : c
      ),
    };
    await filterStore.update(repoId, next);
  }
</script>

<div class="filter-bar" data-repo-id={repoId}>
  {#if filters.conditions.length >= 2}
    <select
      aria-label="conjunction"
      value={filters.conjunction}
      onchange={(e) => setConjunction(e.currentTarget.value as 'and' | 'or')}
    >
      <option value="and">all</option>
      <option value="or">any</option>
    </select>
    of the conditions
  {/if}

  {#each filters.conditions as cond, idx (cond.field_id + idx)}
    <span class="chip">
      <span>{cond.field_name}</span>
      <span>{cond.operator}</span>
      {#if !UNARY.includes(cond.operator)}
        <input
          type="text"
          value={cond.value[0] ?? ''}
          oninput={(e) => setValue(idx, e.currentTarget.value)}
        />
      {/if}
      <button
        type="button"
        aria-label="remove condition"
        onclick={() => removeCondition(idx)}>×</button
      >
    </span>
  {/each}

  <button type="button" onclick={openPicker}>+ Add filter</button>

  {#if pickerOpen && !pickedField}
    <div role="dialog" aria-label="Pick column">
      {#each fields as field (field.field_id)}
        <button
          type="button"
          disabled={!isSupported(field.type)}
          onclick={() => pickField(field)}
        >
          {field.field_name}{#if !isSupported(field.type)}
            (not supported){/if}
        </button>
      {/each}
    </div>
  {/if}

  {#if pickedField}
    <div role="listbox" aria-label="Pick operator">
      {#each OPS_BY_TYPE[pickedField.type] as op (op)}
        <button
          type="button"
          role="option"
          aria-selected="false"
          onclick={() => addCondition(op)}>{op}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .filter-bar {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--border, #e0e0e0);
    flex-wrap: wrap;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.125rem 0.5rem;
    border: 1px solid var(--border, #d0d0d0);
    border-radius: 999px;
    background: var(--chip-bg, #f5f5f5);
  }
  .chip input {
    width: 8rem;
  }
</style>
```

- [ ] **Step 4: Run tests to verify pass**

Run: `bun run test src/lib/components/kanban/FilterBar.test.ts` Expected: PASS —
4 tests (2 from Task 14 + 2 new).

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/kanban/FilterBar.svelte src/lib/components/kanban/FilterBar.test.ts
git commit -m "feat(phase-3a-3-1): FilterBar add-condition flow with per-type operators"
```

---

### Task 16: `FilterBar.svelte` — chip remove + AND/OR + debounce assertion

**Files:**

- Modify: `src/lib/components/kanban/FilterBar.test.ts`

- [ ] **Step 1: Write failing tests**

Append to `src/lib/components/kanban/FilterBar.test.ts`:

```ts
import { filterStore } from '$lib/stores/lark-binding-filters.svelte';
const updateSpy = vi.spyOn(filterStore, 'update');

describe('FilterBar — chip mutations', () => {
  it('clicking × removes that condition and calls filterStore.update', async () => {
    updateSpy.mockClear();
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [
            {
              field_id: 'fldS',
              field_name: 'Status',
              operator: 'is',
              value: ['Done'],
            },
          ],
        },
      },
    });
    await fireEvent.click(
      screen.getByRole('button', { name: /remove condition/i })
    );
    expect(updateSpy).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conditions: [],
      })
    );
  });

  it('changing AND/OR triggers update with new conjunction', async () => {
    updateSpy.mockClear();
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [
            { field_id: 'a', field_name: 'A', operator: 'is', value: ['x'] },
            { field_id: 'b', field_name: 'B', operator: 'is', value: ['y'] },
          ],
        },
      },
    });
    const select = screen.getByRole('combobox', {
      name: /conjunction/i,
    }) as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'or' } });
    expect(updateSpy).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conjunction: 'or',
      })
    );
  });
});
```

- [ ] **Step 2: Run tests to verify pass**

Run: `bun run test src/lib/components/kanban/FilterBar.test.ts` Expected: PASS —
6 tests total.

(Component code from Task 15 already supports both flows; this task only adds
regression coverage.)

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/kanban/FilterBar.test.ts
git commit -m "test(phase-3a-3-1): cover FilterBar chip remove and AND/OR toggle"
```

---

### Task 17: Mount `FilterBar` in `KanbanBoard.svelte`

**Files:**

- Modify: `src/lib/components/kanban/KanbanBoard.svelte`

- [ ] **Step 1: Read the file to find the column-row markup**

Run: `cat src/lib/components/kanban/KanbanBoard.svelte`

Identify the top-level wrapper element that contains the 4 column components.

- [ ] **Step 2: Add the import and mount**

Add to the `<script lang="ts">` block (imports section):

```ts
import FilterBar from './FilterBar.svelte';
import { larkBindings } from '$lib/stores/lark-bindings.svelte';
```

Read the existing props — KanbanBoard already receives `repoId` (verify by
`cat`); if not, add it to its `Props` type.

Above the column row, insert:

```svelte
{#if larkBindings.get(repoId)}
  {@const binding = larkBindings.get(repoId)!}
  <FilterBar
    repoId={repoId}
    appToken={binding.app_token}
    tableId={binding.table_id}
    filters={binding.filters}
  />
{/if}
```

- [ ] **Step 3: Type check + visual smoke**

Run: `bun run check` Expected: PASS.

Run the dev app and manually verify the bar shows above the kanban when a
binding is configured:

```bash
bun run tauri dev
```

Expected: chip bar visible above columns. Clicking `+ Add filter` opens column
picker.

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/kanban/KanbanBoard.svelte
git commit -m "feat(phase-3a-3-1): mount FilterBar above kanban columns"
```

---

### Task 18: E2E test — filter narrows kanban

**Files:**

- Create: `tests/e2e/phase-3a-3-1/phase-3a-3-1-filter-bar.spec.ts`

- [ ] **Step 1: Inspect existing E2E pattern**

Run:
`ls tests/e2e/ && cat tests/e2e/phase-3a-3/*.spec.ts 2>/dev/null | head -80`

Pattern to mirror: env-gated (`process.env.ANSAMBEL_E2E === '1'`),
`ANSAMBEL_MOCK_CLAUDE=1`, Playwright `_electron.launch` (or whatever the
existing tests use).

- [ ] **Step 2: Write E2E spec**

Create `tests/e2e/phase-3a-3-1/phase-3a-3-1-filter-bar.spec.ts`:

```ts
import { expect, test } from '@playwright/test';
import { launchApp, seedLarkBinding, type AppHandle } from '../helpers';

test.describe.configure({ mode: 'serial' });

test.describe('Phase 3a-3.1 — Filter bar', () => {
  let app: AppHandle;

  test.beforeAll(async () => {
    app = await launchApp({
      mockLark: {
        search: [
          {
            record_id: 'r_done',
            fields: { Title: 'Done task', Status: 'Done' },
          },
        ],
        list: [
          {
            record_id: 'r_done',
            fields: { Title: 'Done task', Status: 'Done' },
          },
          {
            record_id: 'r_todo',
            fields: { Title: 'Todo task', Status: 'Todo' },
          },
        ],
        fields: [
          {
            field_id: 'fld_status',
            field_name: 'Status',
            type: 3,
            is_primary: false,
            property: {
              options: [
                { id: 'o1', name: 'Todo' },
                { id: 'o2', name: 'Done' },
              ],
            },
          },
        ],
      },
    });
    await seedLarkBinding(app, {
      repoId: 'repo-e2e',
      appToken: 'appE2E',
      tableId: 'tblE2E',
    });
  });

  test.afterAll(async () => {
    await app.close();
  });

  test('filter bar adds Status=Done and kanban narrows to one task', async () => {
    const page = await app.firstWindow();
    await page.click(
      '[data-repo-id="repo-e2e"] button:has-text("+ Add filter")'
    );
    await page.click('text=Status');
    await page.click('role=option[name="is"]');
    // FilterBar issues optimistic update + 300ms debounce + refresh.
    await page.waitForResponse((r) => r.url().includes('/records/search'), {
      timeout: 5_000,
    });
    await expect(page.locator('text=Done task')).toBeVisible();
    await expect(page.locator('text=Todo task')).toHaveCount(0);
  });

  test('removing the only filter restores all tasks', async () => {
    const page = await app.firstWindow();
    await page.click('button[aria-label="remove condition"]');
    await page.waitForResponse(
      (r) => r.url().includes('/records') && !r.url().includes('/search')
    );
    await expect(page.locator('text=Done task')).toBeVisible();
    await expect(page.locator('text=Todo task')).toBeVisible();
  });
});
```

If `tests/e2e/helpers.ts` does not yet expose `mockLark` config with the
`search`/`list`/`fields` keys, extend it in the same commit; otherwise this test
will need helper updates as a sub-step.

- [ ] **Step 3: Run the E2E**

Run: `ANSAMBEL_E2E=1 bun run e2e tests/e2e/phase-3a-3-1/` Expected: 2 tests
pass.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e/phase-3a-3-1/ tests/e2e/helpers.ts
git commit -m "test(phase-3a-3-1): E2E filter bar narrows kanban via Lark search endpoint"
```

---

### Task 19: Final lint + coverage gate

**Files:**

- None (verification only)

- [ ] **Step 1: Format Rust**

Run: `cd src-tauri && cargo fmt --all` Expected: no diff.

- [ ] **Step 2: Clippy**

Run: `cd src-tauri && cargo clippy --lib --all-targets -- -D warnings` Expected:
PASS — zero warnings.

- [ ] **Step 3: TypeScript + ESLint + Prettier**

Run: `bun run lint && bun run check` Expected: PASS.

- [ ] **Step 4: Rust unit + integration tests**

Run: `cd src-tauri && cargo test --lib` Expected: ALL PASS.

- [ ] **Step 5: Frontend tests + coverage**

Run: `bun run test:coverage` Expected: ≥95 % line + branch + function on changed
files.

- [ ] **Step 6: Rust coverage**

Run:

```bash
cd src-tauri && cargo llvm-cov --lib \
  --ignore-filename-regex 'lib\.rs$|main\.rs$|commands[/\\](repo|workspace|task|agent|diff|files|file_io|search|scripts|terminal|lark_auth|lark_repo_binding)\.rs$|platform[/\\](pty|lark_client)\.rs$' \
  --fail-under-lines 95 --fail-under-functions 94
```

Expected: PASS — same regex as CI. If `task_provider/lark.rs` or `state.rs` slip
below 95, add targeted tests before pushing.

- [ ] **Step 7: Push and open PR**

```bash
git push -u origin feat/phase-3a-3-1-filter-aware-lark-binding
gh pr create --base main --title "feat(phase-3a-3-1): filter-aware Lark binding (v2)" --body "$(cat <<'EOF'
## Summary
- Replaces the closed PR #27 view-scope approach with an in-app Lark-style filter UI above the kanban.
- New `FilterSpec` on `BitableBinding`; routes `list_tasks` to Lark's `records/search` POST endpoint when filters are active.
- Schema migration v1 → v3 (skips v2 since PR #27 never shipped).

## Spec
- `docs/superpowers/specs/2026-05-17-phase-3a-3-1-filter-aware-lark-binding-design.md`

## Test plan
- [ ] Rust: `cargo test --lib` green
- [ ] Frontend: `bun run test:coverage` ≥95 %
- [ ] Manual: configure a binding, add a `Status is Done` filter, verify kanban narrows; remove the filter, verify restore.
- [ ] CI Rust coverage gate green (ignore-regex unchanged)
- [ ] CI E2E green on Ubuntu / Windows / macOS

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR opens; CI runs.

---

## Self-review checklist (for the planner)

- [x] Every spec section maps to ≥1 task:
  - Architecture diagram → Tasks 7, 8, 14, 17
  - `FilterCondition` / `FilterOperator` / `FilterConjunction` / `FilterSpec`
    types → Tasks 1, 2, 11
  - `BitableBinding.filters` field with `#[serde(default)]` → Task 3
  - `bitable_search_records` API → Task 6
  - `list_lark_fields` command → Task 9
  - Operator × field type matrix → Task 15
  - Read-path routing → Task 8
  - Filter store (300 ms debounce, optimistic + revert) → Task 13
  - Schema migration v1 → v3 → Tasks 4, 5
  - Field-rename robustness (OnceCell + `refresh_field_names`) → Task 7
  - Unit, integration, component, E2E tests → Tasks 1-3, 4, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 18
- [x] All Rust symbol names match the spec (`FilterOperator`,
      `FilterConjunction`, `FilterCondition`, `FilterSpec`,
      `bitable_search_records`, `list_lark_fields_inner`, `refresh_field_names`,
      `field_name_by_id`).
- [x] All TS symbol names match the Rust serialization (lowercase
      `'and' | 'or'`, camelCase operator strings).
- [x] No "TBD" / "implement later" / "similar to Task N" placeholders.
- [x] Every code-modifying step shows the actual code (no skeleton bullets).
- [x] Every task ends with a commit using conventional-commit prefixes accepted
      by commitlint.
- [x] Coverage gate addressed (`commands/lark_repo_binding.rs` already in CI
      ignore-regex; `_inner` helpers tested).
- [x] Type consistency: `LarkProvider::new` arg order —
      `client, app_token, table_id, filters, field_mapping, status_value_mapping`
      — is used identically in Tasks 7, 8, 10, 13.
- [x] FilterBar prop name `filters` is used identically in component code (Tasks
      14, 15) and in mount call (Task 17).
