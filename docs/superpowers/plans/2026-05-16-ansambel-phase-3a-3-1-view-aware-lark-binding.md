# Phase 3a-3.1 — View-Aware Lark Binding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ansambel honors per-view filters configured in Lark Bitable. Each
binding can scope to one Bitable view so the user's existing "Current Sprint" /
"Backlog" / "My Open Tasks" filter is reflected in the kanban instead of dumping
the full table.

**Architecture:** Add `view_id: Option<String>` to `BitableBinding`. Extend
`bitable_list_records` and `LarkProvider::list_tasks` to pass the optional
`view_id` so Lark applies the view's filter server-side. New
`bitable_list_views` helper + Tauri command `list_lark_views` powers a new
wizard Step 1.5. View-deletion at runtime auto-falls-back to all records and
surfaces a `lark-view-missing` event for a non-blocking banner.

**Tech Stack:** Rust (existing `async-trait` + `reqwest` + `wiremock` +
`tokio::sync::RwLock` + `tokio::sync::OnceCell`), Svelte 5 runes, Vitest +
Testing Library, Playwright (env-gated).

**Spec:**
`docs/superpowers/specs/2026-05-16-phase-3a-3-1-view-aware-lark-binding-design.md`

---

## File Structure

### Modify

- `src-tauri/src/platform/lark_client.rs` — add `BitableView` type + helper
  `bitable_list_views`; extend `bitable_list_records` signature with
  `view_id: Option<&str>`.
- `src-tauri/src/state.rs` — add `view_id: Option<String>` to `BitableBinding`.
- `src-tauri/src/persistence/lark_repo_bindings.rs` — bump
  `default_schema_version` 1 → 2; add no-op migration helper; legacy-load test.
- `src-tauri/src/task_provider/lark.rs` — `LarkProvider` stores `view_id`;
  threads it through `list_tasks`; view-404 fallback + event emission.
- `src-tauri/src/commands/lark_repo_binding.rs` — new Tauri command
  `list_lark_views`.
- `src-tauri/src/lib.rs` — register `list_lark_views` command in invoke handler;
  run no-op migration on startup.
- `src/lib/types.ts` — `BitableView` type; `view_id` on `BitableBinding`.
- `src/lib/ipc.ts` — `listViews` wrapper.
- `src/lib/components/lark/LarkBindingWizard.svelte` — Step 1.5 view picker.
- `src/lib/components/repo/RepoSettingsDialog.svelte` — "View:" row + Change
  view button.
- `src/lib/components/TitleBar.svelte` — `lark-view-missing` banner + listener.
- `src/lib/stores/view-missing.svelte.ts` — **new** — Svelte store driven by
  Tauri event for banner state.

### Test files

- Inline `#[cfg(test)] mod tests` blocks in each modified `.rs` file (existing
  pattern).
- `src/lib/components/lark/LarkBindingWizard.test.ts` — extend.
- `src/lib/components/repo/RepoSettingsDialog.test.ts` — extend.
- `src/lib/components/TitleBar.test.ts` — extend.
- `src/lib/stores/view-missing.svelte.test.ts` — **new**.
- `tests/e2e/lark-binding-view-scope.spec.ts` — **new**.

---

## Task 1: `BitableView` type + `bitable_list_views` helper

**Files:**

- Modify: `src-tauri/src/platform/lark_client.rs`

- [ ] **Step 1: Write failing test for views helper**

Add to the existing `#[cfg(test)] mod tests` block of
`src-tauri/src/platform/lark_client.rs` (alongside the existing `bitable_list_*`
tests):

```rust
#[tokio::test]
async fn bitable_list_views_returns_views_in_one_page() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/views",
        ))
        .and(header("authorization", "Bearer t_xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "msg": "ok",
            "data": {
                "items": [
                    { "view_id": "vw_grid", "view_name": "Grid view", "view_type": "grid" },
                    { "view_id": "vw_sprint", "view_name": "Current Sprint", "view_type": "grid" }
                ],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let views = client
        .bitable_list_views("bascntest", "tbltest")
        .await
        .unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].view_id, "vw_grid");
    assert_eq!(views[0].view_name, "Grid view");
    assert_eq!(views[0].view_type, "grid");
    assert_eq!(views[1].view_id, "vw_sprint");
}

#[tokio::test]
async fn bitable_list_views_paginates() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/views",
        ))
        .and(query_param("page_size", "500"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [{ "view_id": "v1", "view_name": "First", "view_type": "grid" }],
                "has_more": true,
                "page_token": "p2"
            }
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/views",
        ))
        .and(query_param("page_token", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [{ "view_id": "v2", "view_name": "Second", "view_type": "kanban" }],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let views = client
        .bitable_list_views("bascntest", "tbltest")
        .await
        .unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].view_id, "v1");
    assert_eq!(views[1].view_id, "v2");
}

#[tokio::test]
async fn bitable_list_views_surfaces_non_zero_code_as_error() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/views",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 1254000,
            "msg": "app_token invalid"
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let err = client
        .bitable_list_views("bascntest", "tbltest")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("1254000"), "{err}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_list_views
```

Expected: FAIL with "no method named `bitable_list_views`" / "cannot find type
`BitableView`".

- [ ] **Step 3: Add `BitableView` type**

In `src-tauri/src/platform/lark_client.rs`, near `BitableField` (around line
400), add:

```rust
/// A Bitable view returned by the list-views API. `view_type` is the
/// string Lark uses ("grid", "kanban", "form", "gantt", "gallery", ...).
/// Filters live inside each view but Lark does not expose the filter
/// expression on this endpoint — fetching records with `view_id` applies
/// the filter server-side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitableView {
    pub view_id: String,
    pub view_name: String,
    pub view_type: String,
}

#[derive(Deserialize)]
struct BitableViewListResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableViewListData>,
}

#[derive(Deserialize)]
struct BitableViewListData {
    #[serde(default)]
    items: Vec<BitableView>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    page_token: String,
}
```

- [ ] **Step 4: Add `bitable_list_views` method**

In `src-tauri/src/platform/lark_client.rs`, alongside `bitable_list_fields`
(around line 728), add:

```rust
    /// List all views in a Bitable table. Used by the binding wizard to
    /// populate the view-scope dropdown. Auto-paginates up to
    /// `MAX_LIST_PAGES` pages — production tables rarely have more than
    /// a few views, so pagination is precautionary.
    pub async fn bitable_list_views(
        &self,
        app_token: &str,
        table_id: &str,
    ) -> Result<Vec<BitableView>> {
        let token = self.tenant_access_token().await?;
        let mut out: Vec<BitableView> = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let url = format!(
                "{}/open-apis/bitable/v1/apps/{}/tables/{}/views",
                self.config.base_url, app_token, table_id
            );
            let resp = self
                .send_with_retry("bitable_list_views", || {
                    let mut req = self
                        .http
                        .get(&url)
                        .bearer_auth(&token)
                        .query(&[("page_size", DEFAULT_PAGE_SIZE.to_string())]);
                    if let Some(pt) = page_token.as_ref() {
                        req = req.query(&[("page_token", pt.as_str())]);
                    }
                    req
                })
                .await?;
            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| AppError::Lark(format!("bitable_list_views body: {e}")))?;
            if !status.is_success() {
                return Err(AppError::Lark(format!(
                    "bitable_list_views http {status}: {}",
                    truncate(&text, 200)
                )));
            }
            let parsed: BitableViewListResponse = serde_json::from_str(&text).map_err(|e| {
                AppError::Lark(format!(
                    "bitable_list_views parse: {e}; body={}",
                    truncate(&text, 200)
                ))
            })?;
            if parsed.code != 0 {
                return Err(AppError::Lark(format!(
                    "bitable_list_views code {}: {}",
                    parsed.code, parsed.msg
                )));
            }
            let data = parsed
                .data
                .ok_or_else(|| AppError::Lark("bitable_list_views missing data".into()))?;
            out.extend(data.items);
            if !data.has_more || data.page_token.is_empty() {
                return Ok(out);
            }
            page_token = Some(data.page_token);
        }
        Err(AppError::Lark(format!(
            "bitable_list_views pagination exceeded {MAX_LIST_PAGES} pages"
        )))
    }
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_list_views
```

Expected: 3/3 PASS.

- [ ] **Step 6: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/platform/lark_client.rs
git commit -m "feat(phase-3a-3-1): add bitable_list_views helper + BitableView type"
```

---

## Task 2: Extend `bitable_list_records` with `view_id`

**Files:**

- Modify: `src-tauri/src/platform/lark_client.rs` (signature + 1 call site
  inside)
- Modify: `src-tauri/src/task_provider/lark.rs` (call site)

- [ ] **Step 1: Write failing tests for new `view_id` parameter**

Add to `src-tauri/src/platform/lark_client.rs`'s test module:

```rust
#[tokio::test]
async fn bitable_list_records_with_view_id_passes_query_param() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
        ))
        .and(query_param("view_id", "vw_sprint"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": { "items": [], "has_more": false, "page_token": "" }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let records = client
        .bitable_list_records("bascntest", "tbltest", None, Some("vw_sprint"))
        .await
        .unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn bitable_list_records_with_no_view_id_omits_query_param() {
    use wiremock::matchers::query_param_is_missing;
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
        ))
        .and(query_param_is_missing("view_id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": { "items": [], "has_more": false, "page_token": "" }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let records = client
        .bitable_list_records("bascntest", "tbltest", None, None)
        .await
        .unwrap();
    assert!(records.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_list_records_with
```

Expected: FAIL — wrong number of arguments to `bitable_list_records`.

- [ ] **Step 3: Extend `bitable_list_records` signature**

In `src-tauri/src/platform/lark_client.rs`, change the signature and add the
view_id query (near line 475):

```rust
    /// List every record in a Bitable table, optionally filtered and/or
    /// scoped to one view. `filter` is the Lark expression grammar
    /// (`CurrentValue.[field]=value`). `view_id` applies the view's
    /// server-side filter — pass `None` to fetch the unfiltered table.
    /// Auto-paginates up to `MAX_LIST_PAGES` pages.
    pub async fn bitable_list_records(
        &self,
        app_token: &str,
        table_id: &str,
        filter: Option<&str>,
        view_id: Option<&str>,
    ) -> Result<Vec<BitableRecord>> {
```

Inside the request-builder closure, add the `view_id` branch alongside the
existing `filter` and `page_token` branches:

```rust
                    if let Some(f) = filter {
                        req = req.query(&[("filter", f)]);
                    }
                    if let Some(v) = view_id {
                        req = req.query(&[("view_id", v)]);
                    }
                    if let Some(pt) = page_token.as_ref() {
                        req = req.query(&[("page_token", pt.as_str())]);
                    }
```

- [ ] **Step 4: Update existing test call sites in `lark_client.rs`**

Find every existing call `bitable_list_records(..., None)` inside the test
module and add the new arg. There are several — locate them via:

```bash
cd src-tauri && grep -n "bitable_list_records(" src/platform/lark_client.rs
```

For each call site in the tests module, replace
`bitable_list_records("bascntest", "tbltest", None)` with
`bitable_list_records("bascntest", "tbltest", None, None)`, and similarly for
the filter test (`Some("CurrentValue.[repo_id]=repo_abc")`) append a final
`None`.

- [ ] **Step 5: Update `LarkProvider::list_tasks` call site**

In `src-tauri/src/task_provider/lark.rs:284`, change:

```rust
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, None)
            .await?;
```

to:

```rust
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, None, None)
            .await?;
```

(Task 5 will replace the second `None` with `self.view_id.as_deref()` once the
provider field is added.)

- [ ] **Step 6: Run full test suite to verify nothing broke**

```bash
cd src-tauri && cargo test --lib platform::lark_client
cd src-tauri && cargo test --lib task_provider::lark
```

Expected: existing tests pass; new `view_id` tests pass.

- [ ] **Step 7: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/platform/lark_client.rs src-tauri/src/task_provider/lark.rs
git commit -m "feat(phase-3a-3-1): bitable_list_records accepts optional view_id"
```

---

## Task 3: Add `view_id` to `BitableBinding` + schema_version bump

**Files:**

- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/persistence/lark_repo_bindings.rs`

- [ ] **Step 1: Write failing tests for legacy binding load + new field
      round-trip**

Add to `src-tauri/src/persistence/lark_repo_bindings.rs` test module:

```rust
    #[test]
    fn legacy_binding_without_view_id_loads_as_none() {
        let tmp = tempdir().unwrap();
        let path = lark_repo_bindings_file(tmp.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A schema-v1 file that predates Phase 3a-3.1 (no view_id key).
        let legacy_json = serde_json::json!({
            "schema_version": 1,
            "bindings": {
                "repo_x": {
                    "app_token": "bascntest",
                    "table_id": "tbltest",
                    "field_mapping": {
                        "title": { "field_id": "fld_t", "field_name": "Task name" }
                    },
                    "status_value_mapping": {
                        "entries": {},
                        "default_column": "todo"
                    },
                    "created_at": 1747200000,
                    "updated_at": 1747200000
                }
            }
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&legacy_json).unwrap()).unwrap();
        let loaded = load_bindings(tmp.path()).unwrap();
        let b = loaded.bindings.get("repo_x").unwrap();
        assert_eq!(b.view_id, None);
        assert_eq!(b.app_token, "bascntest");
    }

    #[test]
    fn binding_with_view_id_some_round_trips() {
        let tmp = tempdir().unwrap();
        let mut b = make_binding();
        b.view_id = Some("vw_sprint".into());
        set_binding(tmp.path(), "repo_x", b).unwrap();
        let loaded = load_bindings(tmp.path()).unwrap();
        let got = loaded.bindings.get("repo_x").unwrap();
        assert_eq!(got.view_id, Some("vw_sprint".into()));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib persistence::lark_repo_bindings::tests::legacy_binding_without_view_id
cd src-tauri && cargo test --lib persistence::lark_repo_bindings::tests::binding_with_view_id_some
```

Expected: FAIL — `BitableBinding` has no `view_id` field.

- [ ] **Step 3: Add `view_id` to `BitableBinding`**

In `src-tauri/src/state.rs` (around line 70), modify the struct:

```rust
/// One repo's binding to a Bitable: which table, plus how to map its
/// fields and status options to Ansambel's task model. Optionally scoped
/// to a single Bitable view via `view_id` so the kanban honors the
/// view's filter server-side.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    /// Optional Bitable view id. When `Some`, `LarkProvider::list_tasks`
    /// passes `view_id=...` to Lark so the view's filter applies
    /// server-side. `None` (the legacy default) fetches the full table.
    #[serde(default)]
    pub view_id: Option<String>,
    pub field_mapping: FieldMapping,
    #[serde(default)]
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test --lib persistence::lark_repo_bindings
```

Expected: all tests pass (legacy load + round-trip).

- [ ] **Step 5: Bump persistence schema_version + add no-op migrator**

In `src-tauri/src/persistence/lark_repo_bindings.rs`, change
`default_schema_version`:

```rust
fn default_schema_version() -> u32 {
    2
}
```

Just below `save_bindings`, add the migration helper:

```rust
/// Idempotent Phase 3a-3.1 migration. Reads the bindings file, and if its
/// `schema_version` is still 1, rewrites it to 2. The `view_id` field is
/// added by `#[serde(default)]` on `BitableBinding` — no per-binding
/// rewrite is required. Returns the resulting (post-migration) schema
/// version for logging.
pub(crate) fn migrate_v1_to_v2(data_dir: &Path) -> Result<u32> {
    let mut file = load_bindings(data_dir)?;
    if file.schema_version >= 2 {
        return Ok(file.schema_version);
    }
    file.schema_version = 2;
    save_bindings(data_dir, &file)?;
    Ok(2)
}
```

- [ ] **Step 6: Write tests for the migrator**

Add to the same `tests` module:

```rust
    #[test]
    fn migrate_v1_to_v2_bumps_version_and_preserves_bindings() {
        let tmp = tempdir().unwrap();
        let path = lark_repo_bindings_file(tmp.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let v1_json = serde_json::json!({
            "schema_version": 1,
            "bindings": {
                "repo_x": {
                    "app_token": "bascntest",
                    "table_id": "tbltest",
                    "field_mapping": {
                        "title": { "field_id": "fld_t", "field_name": "Task name" }
                    },
                    "status_value_mapping": { "entries": {}, "default_column": "todo" },
                    "created_at": 1, "updated_at": 1
                }
            }
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&v1_json).unwrap()).unwrap();
        assert_eq!(migrate_v1_to_v2(tmp.path()).unwrap(), 2);
        let loaded = load_bindings(tmp.path()).unwrap();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.bindings.get("repo_x").unwrap().app_token, "bascntest");
        assert_eq!(loaded.bindings.get("repo_x").unwrap().view_id, None);
    }

    #[test]
    fn migrate_v1_to_v2_is_idempotent() {
        let tmp = tempdir().unwrap();
        set_binding(tmp.path(), "repo_x", make_binding()).unwrap();
        // First call: already at v2 (set_binding writes default v2). Should
        // return 2 without rewriting.
        let v = migrate_v1_to_v2(tmp.path()).unwrap();
        assert_eq!(v, 2);
        // Second call: still 2.
        let v = migrate_v1_to_v2(tmp.path()).unwrap();
        assert_eq!(v, 2);
    }

    #[test]
    fn migrate_v1_to_v2_no_op_when_file_absent() {
        let tmp = tempdir().unwrap();
        // No bindings file yet — load returns default (v2), migrator returns
        // 2 without creating a file.
        assert_eq!(migrate_v1_to_v2(tmp.path()).unwrap(), 2);
    }

    #[test]
    fn default_schema_version_is_2() {
        let f = BindingsFile::default();
        assert_eq!(f.schema_version, 2);
    }
```

Also update the existing `load_returns_empty_when_file_absent` and
`schema_version_serialized_in_file` tests — replace `assert_eq!(..., 1)` with
`assert_eq!(..., 2)`.

- [ ] **Step 7: Run all bindings tests**

```bash
cd src-tauri && cargo test --lib persistence::lark_repo_bindings
```

Expected: all pass (existing + new).

- [ ] **Step 8: Update TS types**

In `src/lib/types.ts`, change `BitableBinding`:

```ts
export type BitableBinding = {
  app_token: string;
  table_id: string;
  view_id: string | null;
  field_mapping: FieldMapping;
  status_value_mapping: StatusValueMapping;
  created_at: number;
  updated_at: number;
};
```

Add the new view type:

```ts
export type BitableView = {
  view_id: string;
  view_name: string;
  view_type: string;
};
```

- [ ] **Step 9: Run frontend typecheck**

```bash
bun run check
```

Expected: compiler complains in places that construct a `BitableBinding`
literal. The only such place is
`src/lib/components/lark/LarkBindingWizard.svelte` (line 95) — Task 6 will fix
it. For now, satisfy the typecheck by adding `view_id: null` to the literal:

In `src/lib/components/lark/LarkBindingWizard.svelte:95`, change the binding
literal:

```ts
const binding: BitableBinding = {
  app_token: appToken.trim(),
  table_id: tableId.trim(),
  view_id: existing?.view_id ?? null,
  field_mapping: {
```

- [ ] **Step 10: Re-run typecheck**

```bash
bun run check
```

Expected: clean.

- [ ] **Step 11: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
bun run lint
```

Expected: clean.

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/persistence/lark_repo_bindings.rs \
        src/lib/types.ts src/lib/components/lark/LarkBindingWizard.svelte
git commit -m "feat(phase-3a-3-1): add view_id to BitableBinding; bump schema 1->2"
```

---

## Task 4: `list_lark_views` Tauri command

**Files:**

- Modify: `src-tauri/src/commands/lark_repo_binding.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Write failing command test**

In `src-tauri/src/commands/lark_repo_binding.rs`'s `tests` module, add a test
that goes through the command's inner helper (the inner pattern follows existing
`set_lark_repo_binding_inner`). First add the inner helper:

```rust
pub(crate) async fn list_lark_views_inner(
    app_token: &str,
    table_id: &str,
    client: Arc<crate::platform::lark_client::LarkClient>,
) -> Result<Vec<crate::platform::lark_client::BitableView>> {
    client.bitable_list_views(app_token, table_id).await
}
```

then the test (which uses wiremock so it can exercise the real client):

```rust
    #[tokio::test]
    async fn list_lark_views_inner_returns_view_list() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        // Token endpoint
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "tenant_access_token": "t", "expire": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/views"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        { "view_id": "vw_sprint", "view_name": "Current Sprint", "view_type": "grid" }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let cfg = crate::platform::lark_client::LarkConfig {
            app_id: "a".into(),
            app_secret: "s".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: server.uri(),
        };
        let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
        let views = list_lark_views_inner("bascntest", "tbltest", client)
            .await
            .unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].view_id, "vw_sprint");
    }
```

If the project's existing test pattern uses a different test helper for
`LarkConfig`, copy that pattern here for consistency. Refer to existing tests in
`src-tauri/src/commands/lark_repo_binding.rs` for the canonical shape.

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib commands::lark_repo_binding::tests::list_lark_views_inner_returns_view_list
```

Expected: FAIL — `list_lark_views_inner` not found.

- [ ] **Step 3: Add the Tauri command wrapper**

In `src-tauri/src/commands/lark_repo_binding.rs`, add (placed near
`detect_lark_schema`):

```rust
#[tauri::command]
pub async fn list_lark_views(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<crate::platform::lark_client::BitableView>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(&data_dir, &store)
        .map_err(|e| format!("global Lark credentials missing: {e}"))?;
    cfg.app_token = app_token.clone();
    cfg.table_id = table_id.clone();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    list_lark_views_inner(&app_token, &table_id, client)
        .await
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Register the command**

In `src-tauri/src/lib.rs`, in the `tauri::generate_handler!` macro invocation,
add a new line:

```rust
            crate::commands::lark_repo_binding::detect_lark_schema,
            crate::commands::lark_repo_binding::list_lark_views,
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test --lib commands::lark_repo_binding
```

Expected: all pass.

- [ ] **Step 6: Add TypeScript wrapper**

In `src/lib/ipc.ts`, inside the `lark` block (after `detectSchema`), add:

```ts
    /** List all Bitable views in the table (for the wizard view-scope dropdown). */
    listViews: (appToken: string, tableId: string): Promise<BitableView[]> =>
      invoke('list_lark_views', { appToken, tableId }),
```

And import `BitableView` at the top:

```ts
import type {
  ...,
  BitableBinding,
  BitableView,
  ProposedMapping,
} from './types';
```

- [ ] **Step 7: Run frontend check**

```bash
bun run check
```

Expected: clean.

- [ ] **Step 8: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
bun run lint
```

Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/commands/lark_repo_binding.rs src-tauri/src/lib.rs src/lib/ipc.ts
git commit -m "feat(phase-3a-3-1): list_lark_views Tauri command + ipc wrapper"
```

---

## Task 5: Thread `view_id` through `LarkProvider`

**Files:**

- Modify: `src-tauri/src/task_provider/lark.rs`

- [ ] **Step 1: Write failing tests for view_id pass-through**

Add to `src-tauri/src/task_provider/lark.rs`'s `#[cfg(test)] mod tests` block
(alongside existing `lark_provider_*` tests):

```rust
    #[tokio::test]
    async fn lark_provider_list_tasks_passes_view_id_when_set() {
        use wiremock::matchers::{method, path, query_param};
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param("view_id", "vw_sprint"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [], "has_more": false, "page_token": "" }
            })))
            .mount(&server)
            .await;
        let mut binding = sample_binding();
        binding.view_id = Some("vw_sprint".into());
        let client = Arc::new(make_client(&server.uri()));
        let provider = LarkProvider::from_binding(client, binding);
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert!(tasks.is_empty());
        // wiremock server panics if the mock didn't match — reaching this
        // assert proves the view_id query was sent.
    }

    #[tokio::test]
    async fn lark_provider_list_tasks_omits_view_id_when_none() {
        use wiremock::matchers::{method, path, query_param_is_missing};
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param_is_missing("view_id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [], "has_more": false, "page_token": "" }
            })))
            .mount(&server)
            .await;
        let binding = sample_binding(); // view_id is None
        let client = Arc::new(make_client(&server.uri()));
        let provider = LarkProvider::from_binding(client, binding);
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert!(tasks.is_empty());
    }
```

If `sample_binding` / `make_client` / `mount_token` helpers don't exist in the
test module, look at the existing tests and reuse the helpers already in place.
Adjust struct-literal field order if needed — the existing tests construct
`BitableBinding` directly.

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_list_tasks_passes_view_id_when_set
cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_list_tasks_omits_view_id_when_none
```

Expected: FAIL — `view_id` field doesn't exist on `LarkProvider`; the records
call still passes a hardcoded `None`.

- [ ] **Step 3: Add `view_id` field + thread through list_tasks**

In `src-tauri/src/task_provider/lark.rs`, modify the `LarkProvider` struct
(around line 20):

```rust
#[derive(Debug)]
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
    /// Optional Bitable view id. When `Some`, list_tasks passes
    /// `view_id=...` to Lark so the view's server-side filter applies.
    /// Writes ignore this field — they always target the table directly.
    view_id: Option<String>,
    field_mapping: FieldMapping,
    status_value_mapping: StatusValueMapping,
    primary_field_name: OnceCell<Option<String>>,
    status_options: OnceCell<Vec<BitableOption>>,
}
```

Update `LarkProvider::new`:

```rust
    pub fn new(
        client: Arc<LarkClient>,
        app_token: String,
        table_id: String,
        view_id: Option<String>,
        field_mapping: FieldMapping,
        status_value_mapping: StatusValueMapping,
    ) -> Self {
        Self {
            client,
            app_token,
            table_id,
            view_id,
            field_mapping,
            status_value_mapping,
            primary_field_name: OnceCell::new(),
            status_options: OnceCell::new(),
        }
    }
```

And `from_binding`:

```rust
    pub fn from_binding(client: Arc<LarkClient>, binding: BitableBinding) -> Self {
        Self::new(
            client,
            binding.app_token,
            binding.table_id,
            binding.view_id,
            binding.field_mapping,
            binding.status_value_mapping,
        )
    }
```

In `list_tasks` (around line 284), pass the view_id:

```rust
        let records = self
            .client
            .bitable_list_records(
                &self.app_token,
                &self.table_id,
                None,
                self.view_id.as_deref(),
            )
            .await?;
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib task_provider::lark
```

Expected: all pass (including the two new ones).

- [ ] **Step 5: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/task_provider/lark.rs
git commit -m "feat(phase-3a-3-1): LarkProvider threads view_id through list_tasks"
```

---

## Task 6: View-404 fallback + `lark-view-missing` event emission

**Files:**

- Modify: `src-tauri/src/task_provider/lark.rs`

The fallback path: when the records call returns a Lark error code that matches
"view not found" (Lark uses code `1254045 ViewNotFound` for this), the provider
retries without `view_id`, then emits a Tauri event.

The provider doesn't have an `AppHandle`; events are emitted by an event sink
trait so the provider stays testable without Tauri. The sink is injected by the
wiring in `lib.rs` (Task 7).

- [ ] **Step 1: Write failing test for fallback**

Add to `src-tauri/src/task_provider/lark.rs` test module:

```rust
    #[tokio::test]
    async fn lark_provider_falls_back_when_view_not_found() {
        use wiremock::matchers::{method, path, query_param, query_param_is_missing};
        let server = MockServer::start().await;
        mount_token(&server).await;
        // First call: with view_id → returns Lark view-not-found error.
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param("view_id", "vw_gone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1254045,
                "msg": "ViewNotFound"
            })))
            .mount(&server)
            .await;
        // Second call: without view_id → returns one record.
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param_is_missing("view_id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        { "record_id": "rec1", "fields": { "Task name": "Alpha" } }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;

        let mut binding = sample_binding();
        binding.view_id = Some("vw_gone".into());
        let client = Arc::new(make_client(&server.uri()));
        let sink = Arc::new(InMemorySink::default());
        let provider = LarkProvider::from_binding_with_sink(
            client,
            binding,
            "repo_x".into(),
            sink.clone(),
        );
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Alpha");
        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].repo_id, "repo_x");
        assert_eq!(events[0].view_id, "vw_gone");
    }

    #[tokio::test]
    async fn lark_provider_view_unrelated_error_propagates() {
        use wiremock::matchers::{method, path};
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1254000,
                "msg": "app_token invalid"
            })))
            .mount(&server)
            .await;
        let mut binding = sample_binding();
        binding.view_id = Some("vw_ok".into());
        let client = Arc::new(make_client(&server.uri()));
        let sink = Arc::new(InMemorySink::default());
        let provider = LarkProvider::from_binding_with_sink(
            client, binding, "repo_x".into(), sink.clone(),
        );
        let err = provider.list_tasks(Some("repo_x")).await.unwrap_err();
        assert!(err.to_string().contains("1254000"));
        assert!(sink.events().is_empty(), "no fallback event for non-404");
    }
```

Also add the helper sink struct + trait to the test module:

```rust
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ViewMissingEvent {
        pub repo_id: String,
        pub view_id: String,
    }

    #[derive(Default)]
    pub struct InMemorySink {
        events: std::sync::Mutex<Vec<ViewMissingEvent>>,
    }

    impl InMemorySink {
        pub fn events(&self) -> Vec<ViewMissingEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl crate::task_provider::lark::ViewMissingSink for InMemorySink {
        fn emit_view_missing(&self, repo_id: &str, view_id: &str) {
            self.events.lock().unwrap().push(ViewMissingEvent {
                repo_id: repo_id.into(),
                view_id: view_id.into(),
            });
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_falls_back_when_view_not_found
```

Expected: FAIL — `ViewMissingSink` trait + `from_binding_with_sink` don't exist.

- [ ] **Step 3: Add `ViewMissingSink` trait + `repo_id` + sink fields**

In `src-tauri/src/task_provider/lark.rs`, near the top of the file (above
`LarkProvider` struct):

```rust
/// Notification sink for "the bound Bitable view no longer exists".
/// Implemented by the Tauri-emitter wrapper in `lib.rs` so the provider
/// stays testable without an `AppHandle`. The default `NoopSink` is used
/// in unit tests that don't care about banner emission.
pub trait ViewMissingSink: Send + Sync + std::fmt::Debug {
    fn emit_view_missing(&self, repo_id: &str, view_id: &str);
}

#[derive(Debug, Default)]
pub struct NoopSink;
impl ViewMissingSink for NoopSink {
    fn emit_view_missing(&self, _repo_id: &str, _view_id: &str) {}
}
```

Modify `LarkProvider`:

```rust
#[derive(Debug)]
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
    view_id: Option<String>,
    /// Repo id this provider serves — used for view-missing event
    /// payload only. Set by `from_binding_with_sink`; defaults to empty
    /// string when constructed via `from_binding` (no events emitted).
    repo_id: String,
    /// Receives "view missing" notifications when a 404 falls back.
    /// Defaults to NoopSink for binding-less code paths and tests that
    /// don't care.
    sink: Arc<dyn ViewMissingSink>,
    field_mapping: FieldMapping,
    status_value_mapping: StatusValueMapping,
    primary_field_name: OnceCell<Option<String>>,
    status_options: OnceCell<Vec<BitableOption>>,
}
```

Update constructors:

```rust
    pub fn new(
        client: Arc<LarkClient>,
        app_token: String,
        table_id: String,
        view_id: Option<String>,
        field_mapping: FieldMapping,
        status_value_mapping: StatusValueMapping,
    ) -> Self {
        Self {
            client,
            app_token,
            table_id,
            view_id,
            repo_id: String::new(),
            sink: Arc::new(NoopSink),
            field_mapping,
            status_value_mapping,
            primary_field_name: OnceCell::new(),
            status_options: OnceCell::new(),
        }
    }

    pub fn from_binding(client: Arc<LarkClient>, binding: BitableBinding) -> Self {
        Self::new(
            client,
            binding.app_token,
            binding.table_id,
            binding.view_id,
            binding.field_mapping,
            binding.status_value_mapping,
        )
    }

    /// Production constructor that attaches a `repo_id` + sink so the
    /// view-missing event fires when fallback kicks in. The two-arg
    /// `from_binding` is kept for tests and other call sites that don't
    /// need event emission.
    pub fn from_binding_with_sink(
        client: Arc<LarkClient>,
        binding: BitableBinding,
        repo_id: String,
        sink: Arc<dyn ViewMissingSink>,
    ) -> Self {
        let mut p = Self::from_binding(client, binding);
        p.repo_id = repo_id;
        p.sink = sink;
        p
    }
```

- [ ] **Step 4: Add fallback logic to `list_tasks`**

Replace `list_tasks`'s record fetch with:

```rust
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>> {
        let records = match self
            .client
            .bitable_list_records(
                &self.app_token,
                &self.table_id,
                None,
                self.view_id.as_deref(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) if is_view_missing_error(&e) && self.view_id.is_some() => {
                let missing_view = self.view_id.clone().unwrap_or_default();
                tracing::warn!(
                    repo_id = %self.repo_id,
                    view_id = %missing_view,
                    "Lark view missing; falling back to all records"
                );
                self.sink.emit_view_missing(&self.repo_id, &missing_view);
                self.client
                    .bitable_list_records(&self.app_token, &self.table_id, None, None)
                    .await?
            }
            Err(e) => return Err(e),
        };
        let primary = self.primary_field_name().await;
        // ... rest of list_tasks unchanged ...
```

(Keep the rest of `list_tasks` — `let total`, `let mut skipped`, the filter_map,
the sort — exactly as it is.)

And add the sentinel matcher near the bottom of the file (above `#[cfg(test)]`):

```rust
/// Returns true when the given error came from a Lark Bitable
/// "view not found" response (code 1254045).
fn is_view_missing_error(err: &crate::error::AppError) -> bool {
    err.to_string().contains("1254045")
}
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_falls_back_when_view_not_found
cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_view_unrelated_error_propagates
cd src-tauri && cargo test --lib task_provider::lark
```

Expected: all pass.

- [ ] **Step 6: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/task_provider/lark.rs
git commit -m "feat(phase-3a-3-1): LarkProvider falls back when view is missing"
```

---

## Task 7: Wire view-missing sink into `lib.rs`

**Files:**

- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add a `TauriViewMissingSink` adapter**

In `src-tauri/src/lib.rs`, near the top (below `use` statements but above the
existing helpers), add:

```rust
#[derive(Debug, Clone)]
struct TauriViewMissingSink {
    app_handle: tauri::AppHandle,
}

impl crate::task_provider::lark::ViewMissingSink for TauriViewMissingSink {
    fn emit_view_missing(&self, repo_id: &str, view_id: &str) {
        use tauri::Manager;
        if let Some(window) = self.app_handle.get_webview_window("main") {
            let _ = window.emit(
                "lark-view-missing",
                serde_json::json!({
                    "repo_id": repo_id,
                    "view_id": view_id,
                }),
            );
        }
    }
}
```

- [ ] **Step 2: Use the sink-aware constructor**

In the existing provider-init loop in `setup()` (around lines 118-152), replace
`LarkProvider::from_binding(client, binding.clone())` with
`LarkProvider::from_binding_with_sink(...)`:

```rust
                        let sink: std::sync::Arc<
                            dyn crate::task_provider::lark::ViewMissingSink,
                        > = std::sync::Arc::new(TauriViewMissingSink {
                            app_handle: app_handle.clone(),
                        });
                        let provider: std::sync::Arc<dyn crate::task_provider::TaskProvider> =
                            std::sync::Arc::new(
                                crate::task_provider::lark::LarkProvider::from_binding_with_sink(
                                    client,
                                    binding.clone(),
                                    repo_id.clone(),
                                    sink,
                                ),
                            );
```

You'll need to capture `app_handle` into the spawned task. Add this just before
`tauri::async_runtime::spawn`:

```rust
                let app_handle = app.handle().clone();
```

and ensure the closure captures it via `move`.

- [ ] **Step 3: Also update the runtime path in
      `commands/lark_repo_binding.rs`**

In `set_lark_repo_binding_inner` (around line 71), the provider is re-created
when a binding is saved. That code currently passes the `TaskProviderHandle` but
not an `AppHandle`. Take the path of least resistance: add an `AppHandle`
parameter to `set_lark_repo_binding_inner`, pass it from the existing command
wrapper, and construct the sink the same way as in `lib.rs`.

Update `set_lark_repo_binding`:

```rust
#[tauri::command]
pub async fn set_lark_repo_binding(
    repo_id: String,
    binding: BitableBinding,
    app_handle: tauri::AppHandle,
    provider_handle: State<'_, TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    set_lark_repo_binding_inner(
        &repo_id,
        binding,
        &data_dir,
        provider_handle.inner().clone(),
        Some(app_handle),
    )
    .await
    .map_err(|e| e.to_string())
}

pub(crate) async fn set_lark_repo_binding_inner(
    repo_id: &str,
    mut binding: BitableBinding,
    data_dir: &std::path::Path,
    handle: TaskProviderHandle,
    app_handle: Option<tauri::AppHandle>,
) -> Result<()> {
    if binding.field_mapping.title.field_id.is_empty() {
        return Err(AppError::InvalidState("title field is required".into()));
    }
    let now = now_unix();
    if binding.created_at == 0 {
        binding.created_at = now;
    }
    binding.updated_at = now;

    crate::persistence::lark_repo_bindings::set_binding(data_dir, repo_id, binding.clone())?;

    let store = crate::commands::lark_auth::KeyringStore;
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, &store)
        .map_err(|e| AppError::InvalidState(format!("global Lark credentials missing: {e}")))?;
    cfg.app_token = binding.app_token.clone();
    cfg.table_id = binding.table_id.clone();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    let provider: Arc<dyn crate::task_provider::TaskProvider> = if let Some(h) = app_handle {
        let sink: Arc<dyn crate::task_provider::lark::ViewMissingSink> =
            Arc::new(crate::TauriViewMissingSink { app_handle: h });
        Arc::new(crate::task_provider::lark::LarkProvider::from_binding_with_sink(
            client,
            binding,
            repo_id.to_string(),
            sink,
        ))
    } else {
        Arc::new(crate::task_provider::lark::LarkProvider::from_binding(client, binding))
    };

    {
        let mut guard = handle.write().await;
        guard.insert(repo_id.to_string(), provider);
    }
    Ok(())
}
```

(`TauriViewMissingSink` is `pub(crate)` in lib.rs so this `crate::` path works.
Make sure that the struct definition in `lib.rs` uses
`pub(crate) struct TauriViewMissingSink` rather than the private form shown in
Step 1.)

- [ ] **Step 4: Update any existing test call sites**

If `set_lark_repo_binding_inner` is called directly in tests with the old 4-arg
signature, append `None` as the new `app_handle` argument. Find them with:

```bash
cd src-tauri && grep -rn "set_lark_repo_binding_inner" src/
```

- [ ] **Step 5: Run migration of existing v1 file on startup**

In `setup()` (right after `let data_dir = ...` but before the bindings load),
call the migrator from Task 3:

```rust
            // Phase 3a-3.1 schema migration (v1 → v2). No-op for fresh
            // installs; bumps the version + makes future migrations
            // observable. Failure is non-fatal — log + continue.
            if let Err(e) = crate::persistence::lark_repo_bindings::migrate_v1_to_v2(&data_dir) {
                tracing::warn!(error = %e, "lark_repo_bindings v1->v2 migration failed");
            }
```

- [ ] **Step 6: Run the full lib test suite**

```bash
cd src-tauri && cargo test --lib
```

Expected: all pass.

- [ ] **Step 7: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands/lark_repo_binding.rs
git commit -m "feat(phase-3a-3-1): wire view-missing sink + run v1->v2 migration on startup"
```

---

## Task 8: Wizard Step 1.5 — view picker

**Files:**

- Modify: `src/lib/components/lark/LarkBindingWizard.svelte`
- Modify: `src/lib/components/lark/LarkBindingWizard.test.ts`

- [ ] **Step 1: Write failing component tests**

In `src/lib/components/lark/LarkBindingWizard.test.ts`, add (or extend where
similar setup exists):

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import LarkBindingWizard from './LarkBindingWizard.svelte';
import { api } from '$lib/ipc';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      detectSchema: vi.fn(),
      listViews: vi.fn(),
    },
  },
}));

describe('LarkBindingWizard step 1.5 (view picker)', () => {
  beforeEach(() => {
    vi.mocked(api.lark.detectSchema).mockResolvedValue({
      fields: [
        {
          field_id: 'fld_t',
          field_name: 'Task name',
          type: 1,
          is_primary: true,
        },
      ],
      suggested: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
      },
      status_options: null,
      suggested_status_values: { entries: {}, default_column: 'todo' },
    });
    vi.mocked(api.lark.listViews).mockResolvedValue([
      { view_id: 'vw_grid', view_name: 'Grid view', view_type: 'grid' },
      { view_id: 'vw_sprint', view_name: 'Current Sprint', view_type: 'grid' },
    ]);
  });

  it('renders dropdown with "All records (no view filter)" first', async () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() =>
      expect(screen.getByTestId('wizard-step-1-5')).toBeInTheDocument()
    );
    const select = screen.getByTestId(
      'wizard-view-select'
    ) as HTMLSelectElement;
    expect(select.options[0].value).toBe('');
    expect(select.options[0].textContent).toContain('All records');
    expect(select.options[1].value).toBe('vw_grid');
    expect(select.options[2].value).toBe('vw_sprint');
    // Default selection = All records
    expect(select.value).toBe('');
  });

  it('selecting a view stores its id and Continue advances to step 2', async () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    const select = await screen.findByTestId('wizard-view-select');
    await fireEvent.change(select, { target: { value: 'vw_sprint' } });
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    expect(screen.getByTestId('wizard-step-2')).toBeInTheDocument();
  });

  it('proceeds even when view list is empty', async () => {
    vi.mocked(api.lark.listViews).mockResolvedValue([]);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    const select = await screen.findByTestId('wizard-view-select');
    expect((select as HTMLSelectElement).options.length).toBe(1);
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    expect(screen.getByTestId('wizard-step-2')).toBeInTheDocument();
  });

  it('pre-selects view_id when editing existing binding and preserves it through Save', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: {
          app_token: 'bascntest',
          table_id: 'tbltest',
          view_id: 'vw_sprint',
          field_mapping: {
            title: { field_id: 'fld_t', field_name: 'Task name' },
            description: null,
            status: null,
            order: null,
          },
          status_value_mapping: { entries: {}, default_column: 'todo' },
          created_at: 1747200000,
          updated_at: 1747200000,
        },
        onSave,
        onCancel: vi.fn(),
      },
    });
    // Editing flow opens at step 2. Save immediately and verify view_id
    // round-trips through the saved binding.
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.view_id).toBe('vw_sprint');
  });
});
```

(The last test demonstrates the requirement; adapt the assertion to whatever way
Step 1.5 surfaces in the editing flow once implemented.)

- [ ] **Step 2: Run tests to verify they fail**

```bash
bun run test src/lib/components/lark/LarkBindingWizard.test.ts
```

Expected: FAIL — Step 1.5 + view dropdown don't exist.

- [ ] **Step 3: Implement Step 1.5**

Edit `src/lib/components/lark/LarkBindingWizard.svelte`. Replace the
`Step = 1 | 2 | 3` type with `Step = 1 | 1.5 | 2 | 3` (use `1.5` as the literal
— Svelte/TS accepts numeric union members). Top of `<script>`:

```ts
type Step = 1 | 1.5 | 2 | 3;
// svelte-ignore state_referenced_locally
let step = $state<Step>(existing ? 2 : 1);
```

Add view state:

```ts
let views = $state<BitableView[]>([]);
let viewId = $state<string>(existing?.view_id ?? '');
let loadingViews = $state(false);
```

Import the new type at the top of `<script>`:

```ts
import type {
  BitableBinding,
  BitableView,
  FieldMapping,
  ProposedMapping,
  StatusValueMapping,
  KanbanColumnLiteral,
} from '$lib/types';
```

Modify `handleDetect()` to fetch views in parallel and advance to step 1.5:

```ts
async function handleDetect() {
  if (!appToken.trim() || !tableId.trim()) return;
  detecting = true;
  detectError = null;
  loadingViews = true;
  try {
    const [p, vs] = await Promise.all([
      api.lark.detectSchema(appToken.trim(), tableId.trim()),
      api.lark.listViews(appToken.trim(), tableId.trim()),
    ]);
    proposal = p;
    views = vs;
    titleFieldId = p.suggested.title.field_id;
    descFieldId = p.suggested.description?.field_id ?? '';
    statusFieldId = p.suggested.status?.field_id ?? '';
    orderFieldId = p.suggested.order?.field_id ?? '';
    valueMap = { ...p.suggested_status_values.entries };
    defaultColumn = p.suggested_status_values.default_column;
    step = 1.5;
  } catch (err) {
    detectError = err instanceof Error ? err.message : String(err);
  } finally {
    detecting = false;
    loadingViews = false;
  }
}
```

Modify `handleSave` to include `view_id`:

```ts
    const binding: BitableBinding = {
      app_token: appToken.trim(),
      table_id: tableId.trim(),
      view_id: viewId.trim() === '' ? null : viewId.trim(),
      field_mapping: {
        ...
```

Add the Step 1.5 section in the template, between Step 1 and Step 2 (right
before `{:else if step === 2}`):

```svelte
  {:else if step === 1.5}
    <section class="flex flex-col gap-3" data-testid="wizard-step-1-5">
      <h3 class="text-xs font-semibold text-[var(--text-primary)]">
        Scope this binding (1.5 of 3)
      </h3>
      <label class="flex flex-col gap-1 text-[11px]">
        View
        <select
          bind:value={viewId}
          class={selectClass}
          data-testid="wizard-view-select"
        >
          <option value="">All records (no view filter)</option>
          {#each views as v (v.view_id)}
            <option value={v.view_id}>{v.view_name} ({v.view_type})</option>
          {/each}
        </select>
      </label>
      <p class="text-[11px] text-[var(--text-muted)]">
        When a view is selected, Ansambel honors that view's filter from Lark.
      </p>
      <div class="flex gap-2 justify-end">
        <button
          type="button"
          onclick={() => (step = 1)}
          class="px-2 py-1 text-xs rounded border border-[var(--border-light)]">← Back</button
        >
        <button
          type="button"
          onclick={() => (step = 2)}
          class="px-2 py-1 text-xs rounded bg-[var(--accent)] text-white"
          data-testid="wizard-view-continue"
        >
          Continue →
        </button>
      </div>
    </section>
```

Also update the header counters of step 2 (`(2 of 3)` → `(2 of 4)`) and step 3
(`(3 of 3)` → `(3 of 4)`) to reflect the 4-step total — or leave them as is
since step numbers are user-facing context. **Recommendation:** change to
`(2 of 4)` and `(3 of 4)` for honesty.

- [ ] **Step 4: Run tests**

```bash
bun run test src/lib/components/lark/LarkBindingWizard.test.ts
```

Expected: all pass.

- [ ] **Step 5: Lint + check**

```bash
bun run check && bun run lint
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/lark/LarkBindingWizard.svelte \
        src/lib/components/lark/LarkBindingWizard.test.ts
git commit -m "feat(phase-3a-3-1): wizard step 1.5 view picker"
```

---

## Task 9: Settings dialog — "View:" row + Change view button

**Files:**

- Modify: `src/lib/components/repo/RepoSettingsDialog.svelte`
- Modify: `src/lib/components/repo/RepoSettingsDialog.test.ts`

- [ ] **Step 1: Write failing test**

In `src/lib/components/repo/RepoSettingsDialog.test.ts`, add:

```ts
it('shows view row with view name when bound', async () => {
  larkBindings.bindings.set('repo_x', {
    app_token: 'bascntest',
    table_id: 'tbltest',
    view_id: 'vw_sprint',
    field_mapping: {
      title: { field_id: 'fld_t', field_name: 'Task name' },
      description: null,
      status: null,
      order: null,
    },
    status_value_mapping: { entries: {}, default_column: 'todo' },
    created_at: 0,
    updated_at: 0,
  });
  render(RepoSettingsDialog, {
    props: {
      repoId: 'repo_x',
      repoName: 'Repo X',
      open: true,
      onClose: vi.fn(),
    },
  });
  expect(screen.getByTestId('binding-view-row')).toHaveTextContent('vw_sprint');
});

it('shows "All records" when no view bound', async () => {
  larkBindings.bindings.set('repo_x', {
    app_token: 'bascntest',
    table_id: 'tbltest',
    view_id: null,
    field_mapping: {
      title: { field_id: 'fld_t', field_name: 'Task name' },
      description: null,
      status: null,
      order: null,
    },
    status_value_mapping: { entries: {}, default_column: 'todo' },
    created_at: 0,
    updated_at: 0,
  });
  render(RepoSettingsDialog, {
    props: {
      repoId: 'repo_x',
      repoName: 'Repo X',
      open: true,
      onClose: vi.fn(),
    },
  });
  expect(screen.getByTestId('binding-view-row')).toHaveTextContent(
    'All records'
  );
});
```

- [ ] **Step 2: Run test**

```bash
bun run test src/lib/components/repo/RepoSettingsDialog.test.ts
```

Expected: FAIL — `binding-view-row` testid not present.

- [ ] **Step 3: Add the View row**

In `src/lib/components/repo/RepoSettingsDialog.svelte`, inside the existing
`{#if isConnected && binding}` summary block (around line 104), add this just
below the `binding.app_token...table_id` line:

```svelte
<div
  class="text-[var(--text-muted)] text-[11px]"
  data-testid="binding-view-row"
>
  View: {binding.view_id ?? 'All records (no filter)'}
</div>
```

(The "Change view" button reuses the existing "Edit mapping" button — same
wizard entry. No new button added.)

- [ ] **Step 4: Run tests**

```bash
bun run test src/lib/components/repo/RepoSettingsDialog.test.ts
```

Expected: pass.

- [ ] **Step 5: Lint + check**

```bash
bun run check && bun run lint
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/repo/RepoSettingsDialog.svelte \
        src/lib/components/repo/RepoSettingsDialog.test.ts
git commit -m "feat(phase-3a-3-1): RepoSettingsDialog shows bound view"
```

---

## Task 10: View-missing banner store + listener

**Files:**

- Create: `src/lib/stores/view-missing.svelte.ts`
- Create: `src/lib/stores/view-missing.svelte.test.ts`

- [ ] **Step 1: Write failing tests**

Create `src/lib/stores/view-missing.svelte.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { viewMissing } from './view-missing.svelte';

describe('viewMissing store', () => {
  beforeEach(() => {
    viewMissing.clear();
  });

  it('starts empty', () => {
    expect(viewMissing.entries.size).toBe(0);
  });

  it('records a missing view by repo_id', () => {
    viewMissing.report('repo_x', 'vw_gone');
    expect(viewMissing.entries.get('repo_x')).toBe('vw_gone');
  });

  it('dismiss removes one repo entry', () => {
    viewMissing.report('repo_x', 'vw_gone');
    viewMissing.report('repo_y', 'vw_other');
    viewMissing.dismiss('repo_x');
    expect(viewMissing.entries.has('repo_x')).toBe(false);
    expect(viewMissing.entries.get('repo_y')).toBe('vw_other');
  });

  it('clear wipes all entries', () => {
    viewMissing.report('repo_x', 'vw_gone');
    viewMissing.clear();
    expect(viewMissing.entries.size).toBe(0);
  });
});
```

- [ ] **Step 2: Run test**

```bash
bun run test src/lib/stores/view-missing.svelte.test.ts
```

Expected: FAIL — file doesn't exist.

- [ ] **Step 3: Implement the store**

Create `src/lib/stores/view-missing.svelte.ts`:

```ts
import { SvelteMap } from 'svelte/reactivity';

/**
 * Tracks per-repo Lark view-missing banners. Populated by the
 * `lark-view-missing` Tauri event listener; consumed by `TitleBar` to
 * render a non-blocking banner. Session-local — no persistence.
 */
export class ViewMissingStore {
  readonly entries = new SvelteMap<string, string>();

  report(repoId: string, viewId: string): void {
    this.entries.set(repoId, viewId);
  }

  dismiss(repoId: string): void {
    this.entries.delete(repoId);
  }

  clear(): void {
    this.entries.clear();
  }
}

export const viewMissing = new ViewMissingStore();
```

- [ ] **Step 4: Run test**

```bash
bun run test src/lib/stores/view-missing.svelte.test.ts
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/view-missing.svelte.ts src/lib/stores/view-missing.svelte.test.ts
git commit -m "feat(phase-3a-3-1): view-missing svelte store"
```

---

## Task 11: TitleBar banner + Tauri event listener

**Files:**

- Modify: `src/lib/components/TitleBar.svelte`
- Modify: `src/lib/components/TitleBar.test.ts`

- [ ] **Step 1: Write failing test**

In `src/lib/components/TitleBar.test.ts`, add:

```ts
it('shows banner when viewMissing store has entry for selected repo', async () => {
  viewMissing.clear();
  viewMissing.report('repo_x', 'vw_gone');
  // Use the same `render(TitleBar, { props: ... })` setup as an
  // adjacent test in this file. Look at the most recent passing test
  // for the prop shape (selectedRepo, workspaces store, etc.) and
  // mirror it here.
  renderTitleBarForRepo('repo_x');
  expect(screen.getByTestId('view-missing-banner')).toHaveTextContent(
    'vw_gone'
  );
});

it('dismiss removes the banner', async () => {
  viewMissing.report('repo_x', 'vw_gone');
  renderTitleBarForRepo('repo_x');
  await fireEvent.click(screen.getByTestId('view-missing-dismiss'));
  expect(screen.queryByTestId('view-missing-banner')).not.toBeInTheDocument();
});

it('hides banner when selected repo has no missing view', async () => {
  viewMissing.clear();
  viewMissing.report('repo_other', 'vw_gone');
  renderTitleBarForRepo('repo_x');
  expect(screen.queryByTestId('view-missing-banner')).not.toBeInTheDocument();
});
```

(Adapt the `props` to match the existing TitleBar test setup — copy the shape
from an adjacent existing test in the same file.)

Define `renderTitleBarForRepo(repoId)` at the top of the test file (or reuse an
existing helper if one already wraps TitleBar setup — copy its prop list and
substitute `repoId`).

- [ ] **Step 2: Run test**

```bash
bun run test src/lib/components/TitleBar.test.ts
```

Expected: FAIL.

- [ ] **Step 3: Wire the Tauri listener + banner**

In `src/lib/components/TitleBar.svelte`:

Add to the `<script>`:

```ts
import { listen } from '@tauri-apps/api/event';
import { viewMissing } from '$lib/stores/view-missing.svelte';
import { onDestroy } from 'svelte';

let unlistenViewMissing: (() => void) | null = null;
$effect(() => {
  listen<{ repo_id: string; view_id: string }>('lark-view-missing', (event) =>
    viewMissing.report(event.payload.repo_id, event.payload.view_id)
  ).then((un) => (unlistenViewMissing = un));
  return () => {
    unlistenViewMissing?.();
    unlistenViewMissing = null;
  };
});

const missingViewForSelected = $derived(
  selectedRepo ? (viewMissing.entries.get(selectedRepo.id) ?? null) : null
);
```

Add the banner markup near the top of the rendered title bar (above the
repo-name row):

```svelte
{#if missingViewForSelected}
  <div
    class="flex items-center gap-2 px-3 py-1.5 bg-[var(--bg-warning,#3b3000)] border-b border-[var(--border-warning,#a87900)] text-[11px] text-[var(--text-warning,#fcd34d)]"
    data-testid="view-missing-banner"
  >
    <span>
      The Lark view bound to {selectedRepo?.name ?? 'this repo'} no longer exists
      (id: {missingViewForSelected}). Showing all records.
    </span>
    <button
      type="button"
      onclick={() => (repoSettingsOpen = true)}
      class="px-1.5 py-0.5 rounded border border-[var(--border-warning,#a87900)]"
      data-testid="view-missing-reconfigure"
    >
      Reconfigure
    </button>
    <button
      type="button"
      onclick={() => selectedRepo && viewMissing.dismiss(selectedRepo.id)}
      class="px-1.5 py-0.5 rounded border border-[var(--border-warning,#a87900)]"
      data-testid="view-missing-dismiss"
    >
      Dismiss
    </button>
  </div>
{/if}
```

- [ ] **Step 4: Run tests**

```bash
bun run test src/lib/components/TitleBar.test.ts
```

Expected: pass. If the listener-mock causes issues in JSDOM, mock
`@tauri-apps/api/event` at the top of the test file:

```ts
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
```

- [ ] **Step 5: Lint + check**

```bash
bun run check && bun run lint
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/TitleBar.svelte src/lib/components/TitleBar.test.ts
git commit -m "feat(phase-3a-3-1): view-missing banner + tauri event listener"
```

---

## Task 12: Wiremock integration tests in `lib.rs`

**Files:**

- Modify: `src-tauri/src/lib.rs` (add to existing `migration_tests` /
  integration test block)

- [ ] **Step 1: Write failing integration test for end-to-end view scoping**

In `src-tauri/src/lib.rs`'s existing test module (find via
`grep -n "mod migration_tests\|mod tests" src-tauri/src/lib.rs`), add:

```rust
    #[tokio::test]
    async fn wizard_save_with_view_id_persists_and_provider_uses_it() {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let tmp = tempdir().unwrap();
        let server = MockServer::start().await;
        // Token endpoint
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "tenant_access_token": "t", "expire": 3600
            })))
            .mount(&server)
            .await;
        // Records endpoint MUST receive view_id=vw_sprint to satisfy this mock
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param("view_id", "vw_sprint"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        { "record_id": "rec1", "fields": { "Task name": "Sprint task" } }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        // Write the LarkConfig file so load_lark_config_inner succeeds.
        // Match what the existing tests in lib.rs do for this setup.
        // (Reuse the helper if one already exists, or inline the file write.)
        // After setup, save a binding with view_id and exercise list_tasks
        // via LarkProvider directly (skip the full Tauri command path —
        // wiremock proves the query param flows through).
        let binding = crate::state::BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            view_id: Some("vw_sprint".into()),
            field_mapping: crate::state::FieldMapping {
                title: crate::state::FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "Task name".into(),
                },
                description: None, status: None, order: None,
            },
            status_value_mapping: crate::state::StatusValueMapping::default(),
            created_at: 0, updated_at: 0,
        };
        let cfg = crate::platform::lark_client::LarkConfig {
            app_id: "a".into(),
            app_secret: "s".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: server.uri(),
        };
        let client = std::sync::Arc::new(
            crate::platform::lark_client::LarkClient::new(cfg),
        );
        let provider = crate::task_provider::lark::LarkProvider::from_binding(client, binding);
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Sprint task");
    }

    #[tokio::test]
    async fn view_deleted_after_binding_falls_back_and_continues() {
        use wiremock::matchers::{method, path, query_param, query_param_is_missing};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "tenant_access_token": "t", "expire": 3600
            })))
            .mount(&server)
            .await;
        // First: view-not-found
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param("view_id", "vw_gone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1254045, "msg": "ViewNotFound"
            })))
            .mount(&server)
            .await;
        // Fallback: unfiltered
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .and(query_param_is_missing("view_id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        { "record_id": "rec1", "fields": { "Task name": "All tasks" } }
                    ],
                    "has_more": false, "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let cfg = crate::platform::lark_client::LarkConfig {
            app_id: "a".into(), app_secret: "s".into(),
            app_token: "bascntest".into(), table_id: "tbltest".into(),
            base_url: server.uri(),
        };
        let client = std::sync::Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
        let binding = crate::state::BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            view_id: Some("vw_gone".into()),
            field_mapping: crate::state::FieldMapping {
                title: crate::state::FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "Task name".into(),
                },
                description: None, status: None, order: None,
            },
            status_value_mapping: crate::state::StatusValueMapping::default(),
            created_at: 0, updated_at: 0,
        };
        #[derive(Debug, Default)]
        struct CountingSink { count: std::sync::Mutex<usize> }
        impl crate::task_provider::lark::ViewMissingSink for CountingSink {
            fn emit_view_missing(&self, _: &str, _: &str) {
                *self.count.lock().unwrap() += 1;
            }
        }
        let sink = std::sync::Arc::new(CountingSink::default());
        let provider = crate::task_provider::lark::LarkProvider::from_binding_with_sink(
            client, binding, "repo_x".into(), sink.clone(),
        );
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(*sink.count.lock().unwrap(), 1);
    }
```

- [ ] **Step 2: Run tests**

```bash
cd src-tauri && cargo test --lib lib::wizard_save_with_view_id_persists_and_provider_uses_it
cd src-tauri && cargo test --lib lib::view_deleted_after_binding_falls_back_and_continues
```

(If those test names live in a submodule like `migration_tests`, adjust the test
path accordingly.)

Expected: pass.

- [ ] **Step 3: Run full suite**

```bash
cd src-tauri && cargo test --lib
```

Expected: pass.

- [ ] **Step 4: Lint + fmt**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --lib --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "test(phase-3a-3-1): wiremock integration tests for view_id flow"
```

---

## Task 13: E2E Playwright tests

**Files:**

- Create: `tests/e2e/lark-binding-view-scope.spec.ts`

- [ ] **Step 1: Check existing E2E infrastructure**

```bash
ls tests/e2e/
cat tests/e2e/lark-binding.spec.ts 2>/dev/null | head -40
```

Adapt the new spec to the same pattern (Playwright config,
`ANSAMBEL_MOCK_LARK=1` env, helpers in `tests/e2e/helpers/`). If no Lark mock
fixture exists, copy the existing `ansambel_mock_claude` pattern and extend it
for Lark.

- [ ] **Step 2: Write the E2E spec**

Create `tests/e2e/lark-binding-view-scope.spec.ts`:

```ts
import { test, expect } from '@playwright/test';
import { launchAnsambel, withMockLark } from './helpers';

test.describe('Phase 3a-3.1 view-aware Lark binding', () => {
  test('step 1.5 lists views and defaults to all records', async ({
    context,
  }) => {
    const { mock } = await withMockLark({
      views: [
        { view_id: 'vw_grid', view_name: 'Grid view', view_type: 'grid' },
        {
          view_id: 'vw_sprint',
          view_name: 'Current Sprint',
          view_type: 'grid',
        },
      ],
      records: { vw_sprint: sprintRecords(), all: allRecords() },
    });
    const page = await launchAnsambel(context);
    await openRepoSettings(page);
    await page.getByTestId('connect-binding').click();
    await page.getByTestId('wizard-app-token').fill('bascntest');
    await page.getByTestId('wizard-table-id').fill('tbltest');
    await page.getByTestId('wizard-detect').click();
    await expect(page.getByTestId('wizard-step-1-5')).toBeVisible();
    const select = page.getByTestId('wizard-view-select');
    await expect(select).toHaveValue(''); // default = All records
    const options = await select.locator('option').allTextContents();
    expect(options[0]).toContain('All records');
    expect(options[1]).toContain('Grid view');
    expect(options[2]).toContain('Current Sprint');
  });

  test('selecting a view scopes kanban after save', async ({ context }) => {
    const { mock } = await withMockLark({
      views: [
        {
          view_id: 'vw_sprint',
          view_name: 'Current Sprint',
          view_type: 'grid',
        },
      ],
      records: { vw_sprint: makeRecords(11), all: makeRecords(667) },
    });
    const page = await launchAnsambel(context);
    await openRepoSettings(page);
    await page.getByTestId('connect-binding').click();
    await page.getByTestId('wizard-app-token').fill('bascntest');
    await page.getByTestId('wizard-table-id').fill('tbltest');
    await page.getByTestId('wizard-detect').click();
    await page.getByTestId('wizard-view-select').selectOption('vw_sprint');
    await page.getByTestId('wizard-view-continue').click();
    await page.getByTestId('wizard-continue').click();
    // ... complete status mapping if applicable, then Save
    await page.getByTestId('wizard-save').click();
    // After save: kanban shows only 11 cards
    await expect(page.locator('[data-testid="kanban-card"]')).toHaveCount(11);
  });

  test('change view from settings reopens wizard at correct step', async ({
    context,
  }) => {
    // Pre-populate a binding with view_id; then click Edit; verify Step 1.5 reachable
    // and current view pre-selected.
    // (See helpers/ for the binding-fixture pattern.)
  });

  test('view deleted in Lark shows banner and falls back', async ({
    context,
  }) => {
    const mock = await withMockLark({
      views: [],
      records: { vw_gone: { error: 1254045 }, all: makeRecords(50) },
      preBound: { repo_id: 'repo_x', view_id: 'vw_gone' },
    });
    const page = await launchAnsambel(context);
    await page.getByTestId('refresh-tasks').click();
    await expect(page.getByTestId('view-missing-banner')).toBeVisible();
    await expect(page.locator('[data-testid="kanban-card"]')).toHaveCount(50);
    await page.getByTestId('view-missing-reconfigure').click();
    await expect(page.getByTestId('wizard-step-1-5')).toBeVisible();
  });

  test('empty view list still lets user proceed', async ({ context }) => {
    await withMockLark({ views: [], records: { all: makeRecords(5) } });
    const page = await launchAnsambel(context);
    await openRepoSettings(page);
    await page.getByTestId('connect-binding').click();
    await page.getByTestId('wizard-app-token').fill('bascntest');
    await page.getByTestId('wizard-table-id').fill('tbltest');
    await page.getByTestId('wizard-detect').click();
    await expect(page.getByTestId('wizard-view-select')).toHaveValue('');
    await page.getByTestId('wizard-view-continue').click();
    await expect(page.getByTestId('wizard-step-2')).toBeVisible();
  });
});

// ---- helpers used in these scenarios ----
function sprintRecords() {
  return makeRecords(11);
}
function allRecords() {
  return makeRecords(667);
}
function makeRecords(n: number) {
  return Array.from({ length: n }, (_, i) => ({
    record_id: `rec${i}`,
    fields: { 'Task name': `Task ${i}` },
  }));
}
async function openRepoSettings(page) {
  await page.getByTestId('repo-settings-gear').click();
}
```

The helper signatures (`launchAnsambel`, `withMockLark`) must match what exists
in `tests/e2e/helpers/`. If `withMockLark` doesn't exist, add it next to the
existing mock helpers — extending `ANSAMBEL_MOCK_LARK=1` with the JSON fixture
file pattern already used elsewhere.

- [ ] **Step 3: Run the E2E suite**

```bash
bun run test:e2e -- lark-binding-view-scope
```

Expected: 5/5 pass.

- [ ] **Step 4: Lint + check**

```bash
bun run check && bun run lint
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add tests/e2e/lark-binding-view-scope.spec.ts tests/e2e/helpers/
git commit -m "test(phase-3a-3-1): e2e playwright scenarios for view-aware binding"
```

---

## Task 14: Coverage check + final validation

**Files:**

- All Phase 3a-3.1 files

- [ ] **Step 1: Run full test suites**

```bash
cd src-tauri && cargo test --lib
bun run test
bun run test:e2e -- lark-binding-view-scope
```

Expected: green across all suites.

- [ ] **Step 2: Run coverage**

```bash
bun run coverage
cd src-tauri && cargo tarpaulin --lib --skip-clean --workspace --out Stdout \
  --include-files 'src/platform/lark_client.rs' \
  --include-files 'src/task_provider/lark.rs' \
  --include-files 'src/persistence/lark_repo_bindings.rs' \
  --include-files 'src/state.rs' \
  --include-files 'src/commands/lark_repo_binding.rs' \
  --include-files 'src/lib.rs'
```

Expected: ≥95% line+branch+function on each changed file. Add tests for any
uncovered branch — common gaps will be:

- The `pagination exceeded MAX_LIST_PAGES` arm in `bitable_list_views` (skip if
  unreachable without a fake page-token loop).
- The `is_view_missing_error` false-positive guard.
- The wizard "loading views" intermediate state.

- [ ] **Step 3: Full lint pass**

```bash
cd src-tauri && cargo fmt --all -- --check && cargo clippy --lib --all-targets -- -D warnings
bun run lint
```

Expected: clean.

- [ ] **Step 4: Manual smoke (with real Lark or mock)**

Run the dev app, open RepoSettings, walk the wizard end-to-end:

1. Enter app_token + table_id → Detect.
2. Step 1.5 dropdown lists views; default "All records".
3. Pick "Current Sprint" → Continue.
4. Field + status mapping → Save.
5. Kanban shows only the view's filtered records.
6. In Lark, archive the bound view → refresh in Ansambel.
7. Banner appears, kanban falls back to all records.
8. Click Reconfigure → wizard opens at Step 1.5.

- [ ] **Step 5: Update CHANGELOG / journal**

If the project maintains a journal (per Phase 3a-3 cadence), add a
`docs/journal/2026-05-16-phase-3a-3-1.md` entry:

```markdown
# 2026-05-16 — Phase 3a-3.1 view-aware Lark binding

Ansambel now honors Lark Bitable per-view filters. Bindings gain an optional
`view_id`; the wizard exposes a new Step 1.5 view picker; view-deletion falls
back gracefully with a non-blocking banner.

PR: #<TBD>
```

- [ ] **Step 6: Commit final touches**

```bash
git add docs/journal/2026-05-16-phase-3a-3-1.md
git commit -m "docs(journal): 2026-05-16 Phase 3a-3.1 view-aware Lark binding"
```

- [ ] **Step 7: Open PR**

Branch off `main`; push; open PR with title
`feat(phase-3a-3-1): view-aware Lark binding`. Include:

- Link to spec.
- Brief summary (3 bullets max).
- Test plan checklist (the manual smoke from Step 4).

---

## Coverage of Spec Requirements

| Spec section             | Implemented by      |
| ------------------------ | ------------------- |
| Goal: optional `view_id` | Task 3              |
| Read path uses view_id   | Tasks 2, 5          |
| Wizard Step 1.5          | Task 8              |
| Migration v1→v2          | Tasks 3, 7          |
| View-deletion fallback   | Tasks 6, 7, 11      |
| Settings dialog "View:"  | Task 9              |
| Banner on event          | Tasks 10, 11        |
| New types (BitableView)  | Task 1              |
| `list_lark_views` cmd    | Task 4              |
| Unit tests               | Tasks 1, 2, 3, 5, 6 |
| Integration tests        | Task 12             |
| Component tests          | Tasks 8, 9, 11      |
| E2E tests                | Task 13             |
| Coverage gate            | Task 14             |

---

## Done when

- 14/14 task groups complete.
- All test suites green (Rust unit, Rust integration, Vitest, Playwright).
- Coverage ≥95% on every changed file.
- Manual smoke walks the 8-step happy path + view-deletion fallback.
- PR opened and ready for review.
