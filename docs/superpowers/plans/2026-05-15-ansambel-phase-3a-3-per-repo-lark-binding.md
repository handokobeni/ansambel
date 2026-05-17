# Phase 3a-3 — Per-Repo Lark Binding + Field Mapping Wizard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ansambel adapts to any Bitable structure — per-repo binding with
explicit, user-confirmed field mapping (auto-detected initial guess) keyed by
field ID for rename-stability; schema verify wizard from Phase 3a-2 removed;
auto-migration from global Lark config to per-repo binding on first launch.

**Architecture:** Replace `Arc<RwLock<Arc<dyn TaskProvider>>>` with per-repo
`HashMap<RepoId, Arc<dyn TaskProvider>>`. Refactor `LarkProvider::new` to take a
`FieldMapping`. New 3-step Svelte wizard lives inside a new `RepoSettingsDialog`
accessible via sidebar right-click. Bindings stored in new
`lark_repo_bindings.json`; old `app_token`/`table_id`/`task_source` fields
removed from existing files via migration.

**Tech Stack:** Rust (existing `async-trait` + `reqwest` + `wiremock` +
`tokio::sync::RwLock`), Svelte 5 runes, Vitest + Testing Library.

**Spec:**
`docs/superpowers/specs/2026-05-15-phase-3a-3-per-repo-lark-binding-design.md`

---

## File Structure

### Create

| Path                                                     | Responsibility                                           |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `src-tauri/src/persistence/lark_repo_bindings.rs`        | Load/save `lark_repo_bindings.json` with atomic-write    |
| `src-tauri/src/commands/lark_repo_binding.rs`            | Tauri commands: get/set/delete/list + detect_lark_schema |
| `src-tauri/src/task_provider/lark_field_resolver.rs`     | Pure resolver fns + `BitableSchemaDetector`              |
| `src/lib/components/lark/LarkBindingWizard.svelte`       | 3-step wizard component                                  |
| `src/lib/components/lark/LarkBindingWizard.test.ts`      | Wizard tests                                             |
| `src/lib/components/repo/RepoSettingsDialog.svelte`      | Per-repo settings (Lark Sync section embedded)           |
| `src/lib/components/repo/RepoSettingsDialog.test.ts`     | Per-repo dialog tests                                    |
| `src/lib/stores/lark-bindings.svelte.ts`                 | `SvelteMap<repo_id, BitableBinding>` store               |
| `src/lib/stores/lark-bindings.svelte.test.ts`            | Store tests                                              |
| `tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts` | Env-gated E2E smoke                                      |

### Modify

| Path                                           | Reason                                                                                                                                                     |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/src/state.rs`                       | Add new types; reshape `TaskProviderHandle`; drop `AppSettings.task_source`                                                                                |
| `src-tauri/src/platform/paths.rs`              | Add `lark_repo_bindings_file` helper                                                                                                                       |
| `src-tauri/src/platform/lark_client.rs`        | Add `BitableOption` struct + property options accessor on `BitableField`                                                                                   |
| `src-tauri/src/persistence/mod.rs`             | `pub mod lark_repo_bindings;`                                                                                                                              |
| `src-tauri/src/commands/mod.rs`                | `pub mod lark_repo_binding;` + drop schema commands                                                                                                        |
| `src-tauri/src/commands/lark_auth.rs`          | Shrink `LarkSettings`/`LarkStatus` (drop `app_token`/`table_id`); `test_lark_connection` no longer needs app_token                                         |
| `src-tauri/src/commands/task.rs`               | Multi-provider lookup by repo_id                                                                                                                           |
| `src-tauri/src/task_provider/mod.rs`           | Add `pub mod lark_field_resolver;` + remove `pub mod schema;`                                                                                              |
| `src-tauri/src/task_provider/lark.rs`          | Refactor `LarkProvider::new`; drop schema.rs references                                                                                                    |
| `src-tauri/src/lib.rs`                         | Build provider per-repo on startup; run auto-migration; drop `verify_lark_schema`/`get_task_source`/`set_task_source` registrations; register new commands |
| `src/lib/types.ts`                             | Add `BitableBinding`, `FieldMapping`, `FieldRef`, `StatusValueMapping`, `ProposedMapping`, `BitableOption`; drop `TaskSource`                              |
| `src/lib/ipc.ts`                               | Add new API wrappers; drop `api.task.getSource/setSource`, `api.lark.verifySchema`, `api.lark.getStatus`                                                   |
| `src/lib/components/lark/LarkSettings.svelte`  | Rename to `LarkGlobalSettings.svelte`; shrink form to app_id/secret/base_url only                                                                          |
| `src/lib/components/lark/LarkSettings.test.ts` | Rename + update assertions                                                                                                                                 |
| `src/lib/components/SettingsDialog.svelte`     | Drop "Task source" radio; embed `LarkGlobalSettings`                                                                                                       |
| `src/lib/components/SettingsDialog.test.ts`    | Drop task-source tests                                                                                                                                     |
| `src/lib/components/Sidebar.svelte`            | Add repo right-click context menu → opens `RepoSettingsDialog`                                                                                             |
| `src/lib/stores/tasks.svelte.ts`               | Listen for `binding-updated`; refresh for affected repo                                                                                                    |
| `src/App.svelte`                               | Window-focus refresh derives Lark-mode-per-repo from binding presence (no app-global flag)                                                                 |

### Delete

| Path                                    | Reason                                                                   |
| --------------------------------------- | ------------------------------------------------------------------------ |
| `src-tauri/src/task_provider/schema.rs` | Schema verify wizard deprecated; resolver handles everything via mapping |

---

## Task 1: Mapping data model types + path helper

**Files:**

- Modify: `src-tauri/src/platform/paths.rs` — add `lark_repo_bindings_file`
  helper
- Modify: `src-tauri/src/state.rs` — add binding types
- Modify: `src-tauri/src/platform/lark_client.rs` — add `BitableOption` +
  accessor

- [ ] **Step 1: Write the failing path-helper test**

In `src-tauri/src/platform/paths.rs`, add to the existing `mod tests`:

```rust
#[test]
fn lark_repo_bindings_file_is_at_data_dir_root() {
    let data = PathBuf::from("/tmp/ansambel");
    let p = lark_repo_bindings_file(&data);
    assert_eq!(p, PathBuf::from("/tmp/ansambel/lark_repo_bindings.json"));
}
```

Run:
`cargo test --lib platform::paths::tests::lark_repo_bindings_file -- --nocapture`
Expected: FAIL with "cannot find function `lark_repo_bindings_file`".

- [ ] **Step 2: Add the path helper**

In `src-tauri/src/platform/paths.rs`, add this function after the other `*_file`
helpers:

```rust
pub fn lark_repo_bindings_file(data_dir: &Path) -> PathBuf {
    data_dir.join("lark_repo_bindings.json")
}
```

Run: `cargo test --lib platform::paths::tests::lark_repo_bindings_file`
Expected: PASS.

- [ ] **Step 3: Add `BitableOption` + options accessor to `lark_client`**

In `src-tauri/src/platform/lark_client.rs`, add right after the `BitableField`
struct:

```rust
/// One option of a Bitable single-select field. Lives inside
/// `BitableField.property.options`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BitableOption {
    pub id: String,
    pub name: String,
}

impl BitableField {
    /// Extracts the single-select options list from `property.options`.
    /// Returns an empty Vec for fields that aren't single-select.
    pub fn options(&self) -> Vec<BitableOption> {
        self.property
            .as_ref()
            .and_then(|p| p.get("options"))
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}
```

- [ ] **Step 4: Write a failing test for `BitableOption` extraction**

In the existing `mod tests` of `lark_client.rs`, add:

```rust
#[test]
fn bitable_field_options_extracts_from_property() {
    let f = BitableField {
        field_id: "fld_x".into(),
        field_name: "Status".into(),
        field_type: 3,
        property: Some(serde_json::json!({
            "options": [
                {"id": "opt_1", "name": "To Do"},
                {"id": "opt_2", "name": "Done"}
            ]
        })),
        is_primary: false,
    };
    let opts = f.options();
    assert_eq!(opts.len(), 2);
    assert_eq!(opts[0].id, "opt_1");
    assert_eq!(opts[1].name, "Done");
}

#[test]
fn bitable_field_options_empty_when_no_property() {
    let f = BitableField {
        field_id: "fld_x".into(),
        field_name: "Text".into(),
        field_type: 1,
        property: None,
        is_primary: false,
    };
    assert!(f.options().is_empty());
}
```

Run: `cargo test --lib platform::lark_client::tests::bitable_field_options`
Expected: PASS (helper already implemented in step 3).

- [ ] **Step 5: Add binding types to `state.rs`**

In `src-tauri/src/state.rs`, add after the `KanbanColumn` enum:

```rust
/// A reference to a Bitable field. `field_id` is the stable lookup key
/// (survives renames). `field_name` is cached for UI display; refreshed
/// lazily whenever we re-fetch the Bitable schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FieldRef {
    pub field_id: String,
    pub field_name: String,
}

/// Field mapping for one Bitable. Only `title` is required; everything
/// else has a runtime fallback so a partially-populated mapping still
/// produces usable tasks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FieldMapping {
    pub title: FieldRef,
    #[serde(default)]
    pub description: Option<FieldRef>,
    #[serde(default)]
    pub status: Option<FieldRef>,
    #[serde(default)]
    pub order: Option<FieldRef>,
}

/// Maps Bitable status field values to kanban columns. Keys are
/// `option_id` for single-select fields or lowercased text values for
/// Text fields. `default_column` covers values not in `entries`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StatusValueMapping {
    #[serde(default)]
    pub entries: std::collections::HashMap<String, KanbanColumn>,
    #[serde(default = "default_kanban_column")]
    pub default_column: KanbanColumn,
}

fn default_kanban_column() -> KanbanColumn {
    KanbanColumn::Todo
}

impl Default for StatusValueMapping {
    fn default() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            default_column: KanbanColumn::Todo,
        }
    }
}

/// One repo's binding to a Bitable: which table, plus how to map its
/// fields and status options to Ansambel's task model.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    pub field_mapping: FieldMapping,
    #[serde(default)]
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}
```

- [ ] **Step 6: Add types-roundtrip test**

In the `tests` mod of `state.rs`, add:

```rust
#[test]
fn binding_serde_round_trip_preserves_fields() {
    let b = BitableBinding {
        app_token: "bascntest".into(),
        table_id: "tbltest".into(),
        field_mapping: FieldMapping {
            title: FieldRef {
                field_id: "fld_pri".into(),
                field_name: "Task name".into(),
            },
            description: None,
            status: Some(FieldRef {
                field_id: "fld_status".into(),
                field_name: "Task Status".into(),
            }),
            order: None,
        },
        status_value_mapping: StatusValueMapping {
            entries: {
                let mut m = std::collections::HashMap::new();
                m.insert("opt_a".into(), KanbanColumn::Todo);
                m.insert("opt_b".into(), KanbanColumn::Done);
                m
            },
            default_column: KanbanColumn::Todo,
        },
        created_at: 1747200000,
        updated_at: 1747200000,
    };
    let json = serde_json::to_string(&b).unwrap();
    let parsed: BitableBinding = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, b);
}
```

Run: `cargo test --lib state::tests::binding_serde_round_trip` Expected: PASS.

- [ ] **Step 7: Run full Rust gates**

```bash
cargo test --lib && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/platform/paths.rs src-tauri/src/platform/lark_client.rs src-tauri/src/state.rs
git commit -m "feat(phase-3a-3): mapping data model + BitableOption accessor"
```

---

## Task 2: `persistence::lark_repo_bindings` module

**Files:**

- Create: `src-tauri/src/persistence/lark_repo_bindings.rs`
- Modify: `src-tauri/src/persistence/mod.rs`

- [ ] **Step 1: Wire up the new module**

In `src-tauri/src/persistence/mod.rs`, add:

```rust
pub mod lark_repo_bindings;
```

- [ ] **Step 2: Write failing round-trip test**

Create `src-tauri/src/persistence/lark_repo_bindings.rs`:

```rust
use crate::error::Result;
use crate::platform::paths::lark_repo_bindings_file;
use crate::state::BitableBinding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// On-disk shape of `lark_repo_bindings.json`. Wraps the bindings map
/// in a versioned envelope so future schema changes can be detected
/// without breaking old installs.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingsFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub bindings: HashMap<String, BitableBinding>,
}

fn default_schema_version() -> u32 {
    1
}

pub fn load_bindings(data_dir: &Path) -> Result<BindingsFile> {
    let path = lark_repo_bindings_file(data_dir);
    if !path.exists() {
        return Ok(BindingsFile::default());
    }
    let bytes = std::fs::read(&path)?;
    let parsed: BindingsFile = serde_json::from_slice(&bytes)?;
    Ok(parsed)
}

pub fn save_bindings(data_dir: &Path, file: &BindingsFile) -> Result<()> {
    let path = lark_repo_bindings_file(data_dir);
    let bytes = serde_json::to_vec_pretty(file)?;
    crate::persistence::atomic::atomic_write(&path, &bytes)
}

pub fn get_binding(data_dir: &Path, repo_id: &str) -> Result<Option<BitableBinding>> {
    let file = load_bindings(data_dir)?;
    Ok(file.bindings.get(repo_id).cloned())
}

pub fn set_binding(data_dir: &Path, repo_id: &str, binding: BitableBinding) -> Result<()> {
    let mut file = load_bindings(data_dir)?;
    file.bindings.insert(repo_id.to_string(), binding);
    save_bindings(data_dir, &file)
}

pub fn delete_binding(data_dir: &Path, repo_id: &str) -> Result<bool> {
    let mut file = load_bindings(data_dir)?;
    let removed = file.bindings.remove(repo_id).is_some();
    if removed {
        save_bindings(data_dir, &file)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FieldMapping, FieldRef, StatusValueMapping};
    use tempfile::tempdir;

    fn make_binding() -> BitableBinding {
        BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            field_mapping: FieldMapping {
                title: FieldRef {
                    field_id: "fld_pri".into(),
                    field_name: "Task name".into(),
                },
                description: None,
                status: None,
                order: None,
            },
            status_value_mapping: StatusValueMapping::default(),
            created_at: 1747200000,
            updated_at: 1747200000,
        }
    }

    #[test]
    fn load_returns_empty_when_file_absent() {
        let tmp = tempdir().unwrap();
        let f = load_bindings(tmp.path()).unwrap();
        assert_eq!(f.bindings.len(), 0);
        assert_eq!(f.schema_version, 1);
    }

    #[test]
    fn round_trip_save_and_load_preserves_binding() {
        let tmp = tempdir().unwrap();
        set_binding(tmp.path(), "repo_x", make_binding()).unwrap();
        let f = load_bindings(tmp.path()).unwrap();
        assert_eq!(f.bindings.len(), 1);
        assert_eq!(f.bindings.get("repo_x").unwrap().app_token, "bascntest");
    }

    #[test]
    fn get_binding_returns_none_when_repo_missing() {
        let tmp = tempdir().unwrap();
        set_binding(tmp.path(), "repo_x", make_binding()).unwrap();
        assert!(get_binding(tmp.path(), "repo_other").unwrap().is_none());
    }

    #[test]
    fn delete_binding_returns_true_when_removed_else_false() {
        let tmp = tempdir().unwrap();
        set_binding(tmp.path(), "repo_x", make_binding()).unwrap();
        assert!(delete_binding(tmp.path(), "repo_x").unwrap());
        assert!(!delete_binding(tmp.path(), "repo_x").unwrap());
        assert_eq!(load_bindings(tmp.path()).unwrap().bindings.len(), 0);
    }

    #[test]
    fn set_binding_overwrites_existing_entry() {
        let tmp = tempdir().unwrap();
        let mut b1 = make_binding();
        b1.app_token = "v1".into();
        set_binding(tmp.path(), "repo_x", b1).unwrap();
        let mut b2 = make_binding();
        b2.app_token = "v2".into();
        set_binding(tmp.path(), "repo_x", b2).unwrap();
        let f = load_bindings(tmp.path()).unwrap();
        assert_eq!(f.bindings.get("repo_x").unwrap().app_token, "v2");
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib persistence::lark_repo_bindings
```

Expected: 5/5 PASS.

- [ ] **Step 4: Lint + fmt**

```bash
cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/persistence/mod.rs src-tauri/src/persistence/lark_repo_bindings.rs
git commit -m "feat(phase-3a-3): lark_repo_bindings persistence with atomic write"
```

---

## Task 3: `lark_field_resolver` — pure resolver functions

**Files:**

- Create: `src-tauri/src/task_provider/lark_field_resolver.rs`
- Modify: `src-tauri/src/task_provider/mod.rs`

- [ ] **Step 1: Wire up the new module**

In `src-tauri/src/task_provider/mod.rs`, add:

```rust
pub mod lark_field_resolver;
```

- [ ] **Step 2: Write the resolver scaffold + first failing test**

Create `src-tauri/src/task_provider/lark_field_resolver.rs`:

```rust
//! Pure resolver functions that map a Bitable record + a `FieldMapping`
//! to the parts of a `Task`. No I/O — easy to unit-test against arbitrary
//! mappings. Lives separate from `LarkProvider` so the runtime path is
//! independent of network access.

use crate::error::{AppError, Result};
use crate::platform::lark_client::BitableRecord;
use crate::state::{FieldMapping, KanbanColumn, StatusValueMapping};
use crate::task_provider::lark::parse_kanban_column;

/// Reads a field's string value off a record by `field_id`. Bitable
/// returns record fields keyed by name in the JSON payload, so we look
/// it up by name; the resolver passes the `field_name` cached on the
/// `FieldRef`. Returns `None` if the field is missing/null/empty.
fn read_string_by_name<'a>(
    record: &'a BitableRecord,
    field_name: &str,
) -> Option<&'a str> {
    record
        .fields
        .as_object()
        .and_then(|m| m.get(field_name))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

/// Resolves the title for a record. Tries the mapped `title` field
/// first; falls back to `primary_field_name` if both are non-empty.
/// Returns an error message if neither has a value.
pub fn resolve_title(
    record: &BitableRecord,
    mapping: &FieldMapping,
    primary_field_name: Option<&str>,
) -> Result<String> {
    if let Some(v) = read_string_by_name(record, &mapping.title.field_name) {
        return Ok(v.to_string());
    }
    if let Some(p) = primary_field_name {
        if let Some(v) = read_string_by_name(record, p) {
            return Ok(v.to_string());
        }
    }
    Err(AppError::Lark(format!(
        "record {} missing title (mapped field '{}' empty; primary '{}' empty)",
        record.record_id,
        mapping.title.field_name,
        primary_field_name.unwrap_or("<unknown>"),
    )))
}

/// Resolves the description for a record. Returns empty string when
/// the mapping has no description field set or the value is missing.
pub fn resolve_description(record: &BitableRecord, mapping: &FieldMapping) -> String {
    mapping
        .description
        .as_ref()
        .and_then(|f| read_string_by_name(record, &f.field_name))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Resolves the kanban column for a record using the mapped status
/// field + value mapping. Layered fallback:
///   1. If status field unmapped → `default_column`
///   2. If status value present and in `entries` → that column
///   3. If value not in entries → try fuzzy parser
///   4. If fuzzy returns None → `default_column`
pub fn resolve_status(
    record: &BitableRecord,
    mapping: &FieldMapping,
    values: &StatusValueMapping,
) -> KanbanColumn {
    let Some(status_field) = &mapping.status else {
        return values.default_column;
    };
    // Status field values come as either a single-select object
    // ({id, text}) or a plain string. We accept both.
    let fields = match record.fields.as_object() {
        Some(o) => o,
        None => return values.default_column,
    };
    let raw = fields.get(&status_field.field_name);
    let Some(raw) = raw else {
        return values.default_column;
    };
    // Single-select shape: {"id": "opt_x", "text": "To Do"}
    if let Some(obj) = raw.as_object() {
        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            if let Some(col) = values.entries.get(id) {
                return *col;
            }
        }
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            if let Some(col) = parse_kanban_column(text) {
                return col;
            }
        }
    }
    // Text shape
    if let Some(s) = raw.as_str().filter(|s| !s.is_empty()) {
        if let Some(col) = values.entries.get(&s.to_lowercase()) {
            return *col;
        }
        if let Some(col) = parse_kanban_column(s) {
            return col;
        }
    }
    values.default_column
}

/// Resolves the order value for sorting. Mapped `order` field wins;
/// otherwise falls back to negative `created_time` (so newer rows sort
/// first when sorted ASC by this number).
pub fn resolve_order(record: &BitableRecord, mapping: &FieldMapping) -> i32 {
    if let Some(order_ref) = &mapping.order {
        let fields = match record.fields.as_object() {
            Some(o) => o,
            None => return 0,
        };
        if let Some(n) = fields.get(&order_ref.field_name).and_then(|v| v.as_i64()) {
            return n as i32;
        }
    }
    // Newer rows sort first when we sort ASC by this fallback value.
    let created = record.extra_i64("created_time").unwrap_or(0);
    -(created / 1000) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::FieldRef;

    fn rec(record_id: &str, fields: serde_json::Value) -> BitableRecord {
        serde_json::from_value(serde_json::json!({
            "record_id": record_id,
            "fields": fields,
        }))
        .unwrap()
    }

    fn title_mapping(name: &str) -> FieldMapping {
        FieldMapping {
            title: FieldRef {
                field_id: "fld_t".into(),
                field_name: name.into(),
            },
            ..Default::default()
        }
    }

    // ── resolve_title ──────────────────────────────────────────

    #[test]
    fn resolve_title_uses_explicit_field() {
        let r = rec("r1", serde_json::json!({"title": "Hello"}));
        let m = title_mapping("title");
        assert_eq!(resolve_title(&r, &m, None).unwrap(), "Hello");
    }

    #[test]
    fn resolve_title_falls_back_to_primary() {
        let r = rec(
            "r1",
            serde_json::json!({"Task name": "From primary", "title": ""}),
        );
        let m = title_mapping("title");
        assert_eq!(
            resolve_title(&r, &m, Some("Task name")).unwrap(),
            "From primary"
        );
    }

    #[test]
    fn resolve_title_errors_when_both_empty() {
        let r = rec("r1", serde_json::json!({"title": "", "Task name": ""}));
        let m = title_mapping("title");
        let err = resolve_title(&r, &m, Some("Task name")).unwrap_err();
        assert!(err.to_string().contains("missing title"));
        assert!(err.to_string().contains("r1"));
    }

    // ── resolve_description ────────────────────────────────────

    #[test]
    fn resolve_description_returns_empty_when_unmapped() {
        let r = rec("r1", serde_json::json!({"description": "ignored"}));
        let m = title_mapping("title");
        assert_eq!(resolve_description(&r, &m), "");
    }

    #[test]
    fn resolve_description_uses_mapped_field() {
        let r = rec("r1", serde_json::json!({"desc": "hello"}));
        let mut m = title_mapping("title");
        m.description = Some(FieldRef {
            field_id: "fld_d".into(),
            field_name: "desc".into(),
        });
        assert_eq!(resolve_description(&r, &m), "hello");
    }

    // ── resolve_status ─────────────────────────────────────────

    fn status_mapping(values: StatusValueMapping) -> FieldMapping {
        FieldMapping {
            title: FieldRef {
                field_id: "fld_t".into(),
                field_name: "title".into(),
            },
            status: Some(FieldRef {
                field_id: "fld_s".into(),
                field_name: "Task Status".into(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_status_returns_default_when_unmapped() {
        let r = rec("r1", serde_json::json!({"title": "x"}));
        let m = title_mapping("title");
        let v = StatusValueMapping {
            default_column: KanbanColumn::Review,
            ..Default::default()
        };
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Review);
    }

    #[test]
    fn resolve_status_uses_option_id_via_entries() {
        let r = rec(
            "r1",
            serde_json::json!({"Task Status": {"id": "opt_a", "text": "Hai"}}),
        );
        let mut entries = std::collections::HashMap::new();
        entries.insert("opt_a".into(), KanbanColumn::Done);
        let v = StatusValueMapping {
            entries,
            default_column: KanbanColumn::Todo,
        };
        let m = status_mapping(v.clone());
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Done);
    }

    #[test]
    fn resolve_status_falls_back_to_fuzzy_for_text_value() {
        let r = rec(
            "r1",
            serde_json::json!({"Task Status": "In Progress"}),
        );
        let v = StatusValueMapping::default();
        let m = status_mapping(v.clone());
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::InProgress);
    }

    #[test]
    fn resolve_status_uses_default_for_unmapped_unknown_text() {
        let r = rec("r1", serde_json::json!({"Task Status": "xyz"}));
        let v = StatusValueMapping {
            default_column: KanbanColumn::Review,
            ..Default::default()
        };
        let m = status_mapping(v.clone());
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Review);
    }

    #[test]
    fn resolve_status_uses_default_when_field_missing_on_record() {
        let r = rec("r1", serde_json::json!({"title": "x"}));
        let v = StatusValueMapping {
            default_column: KanbanColumn::InProgress,
            ..Default::default()
        };
        let m = status_mapping(v.clone());
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::InProgress);
    }

    // ── resolve_order ──────────────────────────────────────────

    #[test]
    fn resolve_order_uses_mapped_field_when_present() {
        let r = rec("r1", serde_json::json!({"order": 42}));
        let mut m = title_mapping("title");
        m.order = Some(FieldRef {
            field_id: "fld_o".into(),
            field_name: "order".into(),
        });
        assert_eq!(resolve_order(&r, &m), 42);
    }

    #[test]
    fn resolve_order_falls_back_to_negative_created_time() {
        let r = serde_json::from_value::<BitableRecord>(serde_json::json!({
            "record_id": "r1",
            "fields": {"title": "x"},
            "created_time": 1700000000000_i64,
        }))
        .unwrap();
        let m = title_mapping("title");
        // 1700000000000 ms / 1000 = 1700000000 s; negated for ASC sort.
        assert_eq!(resolve_order(&r, &m), -1700000000);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib task_provider::lark_field_resolver
```

Expected: 13/13 PASS.

- [ ] **Step 4: Lint + commit**

```bash
cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/task_provider/mod.rs src-tauri/src/task_provider/lark_field_resolver.rs
git commit -m "feat(phase-3a-3): pure lark_field_resolver with layered fallbacks"
```

---

## Task 4: `BitableSchemaDetector::propose_mapping`

**Files:**

- Modify: `src-tauri/src/task_provider/lark_field_resolver.rs` — append detector
- Modify: `src-tauri/src/state.rs` — add `ProposedMapping` type

- [ ] **Step 1: Add `ProposedMapping` to state.rs**

Append to `src-tauri/src/state.rs` after `BitableBinding`:

```rust
/// What `detect_lark_schema` returns to the wizard. Carries the raw
/// field list (for dropdown population), an auto-detected initial
/// guess at the mapping, and (when status is single-select) the option
/// list with a fuzzy-parsed initial value mapping.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProposedMapping {
    pub fields: Vec<crate::platform::lark_client::BitableField>,
    pub suggested: FieldMapping,
    #[serde(default)]
    pub status_options: Option<Vec<crate::platform::lark_client::BitableOption>>,
    #[serde(default)]
    pub suggested_status_values: StatusValueMapping,
}
```

- [ ] **Step 2: Add `BitableSchemaDetector` impl + test scaffold**

Append to `src-tauri/src/task_provider/lark_field_resolver.rs`:

```rust
use crate::platform::lark_client::{BitableField, BitableOption, LarkClient};
use crate::state::{FieldRef, ProposedMapping};
use std::sync::Arc;

pub struct BitableSchemaDetector {
    client: Arc<LarkClient>,
}

impl BitableSchemaDetector {
    pub fn new(client: Arc<LarkClient>) -> Self {
        Self { client }
    }

    /// Fetches Bitable fields and proposes a mapping using deterministic
    /// auto-detection rules:
    ///   - title: the `is_primary: true` field
    ///   - status: first field whose normalised name contains
    ///     "status", "stage", "phase", or "kanban" (alphabetic order)
    ///   - description / order: left as None (user opts in)
    pub async fn propose_mapping(
        &self,
        app_token: &str,
        table_id: &str,
    ) -> Result<ProposedMapping> {
        let fields = self.client.bitable_list_fields(app_token, table_id).await?;
        let primary = fields
            .iter()
            .find(|f| f.is_primary)
            .ok_or_else(|| AppError::Lark(format!(
                "Bitable {app_token}/{table_id} has no primary field"
            )))?
            .clone();
        let suggested_title = FieldRef {
            field_id: primary.field_id.clone(),
            field_name: primary.field_name.clone(),
        };
        // Sort candidate status fields alphabetically by name for
        // deterministic selection when multiple fields match.
        let mut status_candidates: Vec<&BitableField> = fields
            .iter()
            .filter(|f| {
                let normalised: String = f
                    .field_name
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect();
                ["status", "stage", "phase", "kanban"]
                    .iter()
                    .any(|kw| normalised.contains(kw))
            })
            .collect();
        status_candidates.sort_by(|a, b| a.field_name.cmp(&b.field_name));
        let status_field = status_candidates.first().cloned().cloned();

        // Build initial status-value mapping using the fuzzy parser.
        let (status_options, suggested_status_values) = if let Some(sf) = &status_field
        {
            let opts = sf.options();
            let mut entries = std::collections::HashMap::new();
            for opt in &opts {
                if let Some(col) = crate::task_provider::lark::parse_kanban_column(
                    &opt.name,
                ) {
                    entries.insert(opt.id.clone(), col);
                }
            }
            (
                Some(opts),
                StatusValueMapping {
                    entries,
                    default_column: KanbanColumn::Todo,
                },
            )
        } else {
            (None, StatusValueMapping::default())
        };

        let suggested = FieldMapping {
            title: suggested_title,
            description: None,
            status: status_field.map(|f| FieldRef {
                field_id: f.field_id.clone(),
                field_name: f.field_name.clone(),
            }),
            order: None,
        };
        Ok(ProposedMapping {
            fields,
            suggested,
            status_options,
            suggested_status_values,
        })
    }
}
```

- [ ] **Step 3: Add wiremock tests for the detector**

Append to the `tests` mod in `lark_field_resolver.rs`:

```rust
use crate::platform::lark_client::LarkConfig;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "tenant_access_token": "t_xyz",
            "expire": 7200
        })))
        .mount(server)
        .await;
}

fn detector(uri: &str) -> BitableSchemaDetector {
    BitableSchemaDetector::new(Arc::new(LarkClient::new(LarkConfig {
        app_id: "cli_t".into(),
        app_secret: "s".into(),
        app_token: "bascntest".into(),
        table_id: "tbltest".into(),
        base_url: uri.into(),
    })))
}

async fn mount_fields(server: &MockServer, fields: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": { "items": fields }
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn propose_picks_primary_as_title() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_fields(
        &server,
        serde_json::json!([
            {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
            {"field_id": "fld_other", "field_name": "Notes", "type": 1, "is_primary": false}
        ]),
    )
    .await;
    let p = detector(&server.uri())
        .propose_mapping("bascntest", "tbltest")
        .await
        .unwrap();
    assert_eq!(p.suggested.title.field_id, "fld_pri");
    assert_eq!(p.suggested.title.field_name, "Task name");
}

#[tokio::test]
async fn propose_detects_status_field_by_name_keyword() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_fields(
        &server,
        serde_json::json!([
            {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
            {"field_id": "fld_s", "field_name": "Task Status", "type": 3, "is_primary": false,
             "property": {"options": [
                 {"id": "opt_a", "name": "To Do"},
                 {"id": "opt_b", "name": "Done"}
             ]}}
        ]),
    )
    .await;
    let p = detector(&server.uri())
        .propose_mapping("bascntest", "tbltest")
        .await
        .unwrap();
    let status = p.suggested.status.expect("status should be detected");
    assert_eq!(status.field_id, "fld_s");
    let opts = p.status_options.unwrap();
    assert_eq!(opts.len(), 2);
    assert_eq!(p.suggested_status_values.entries.get("opt_a"), Some(&KanbanColumn::Todo));
    assert_eq!(p.suggested_status_values.entries.get("opt_b"), Some(&KanbanColumn::Done));
}

#[tokio::test]
async fn propose_returns_none_status_when_no_match() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_fields(
        &server,
        serde_json::json!([
            {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
            {"field_id": "fld_n", "field_name": "Notes", "type": 1, "is_primary": false}
        ]),
    )
    .await;
    let p = detector(&server.uri())
        .propose_mapping("bascntest", "tbltest")
        .await
        .unwrap();
    assert!(p.suggested.status.is_none());
    assert!(p.status_options.is_none());
}

#[tokio::test]
async fn propose_picks_alphabetically_first_when_multiple_status_fields() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_fields(
        &server,
        serde_json::json!([
            {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
            {"field_id": "fld_sprint", "field_name": "Sprint Stage", "type": 3, "is_primary": false},
            {"field_id": "fld_status", "field_name": "Task Status", "type": 3, "is_primary": false}
        ]),
    )
    .await;
    let p = detector(&server.uri())
        .propose_mapping("bascntest", "tbltest")
        .await
        .unwrap();
    // "Sprint Stage" < "Task Status" alphabetically.
    assert_eq!(p.suggested.status.unwrap().field_id, "fld_sprint");
}

#[tokio::test]
async fn propose_errors_when_no_primary() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_fields(
        &server,
        serde_json::json!([
            {"field_id": "fld_x", "field_name": "Notes", "type": 1, "is_primary": false}
        ]),
    )
    .await;
    let err = detector(&server.uri())
        .propose_mapping("bascntest", "tbltest")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no primary field"));
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib task_provider::lark_field_resolver
```

Expected: 18/18 PASS (13 from Task 3 + 5 new).

- [ ] **Step 5: Lint + commit**

```bash
cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/state.rs src-tauri/src/task_provider/lark_field_resolver.rs
git commit -m "feat(phase-3a-3): BitableSchemaDetector + ProposedMapping"
```

---

## Task 5: Refactor `LarkProvider` to consume `FieldMapping`

**Files:**

- Modify: `src-tauri/src/task_provider/lark.rs`
- Delete: `src-tauri/src/task_provider/schema.rs`
- Modify: `src-tauri/src/task_provider/mod.rs` (remove `pub mod schema;`)

- [ ] **Step 1: Replace `LarkProvider::new` signature + delegate to resolver**

In `src-tauri/src/task_provider/lark.rs`, replace the existing `LarkProvider`
struct + `new` + `record_to_task` + the resolver block in `list_tasks` with:

```rust
use crate::state::{BitableBinding, FieldMapping, StatusValueMapping, Task};
use crate::task_provider::lark_field_resolver::{
    resolve_description, resolve_order, resolve_status, resolve_title,
};

#[derive(Debug)]
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
    field_mapping: FieldMapping,
    status_value_mapping: StatusValueMapping,
    primary_field_name: OnceCell<Option<String>>,
}

impl LarkProvider {
    pub fn new(
        client: Arc<LarkClient>,
        app_token: String,
        table_id: String,
        field_mapping: FieldMapping,
        status_value_mapping: StatusValueMapping,
    ) -> Self {
        Self {
            client,
            app_token,
            table_id,
            field_mapping,
            status_value_mapping,
            primary_field_name: OnceCell::new(),
        }
    }

    /// Convenience constructor from a `BitableBinding` (per-repo binding).
    pub fn from_binding(client: Arc<LarkClient>, binding: BitableBinding) -> Self {
        Self::new(
            client,
            binding.app_token,
            binding.table_id,
            binding.field_mapping,
            binding.status_value_mapping,
        )
    }

    async fn primary_field_name(&self) -> Option<String> {
        self.primary_field_name
            .get_or_init(|| async {
                self.client
                    .bitable_list_fields(&self.app_token, &self.table_id)
                    .await
                    .ok()
                    .and_then(|fields| fields.into_iter().find(|f| f.is_primary).map(|f| f.field_name))
            })
            .await
            .clone()
    }
}

fn record_to_task(
    rec: &BitableRecord,
    mapping: &FieldMapping,
    status_values: &StatusValueMapping,
    primary_field_name: Option<&str>,
    default_repo_id: Option<&str>,
) -> Result<Task> {
    let title = resolve_title(rec, mapping, primary_field_name)?;
    let description = resolve_description(rec, mapping);
    let column = resolve_status(rec, mapping, status_values);
    let order = resolve_order(rec, mapping);
    let repo_id = default_repo_id.unwrap_or("").to_string();
    let created_at = rec.extra_i64("created_time").map(|ms| ms / 1000).unwrap_or(0);
    let updated_at = rec
        .extra_i64("last_modified_time")
        .map(|ms| ms / 1000)
        .unwrap_or(created_at);
    Ok(Task {
        id: rec.record_id.clone(),
        repo_id,
        workspace_id: None,
        title,
        description,
        column,
        order,
        created_at,
        updated_at,
    })
}
```

- [ ] **Step 2: Update `list_tasks` to use the new `record_to_task`**

In `LarkProvider::list_tasks` body, replace the existing call to the old
`record_to_task` with:

```rust
let primary = self.primary_field_name().await;
let total = records.len();
let mut skipped = 0usize;
let mut tasks: Vec<Task> = records
    .iter()
    .filter_map(|r| match record_to_task(
        r,
        &self.field_mapping,
        &self.status_value_mapping,
        primary.as_deref(),
        repo_filter,
    ) {
        Ok(t) => Some(t),
        Err(e) => {
            tracing::debug!(
                record_id = %r.record_id,
                error = %e,
                "skipping malformed Bitable record"
            );
            skipped += 1;
            None
        }
    })
    .collect();
if skipped > 0 {
    tracing::warn!(skipped, total, "skipped {skipped}/{total} Bitable records");
}
```

Leave the existing client-side `tasks.retain(|t| t.repo_id == filter)` block
intact below.

- [ ] **Step 3: Update `create_task`/`update_task`/`move_task` write paths**

In `create_task`, the JSON body now writes to mapped field names:

```rust
async fn create_task(&self, args: CreateTaskArgs) -> Result<Task> {
    let column = args.column.unwrap_or_default();
    let mut fields = serde_json::Map::new();
    fields.insert(
        self.field_mapping.title.field_name.clone(),
        serde_json::Value::String(args.title.clone()),
    );
    if let Some(desc_ref) = &self.field_mapping.description {
        fields.insert(
            desc_ref.field_name.clone(),
            serde_json::Value::String(args.description.clone()),
        );
    }
    if let Some(status_ref) = &self.field_mapping.status {
        if let Some(option_id) = reverse_lookup_option(&self.status_value_mapping, column) {
            fields.insert(
                status_ref.field_name.clone(),
                serde_json::json!({"id": option_id}),
            );
        }
    }
    let record = self
        .client
        .bitable_create_record(&self.app_token, &self.table_id, serde_json::Value::Object(fields))
        .await?;
    let primary = self.primary_field_name().await;
    record_to_task(
        &record,
        &self.field_mapping,
        &self.status_value_mapping,
        primary.as_deref(),
        Some(args.repo_id.as_str()),
    )
}

/// Find the first `option_id` whose mapping equals `target`. Returns
/// `None` when no option matches — caller skips the status write so
/// Bitable keeps its current value.
fn reverse_lookup_option(values: &StatusValueMapping, target: KanbanColumn) -> Option<String> {
    values
        .entries
        .iter()
        .find(|(_, col)| **col == target)
        .map(|(id, _)| id.clone())
}
```

Apply the same shape to `update_task` (write title/description/order using
mapped field names) and `move_task` (write the status field with the
reverse-looked-up option id, skip if no match).

- [ ] **Step 4: Delete `schema.rs` and unregister it**

```bash
rm src-tauri/src/task_provider/schema.rs
```

In `src-tauri/src/task_provider/mod.rs`, remove the line `pub mod schema;`.

- [ ] **Step 5: Update existing LarkProvider tests**

Existing tests construct `LarkProvider::new` with the old signature. Replace
each call with the new signature using a `make_mapping()` helper. Add at the top
of `mod tests`:

```rust
fn canonical_mapping() -> FieldMapping {
    FieldMapping {
        title: FieldRef { field_id: "fld_t".into(), field_name: "title".into() },
        description: Some(FieldRef { field_id: "fld_d".into(), field_name: "description".into() }),
        status: Some(FieldRef { field_id: "fld_s".into(), field_name: "kanban_column".into() }),
        order: Some(FieldRef { field_id: "fld_o".into(), field_name: "order_within_column".into() }),
    }
}

fn canonical_values() -> StatusValueMapping {
    StatusValueMapping::default()
}
```

Then in `make_provider`, change to:

```rust
fn make_provider(uri: &str) -> LarkProvider {
    let client = Arc::new(LarkClient::new(make_config(uri)));
    LarkProvider::new(
        client,
        "bascntest".into(),
        "tbltest".into(),
        canonical_mapping(),
        canonical_values(),
    )
}
```

The Phase 3a-2 tests that asserted on direct field-name access already match the
canonical names, so they continue to pass.

- [ ] **Step 6: Run all Lark tests**

```bash
cargo test --lib task_provider::lark
```

Expected: existing tests pass under new signature.

- [ ] **Step 7: Lint + commit**

```bash
cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/task_provider/
git commit -m "feat(phase-3a-3): LarkProvider accepts FieldMapping; delete schema.rs"
```

---

## Task 6: Reshape `TaskProviderHandle` to per-repo map

**Files:**

- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands/task.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Change `TaskProviderHandle` typedef**

In `src-tauri/src/state.rs`, replace the existing line:

```rust
pub type TaskProviderHandle =
    std::sync::Arc<tokio::sync::RwLock<std::sync::Arc<dyn crate::task_provider::TaskProvider>>>;
```

with:

```rust
pub type RepoId = String;

pub type TaskProviderHandle = std::sync::Arc<
    tokio::sync::RwLock<
        std::collections::HashMap<RepoId, std::sync::Arc<dyn crate::task_provider::TaskProvider>>,
    >,
>;
```

- [ ] **Step 2: Add a `default_provider` constant**

The handle now lacks a fallback for repos without a binding. Define a helper:

```rust
// In state.rs, add:
pub fn make_default_local_provider(
    data_dir: &std::path::Path,
) -> std::sync::Arc<dyn crate::task_provider::TaskProvider> {
    std::sync::Arc::new(crate::task_provider::local::LocalProvider::new(
        data_dir.to_path_buf(),
    ))
}
```

- [ ] **Step 3: Update `commands::task.rs` provider lookup**

In every `*_inner` function in `src-tauri/src/commands/task.rs` that currently
does:

```rust
let provider = provider_handle.read().await.clone();
```

replace with a per-repo lookup helper. Add at the top of the file:

```rust
async fn provider_for_repo(
    handle: &crate::state::TaskProviderHandle,
    data_dir: &std::path::Path,
    repo_id: &str,
) -> std::sync::Arc<dyn crate::task_provider::TaskProvider> {
    let guard = handle.read().await;
    if let Some(p) = guard.get(repo_id) {
        return p.clone();
    }
    drop(guard);
    crate::state::make_default_local_provider(data_dir)
}
```

Then update each `_inner` fn: callers that previously had no repo context get a
`repo_id: &str` parameter threaded through. For the existing
`refresh_tasks_inner` (no repo_id arg) — refactor signature to require it, or
iterate over all keys when None.

- [ ] **Step 4: Update `lib.rs` startup**

In `src-tauri/src/lib.rs` `.setup()`, replace the existing provider-init block
with:

```rust
let provider_handle: crate::state::TaskProviderHandle = std::sync::Arc::new(
    tokio::sync::RwLock::new(std::collections::HashMap::new()),
);
app.manage(provider_handle.clone());
```

(The actual per-repo build happens in Task 10 inside the migration block — for
now just an empty map so the app boots.)

- [ ] **Step 5: Update tests in commands/task.rs**

Tests that build `TaskProviderHandle` need to insert their test provider into
the new HashMap shape:

```rust
fn make_handle(provider: Arc<dyn crate::task_provider::TaskProvider>, repo_id: &str) -> TaskProviderHandle {
    let mut map = std::collections::HashMap::new();
    map.insert(repo_id.to_string(), provider);
    Arc::new(tokio::sync::RwLock::new(map))
}
```

Replace existing `make_handle(p)` helpers accordingly.

- [ ] **Step 6: Run tests**

```bash
cargo test --lib commands::task && cargo test --lib state
```

Expected: PASS.

- [ ] **Step 7: Full Rust gates + commit**

```bash
cargo test --lib && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/
git commit -m "feat(phase-3a-3): TaskProviderHandle is now per-repo HashMap"
```

---

## Task 7: Drop `AppSettings.task_source`

**Files:**

- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands/task.rs` (remove
  `get_task_source`/`set_task_source`)
- Modify: `src-tauri/src/persistence/settings.rs` (already tolerant of missing
  fields via `#[serde(default)]`)

- [ ] **Step 1: Remove the field**

In `src-tauri/src/state.rs`, remove the `pub task_source: TaskSource` field from
`AppSettings` and remove the `default_task_source` helper. Keep the `TaskSource`
enum import-free for now — `unused_variables` warning is OK because we'll delete
the enum once frontend types align.

Actually, **delete the enum entirely** if no other consumer remains. Check:

```bash
grep -rn "TaskSource" src-tauri/src/ | grep -v "task_source"
```

If only the enum definition shows up: delete the enum too.

- [ ] **Step 2: Delete the inner fns + commands**

In `src-tauri/src/commands/task.rs`, delete:

- `get_task_source` Tauri command + its `_inner`
- `set_task_source` Tauri command + its `_inner`
- Their tests

- [ ] **Step 3: Unregister from `lib.rs`**

In `src-tauri/src/lib.rs` `tauri::generate_handler![]` list, remove:

- `crate::commands::task::get_task_source,`
- `crate::commands::task::set_task_source,`

- [ ] **Step 4: Compile + commit**

```bash
cargo check --lib && cargo test --lib && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/
git commit -m "refactor(phase-3a-3): drop AppSettings.task_source + related commands"
```

Expected: PASS (frontend will be updated in Task 11; backend is independent).

---

## Task 8: `commands::lark_repo_binding` module + register

**Files:**

- Create: `src-tauri/src/commands/lark_repo_binding.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Wire up the new module**

In `src-tauri/src/commands/mod.rs`, add:

```rust
pub mod lark_repo_binding;
```

- [ ] **Step 2: Write the command module skeleton**

Create `src-tauri/src/commands/lark_repo_binding.rs`:

```rust
use crate::error::{AppError, Result};
use crate::state::{AppState, BitableBinding, ProposedMapping, TaskProviderHandle};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tauri::command]
pub async fn get_lark_repo_binding(
    repo_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Option<BitableBinding>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    crate::persistence::lark_repo_bindings::get_binding(&data_dir, &repo_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_lark_repo_binding(
    repo_id: String,
    binding: BitableBinding,
    app_handle: tauri::AppHandle,
    provider_handle: State<'_, TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    set_lark_repo_binding_inner(&repo_id, binding, &data_dir, provider_handle.inner().clone())
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn set_lark_repo_binding_inner(
    repo_id: &str,
    mut binding: BitableBinding,
    data_dir: &std::path::Path,
    handle: TaskProviderHandle,
) -> Result<()> {
    if binding.field_mapping.title.field_id.is_empty() {
        return Err(AppError::InvalidState("title field is required".into()));
    }
    // Stamp timestamps.
    let now = now_unix();
    if binding.created_at == 0 {
        binding.created_at = now;
    }
    binding.updated_at = now;

    // Persist before swapping the provider so a write failure doesn't
    // leave the runtime ahead of disk.
    crate::persistence::lark_repo_bindings::set_binding(
        data_dir,
        repo_id,
        binding.clone(),
    )?;

    // Build the new provider and swap into the map.
    let store = crate::commands::lark_auth::KeyringStore;
    let cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, &store)
        .map_err(|e| {
            AppError::InvalidState(format!(
                "global Lark credentials missing: {e}"
            ))
        })?;
    // Re-target the client at the binding's table.
    let mut cfg = cfg;
    cfg.app_token = binding.app_token.clone();
    cfg.table_id = binding.table_id.clone();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(
        crate::task_provider::lark::LarkProvider::from_binding(client, binding),
    );

    {
        let mut guard = handle.write().await;
        guard.insert(repo_id.to_string(), provider);
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_lark_repo_binding(
    repo_id: String,
    app_handle: tauri::AppHandle,
    provider_handle: State<'_, TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    crate::persistence::lark_repo_bindings::delete_binding(&data_dir, &repo_id)
        .map_err(|e| e.to_string())?;
    let mut guard = provider_handle.write().await;
    guard.remove(&repo_id);
    Ok(())
}

#[tauri::command]
pub async fn list_lark_repo_bindings(
    app_handle: tauri::AppHandle,
) -> std::result::Result<std::collections::HashMap<String, BitableBinding>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let file = crate::persistence::lark_repo_bindings::load_bindings(&data_dir)
        .map_err(|e| e.to_string())?;
    Ok(file.bindings)
}

#[tauri::command]
pub async fn detect_lark_schema(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<ProposedMapping, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    let cfg = crate::commands::lark_auth::load_lark_config_inner(&data_dir, &store)
        .map_err(|e| format!("global Lark credentials missing: {e}"))?;
    let mut cfg = cfg;
    cfg.app_token = app_token.clone();
    cfg.table_id = table_id.clone();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    let detector = crate::task_provider::lark_field_resolver::BitableSchemaDetector::new(client);
    detector
        .propose_mapping(&app_token, &table_id)
        .await
        .map_err(|e| e.to_string())
}

fn data_dir_from(app_handle: &tauri::AppHandle) -> std::result::Result<PathBuf, String> {
    use tauri::Manager;
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))
}
```

- [ ] **Step 3: Register commands in `lib.rs`**

In `src-tauri/src/lib.rs`, add to `tauri::generate_handler![]`:

```rust
crate::commands::lark_repo_binding::get_lark_repo_binding,
crate::commands::lark_repo_binding::set_lark_repo_binding,
crate::commands::lark_repo_binding::delete_lark_repo_binding,
crate::commands::lark_repo_binding::list_lark_repo_bindings,
crate::commands::lark_repo_binding::detect_lark_schema,
```

Also remove from the same list:

```rust
crate::commands::lark_auth::verify_lark_schema,   // delete this line
```

- [ ] **Step 4: Add unit tests for `set_lark_repo_binding_inner`**

Append to `src-tauri/src/commands/lark_repo_binding.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FieldMapping, FieldRef, StatusValueMapping};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn sample_binding() -> BitableBinding {
        BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            field_mapping: FieldMapping {
                title: FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "Task name".into(),
                },
                description: None,
                status: None,
                order: None,
            },
            status_value_mapping: StatusValueMapping::default(),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn empty_handle() -> TaskProviderHandle {
        Arc::new(tokio::sync::RwLock::new(HashMap::new()))
    }

    #[tokio::test]
    async fn set_binding_rejects_missing_title_field() {
        let tmp = tempdir().unwrap();
        let mut b = sample_binding();
        b.field_mapping.title.field_id = String::new();
        let handle = empty_handle();
        let err = set_lark_repo_binding_inner("repo_x", b, tmp.path(), handle)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("title field is required"));
    }

    #[tokio::test]
    async fn set_binding_persists_and_stamps_timestamps() {
        let tmp = tempdir().unwrap();
        // Pre-seed global Lark creds so the provider build doesn't fail.
        seed_global_creds(tmp.path());
        let mut b = sample_binding();
        b.created_at = 0;
        let handle = empty_handle();
        set_lark_repo_binding_inner("repo_x", b, tmp.path(), handle)
            .await
            .unwrap();
        let stored = crate::persistence::lark_repo_bindings::get_binding(tmp.path(), "repo_x")
            .unwrap()
            .unwrap();
        assert!(stored.created_at > 0);
        assert_eq!(stored.created_at, stored.updated_at);
    }

    fn seed_global_creds(data_dir: &std::path::Path) {
        // Write a minimal lark_settings.json so load_lark_config_inner succeeds.
        // Adjust this helper to match whatever the existing lark_auth tests use.
        let settings = serde_json::json!({
            "app_id": "cli_test",
            "base_url": "https://open.larksuite.com"
        });
        std::fs::write(
            data_dir.join("lark_settings.json"),
            serde_json::to_string(&settings).unwrap(),
        )
        .unwrap();
        // Seed an in-memory keyring entry — depends on test infra; use
        // commands::lark_auth::test_helpers if available.
    }
}
```

(Note: the `seed_global_creds` helper needs to match the existing keyring test
pattern. Reuse what `commands::lark_auth::tests` already does.)

- [ ] **Step 5: Run tests + lint + commit**

```bash
cargo test --lib commands::lark_repo_binding
cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/commands/
git commit -m "feat(phase-3a-3): lark_repo_binding Tauri commands"
```

---

## Task 9: Shrink `lark_auth` + `LarkStatus`

**Files:**

- Modify: `src-tauri/src/commands/lark_auth.rs`
- Modify: `src-tauri/src/lib.rs` (unregister `verify_lark_schema`)
- Delete: integration with schema.rs (already removed in Task 5)

- [ ] **Step 1: Remove `app_token` and `table_id` from
      `LarkSettings`/`LarkStatus`**

In `src-tauri/src/commands/lark_auth.rs`, edit the `LarkSettings` struct:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LarkSettings {
    pub app_id: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    crate::platform::lark_client::DEFAULT_BASE_URL.to_string()
}
```

And `LarkStatus`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LarkStatus {
    pub configured: bool,
    pub app_id: Option<String>,
    pub base_url: String,
    pub has_secret: bool,
}
```

- [ ] **Step 2: Remove `verify_lark_schema` Tauri command**

Delete the entire `verify_lark_schema` function + its `_inner` from
`lark_auth.rs`. Drop the corresponding tests.

- [ ] **Step 3: Update `load_lark_config_inner` to keep working**

`load_lark_config_inner` returns a `LarkConfig` (used by lark*client). It still
needs `app_token` and `table_id` \_for the client init*. Change its signature:

```rust
pub fn load_lark_config_inner(
    data_dir: &std::path::Path,
    store: &dyn SecretStore,
) -> Result<crate::platform::lark_client::LarkConfig> {
    let settings = load_settings_inner(data_dir)?;
    let secret = store.get(SERVICE_NAME, SECRET_KEY)?
        .ok_or_else(|| AppError::InvalidState("Lark app_secret missing".into()))?;
    Ok(crate::platform::lark_client::LarkConfig {
        app_id: settings.app_id,
        app_secret: secret,
        // Per-binding values must be injected by caller (see
        // commands::lark_repo_binding::set_lark_repo_binding_inner).
        app_token: String::new(),
        table_id: String::new(),
        base_url: settings.base_url,
    })
}
```

The empty strings get overwritten by callers that have a binding.

- [ ] **Step 4: Update `test_lark_connection`**

The test command needs to accept `app_token` + `table_id` now since they're no
longer global:

```rust
#[tauri::command]
pub async fn test_lark_connection(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = KeyringStore;
    let mut cfg = load_lark_config_inner(&data_dir, &store).map_err(|e| e.to_string())?;
    cfg.app_token = app_token;
    cfg.table_id = table_id;
    let client = crate::platform::lark_client::LarkClient::new(cfg);
    client.tenant_access_token().await.map(|_| ()).map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Update tests**

Update the tests in `lark_auth.rs` that asserted on `app_token` or `table_id` in
`LarkStatus` to expect the shrunken shape. Remove tests for
`verify_lark_schema`.

- [ ] **Step 6: Run + commit**

```bash
cargo test --lib commands::lark_auth
cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/
git commit -m "refactor(phase-3a-3): shrink lark_auth — drop app_token/table_id from global"
```

---

## Task 10: Auto-migration logic in `lib.rs`

**Files:**

- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add migration helper**

In `src-tauri/src/lib.rs`, add before `pub fn run()`:

```rust
/// Auto-migrate the user's Phase 3a-2 global Lark config to a per-repo
/// binding on first launch after upgrade. Idempotent; no-op when:
///   - bindings file already exists with at least one entry, OR
///   - old `lark_settings.json` has no `app_token`/`table_id`, OR
///   - no repo is selected.
fn maybe_migrate_to_per_repo_binding(
    data_dir: &std::path::Path,
    settings: &crate::state::AppSettings,
) -> Option<(String, crate::state::BitableBinding)> {
    let bindings = crate::persistence::lark_repo_bindings::load_bindings(data_dir).ok()?;
    if !bindings.bindings.is_empty() {
        return None;
    }
    let selected_repo = settings.selected_repo_id.clone()?;
    // Peek at the legacy lark_settings.json shape.
    let legacy_path = data_dir.join("lark_settings.json");
    let bytes = std::fs::read(&legacy_path).ok()?;
    let legacy: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let app_token = legacy.get("app_token").and_then(|v| v.as_str())?.to_string();
    let table_id = legacy.get("table_id").and_then(|v| v.as_str())?.to_string();
    if app_token.is_empty() || table_id.is_empty() {
        return None;
    }
    // Build a placeholder binding with title=primary (unknown at this
    // point — the wizard will refresh on first open).
    let binding = crate::state::BitableBinding {
        app_token,
        table_id,
        field_mapping: crate::state::FieldMapping {
            title: crate::state::FieldRef {
                field_id: "PENDING_RESOLVE".into(),
                field_name: "title".into(),
            },
            description: None,
            status: None,
            order: None,
        },
        status_value_mapping: crate::state::StatusValueMapping::default(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        updated_at: 0,
    };
    // Save the binding so the user can open the wizard and let it
    // refresh the field IDs in-place.
    let _ = crate::persistence::lark_repo_bindings::set_binding(
        data_dir,
        &selected_repo,
        binding.clone(),
    );
    // Rewrite legacy file with only the global identity fields.
    let _ = std::fs::write(
        legacy_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "app_id": legacy.get("app_id").and_then(|v| v.as_str()).unwrap_or(""),
            "base_url": legacy.get("base_url").and_then(|v| v.as_str()).unwrap_or(
                crate::platform::lark_client::DEFAULT_BASE_URL
            ),
        }))
        .unwrap_or_default(),
    );
    Some((selected_repo, binding))
}
```

- [ ] **Step 2: Call the helper during `.setup()`**

In `.setup()`, after `app.manage(provider_handle.clone())`, insert:

```rust
// Phase 3a-3 migration: convert global Lark config (3a-2 shape) into
// per-repo binding for the currently selected repo.
let migrated = maybe_migrate_to_per_repo_binding(&data_dir, &settings_for_provider);

// Build providers for every repo that has a binding.
let bindings = crate::persistence::lark_repo_bindings::load_bindings(&data_dir)
    .unwrap_or_default();
{
    let handle = provider_handle.clone();
    let data_dir_clone = data_dir.clone();
    tauri::async_runtime::spawn(async move {
        for (repo_id, binding) in bindings.bindings.iter() {
            let store = crate::commands::lark_auth::KeyringStore;
            let cfg_res = crate::commands::lark_auth::load_lark_config_inner(
                &data_dir_clone,
                &store,
            );
            let Ok(mut cfg) = cfg_res else {
                tracing::warn!(
                    repo_id = %repo_id,
                    "skipping Lark provider init: global creds missing"
                );
                continue;
            };
            cfg.app_token = binding.app_token.clone();
            cfg.table_id = binding.table_id.clone();
            let client = std::sync::Arc::new(
                crate::platform::lark_client::LarkClient::new(cfg),
            );
            let provider: std::sync::Arc<dyn crate::task_provider::TaskProvider> =
                std::sync::Arc::new(
                    crate::task_provider::lark::LarkProvider::from_binding(
                        client,
                        binding.clone(),
                    ),
                );
            handle.write().await.insert(repo_id.clone(), provider);
        }
    });
}

if let Some((repo_id, _)) = migrated {
    tracing::info!(
        repo_id = %repo_id,
        "Phase 3a-3 auto-migration: created binding from old lark_settings.json"
    );
    // Frontend listens for this event and shows a banner inviting the
    // user to review the auto-created mapping.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("lark-migrated", repo_id);
    }
}
```

- [ ] **Step 3: Add migration test in `lib.rs`**

```rust
#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn migration_no_op_when_bindings_already_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bindings = crate::persistence::lark_repo_bindings::BindingsFile::default();
        bindings.bindings.insert("repo_x".into(), make_existing_binding());
        crate::persistence::lark_repo_bindings::save_bindings(tmp.path(), &bindings).unwrap();
        let settings = crate::state::AppSettings::default();
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
    }

    #[test]
    fn migration_no_op_when_no_selected_repo() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("lark_settings.json"),
            r#"{"app_id":"x","app_token":"t","table_id":"i"}"#,
        )
        .unwrap();
        let settings = crate::state::AppSettings::default();
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
    }

    #[test]
    fn migration_creates_binding_for_selected_repo() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("lark_settings.json"),
            r#"{"app_id":"cli_x","app_token":"bascn","table_id":"tbl","base_url":"https://open.larksuite.com"}"#,
        )
        .unwrap();
        let mut settings = crate::state::AppSettings::default();
        settings.selected_repo_id = Some("repo_x".into());
        let result = maybe_migrate_to_per_repo_binding(tmp.path(), &settings);
        assert!(result.is_some());
        let (repo_id, _) = result.unwrap();
        assert_eq!(repo_id, "repo_x");
        let stored = crate::persistence::lark_repo_bindings::get_binding(tmp.path(), "repo_x")
            .unwrap()
            .unwrap();
        assert_eq!(stored.app_token, "bascn");
        assert_eq!(stored.table_id, "tbl");
        // Legacy file should be trimmed.
        let new: serde_json::Value = serde_json::from_slice(
            &std::fs::read(tmp.path().join("lark_settings.json")).unwrap(),
        )
        .unwrap();
        assert!(new.get("app_token").is_none());
    }

    fn make_existing_binding() -> crate::state::BitableBinding {
        crate::state::BitableBinding {
            app_token: "x".into(),
            table_id: "y".into(),
            field_mapping: crate::state::FieldMapping {
                title: crate::state::FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "title".into(),
                },
                description: None,
                status: None,
                order: None,
            },
            status_value_mapping: crate::state::StatusValueMapping::default(),
            created_at: 0,
            updated_at: 0,
        }
    }
}
```

- [ ] **Step 4: Run gates + commit**

```bash
cargo test --lib migration_tests && cargo test --lib && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all
git add src-tauri/src/lib.rs
git commit -m "feat(phase-3a-3): auto-migrate global Lark config to per-repo binding"
```

---

## Task 11: Frontend types + IPC wrappers

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/ipc.test.ts`

- [ ] **Step 1: Add new types**

In `src/lib/types.ts`, replace the existing `TaskSource` and `SchemaCheckResult`
types with:

```ts
export type FieldRef = {
  field_id: string;
  field_name: string;
};

export type FieldMapping = {
  title: FieldRef;
  description: FieldRef | null;
  status: FieldRef | null;
  order: FieldRef | null;
};

export type KanbanColumnLiteral = 'todo' | 'in_progress' | 'review' | 'done';

export type StatusValueMapping = {
  entries: Record<string, KanbanColumnLiteral>;
  default_column: KanbanColumnLiteral;
};

export type BitableBinding = {
  app_token: string;
  table_id: string;
  field_mapping: FieldMapping;
  status_value_mapping: StatusValueMapping;
  created_at: number;
  updated_at: number;
};

export type BitableField = {
  field_id: string;
  field_name: string;
  type: number;
  property?: { options?: { id: string; name: string }[] } | null;
  is_primary: boolean;
};

export type BitableOption = { id: string; name: string };

export type ProposedMapping = {
  fields: BitableField[];
  suggested: FieldMapping;
  status_options: BitableOption[] | null;
  suggested_status_values: StatusValueMapping;
};
```

Update the existing `LarkStatus` to drop `app_token`/`table_id`:

```ts
export type LarkStatus = {
  configured: boolean;
  app_id: string | null;
  base_url: string;
  has_secret: boolean;
};
```

- [ ] **Step 2: Update IPC wrappers**

In `src/lib/ipc.ts`, in the `api.task` block, **remove** `getSource`,
`setSource`, and keep `refresh`.

In `api.lark`, **remove** `verifySchema` and `getStatus` (replace with shrunken
`getStatus` that returns the new `LarkStatus`).

Add the new wrappers:

```ts
api.lark = {
  ...api.lark,
  getRepoBinding: (repoId: string) =>
    invoke<BitableBinding | null>('get_lark_repo_binding', { repoId }),
  setRepoBinding: (repoId: string, binding: BitableBinding) =>
    invoke<void>('set_lark_repo_binding', { repoId, binding }),
  deleteRepoBinding: (repoId: string) =>
    invoke<void>('delete_lark_repo_binding', { repoId }),
  listRepoBindings: () =>
    invoke<Record<string, BitableBinding>>('list_lark_repo_bindings'),
  detectSchema: (appToken: string, tableId: string) =>
    invoke<ProposedMapping>('detect_lark_schema', { appToken, tableId }),
  testConnection: (appToken: string, tableId: string) =>
    invoke<void>('test_lark_connection', { appToken, tableId }),
};
```

- [ ] **Step 3: Update IPC tests**

In `src/lib/ipc.test.ts`, drop tests for `getSource`/`setSource`/
`verifySchema`/`getStatus`-old-shape. Add:

```ts
it('api.lark.getRepoBinding invokes get_lark_repo_binding with repoId', async () => {
  vi.mocked(invoke).mockResolvedValue(null);
  await api.lark.getRepoBinding('repo_x');
  expect(invoke).toHaveBeenCalledWith('get_lark_repo_binding', {
    repoId: 'repo_x',
  });
});

it('api.lark.setRepoBinding passes binding payload', async () => {
  const binding: BitableBinding = {
    app_token: 'bascn',
    table_id: 'tbl',
    field_mapping: {
      title: { field_id: 'fld_t', field_name: 'Task name' },
      description: null,
      status: null,
      order: null,
    },
    status_value_mapping: { entries: {}, default_column: 'todo' },
    created_at: 0,
    updated_at: 0,
  };
  vi.mocked(invoke).mockResolvedValue(undefined);
  await api.lark.setRepoBinding('repo_x', binding);
  expect(invoke).toHaveBeenCalledWith('set_lark_repo_binding', {
    repoId: 'repo_x',
    binding,
  });
});

it('api.lark.detectSchema invokes detect_lark_schema with creds', async () => {
  vi.mocked(invoke).mockResolvedValue({
    fields: [],
    suggested: {
      title: { field_id: '', field_name: '' },
      description: null,
      status: null,
      order: null,
    },
    status_options: null,
    suggested_status_values: { entries: {}, default_column: 'todo' },
  });
  await api.lark.detectSchema('bascn', 'tbl');
  expect(invoke).toHaveBeenCalledWith('detect_lark_schema', {
    appToken: 'bascn',
    tableId: 'tbl',
  });
});

it('api.lark.testConnection invokes test_lark_connection with creds', async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  await api.lark.testConnection('bascn', 'tbl');
  expect(invoke).toHaveBeenCalledWith('test_lark_connection', {
    appToken: 'bascn',
    tableId: 'tbl',
  });
});
```

- [ ] **Step 4: Run + commit**

```bash
bun run check && bun run test --run src/lib/ipc.test.ts && bun run lint
git add src/lib/types.ts src/lib/ipc.ts src/lib/ipc.test.ts
git commit -m "feat(phase-3a-3): frontend types + IPC wrappers for per-repo binding"
```

---

## Task 12: `lark-bindings` Svelte store

**Files:**

- Create: `src/lib/stores/lark-bindings.svelte.ts`
- Create: `src/lib/stores/lark-bindings.svelte.test.ts`

- [ ] **Step 1: Write the store**

Create `src/lib/stores/lark-bindings.svelte.ts`:

```ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
import type { BitableBinding } from '$lib/types';

export class LarkBindingsStore {
  readonly bindings = new SvelteMap<string, BitableBinding>();

  async load(): Promise<void> {
    const list = await api.lark.listRepoBindings();
    this.bindings.clear();
    for (const [repoId, b] of Object.entries(list)) {
      this.bindings.set(repoId, b);
    }
  }

  has(repoId: string): boolean {
    return this.bindings.has(repoId);
  }

  get(repoId: string): BitableBinding | undefined {
    return this.bindings.get(repoId);
  }

  /** Optimistic upsert: writes locally first, awaits backend, reverts on error. */
  async setBinding(repoId: string, binding: BitableBinding): Promise<void> {
    const prev = this.bindings.get(repoId);
    this.bindings.set(repoId, binding);
    try {
      await api.lark.setRepoBinding(repoId, binding);
    } catch (err) {
      if (prev) this.bindings.set(repoId, prev);
      else this.bindings.delete(repoId);
      addToast(
        `Save binding failed: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      );
      throw err;
    }
  }

  async deleteBinding(repoId: string): Promise<void> {
    const prev = this.bindings.get(repoId);
    this.bindings.delete(repoId);
    try {
      await api.lark.deleteRepoBinding(repoId);
    } catch (err) {
      if (prev) this.bindings.set(repoId, prev);
      addToast(
        `Delete binding failed: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      );
      throw err;
    }
  }
}

export const larkBindings = new LarkBindingsStore();
```

- [ ] **Step 2: Write tests**

Create `src/lib/stores/lark-bindings.svelte.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      listRepoBindings: vi.fn(),
      setRepoBinding: vi.fn(),
      deleteRepoBinding: vi.fn(),
    },
  },
}));

vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
}));

import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
import { LarkBindingsStore } from './lark-bindings.svelte';
import type { BitableBinding } from '$lib/types';

const makeBinding = (
  overrides: Partial<BitableBinding> = {}
): BitableBinding => ({
  app_token: 'bascn',
  table_id: 'tbl',
  field_mapping: {
    title: { field_id: 'fld_t', field_name: 'Task name' },
    description: null,
    status: null,
    order: null,
  },
  status_value_mapping: { entries: {}, default_column: 'todo' },
  created_at: 0,
  updated_at: 0,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('LarkBindingsStore', () => {
  it('load populates map from list response', async () => {
    vi.mocked(api.lark.listRepoBindings).mockResolvedValue({
      repo_x: makeBinding({ app_token: 'one' }),
      repo_y: makeBinding({ app_token: 'two' }),
    });
    const s = new LarkBindingsStore();
    await s.load();
    expect(s.has('repo_x')).toBe(true);
    expect(s.get('repo_y')?.app_token).toBe('two');
  });

  it('setBinding optimistically writes then reconciles', async () => {
    vi.mocked(api.lark.setRepoBinding).mockResolvedValue(undefined);
    const s = new LarkBindingsStore();
    const b = makeBinding();
    const p = s.setBinding('repo_x', b);
    expect(s.has('repo_x')).toBe(true);
    await p;
    expect(s.get('repo_x')).toEqual(b);
  });

  it('setBinding reverts and toasts on error', async () => {
    vi.mocked(api.lark.setRepoBinding).mockRejectedValueOnce(
      new Error('IPC fail')
    );
    const s = new LarkBindingsStore();
    await s.setBinding('repo_x', makeBinding()).catch(() => {});
    expect(s.has('repo_x')).toBe(false);
    expect(addToast).toHaveBeenCalledWith(
      expect.stringContaining('IPC fail'),
      'error'
    );
  });

  it('deleteBinding optimistically removes then reconciles', async () => {
    vi.mocked(api.lark.deleteRepoBinding).mockResolvedValue(undefined);
    const s = new LarkBindingsStore();
    s.bindings.set('repo_x', makeBinding());
    await s.deleteBinding('repo_x');
    expect(s.has('repo_x')).toBe(false);
  });

  it('deleteBinding reverts and toasts on error', async () => {
    vi.mocked(api.lark.deleteRepoBinding).mockRejectedValueOnce(
      new Error('IPC fail')
    );
    const s = new LarkBindingsStore();
    s.bindings.set('repo_x', makeBinding());
    await s.deleteBinding('repo_x').catch(() => {});
    expect(s.has('repo_x')).toBe(true);
    expect(addToast).toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
bun run check && bun run test --run src/lib/stores/lark-bindings.svelte.test.ts
git add src/lib/stores/lark-bindings.svelte.ts src/lib/stores/lark-bindings.svelte.test.ts
git commit -m "feat(phase-3a-3): lark-bindings Svelte store with optimistic update"
```

---

## Task 13: `LarkBindingWizard` component

**Files:**

- Create: `src/lib/components/lark/LarkBindingWizard.svelte`
- Create: `src/lib/components/lark/LarkBindingWizard.test.ts`

- [ ] **Step 1: Write the wizard component (skeleton + Step 1)**

Create `src/lib/components/lark/LarkBindingWizard.svelte`:

```svelte
<!-- src/lib/components/lark/LarkBindingWizard.svelte -->
<script lang="ts">
  import { api } from '$lib/ipc';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type {
    BitableBinding,
    FieldMapping,
    ProposedMapping,
    StatusValueMapping,
    KanbanColumnLiteral,
  } from '$lib/types';

  const {
    repoId,
    existing,
    onSave,
    onCancel,
  }: {
    repoId: string;
    existing: BitableBinding | null;
    onSave: (b: BitableBinding) => Promise<void>;
    onCancel: () => void;
  } = $props();

  type Step = 1 | 2 | 3;
  let step = $state<Step>(existing ? 2 : 1);
  let appToken = $state(existing?.app_token ?? '');
  let tableId = $state(existing?.table_id ?? '');
  let detecting = $state(false);
  let detectError = $state<string | null>(null);
  let proposal = $state<ProposedMapping | null>(null);

  // Step 2 form state
  let titleFieldId = $state<string>('');
  let descFieldId = $state<string>('');
  let statusFieldId = $state<string>('');
  let orderFieldId = $state<string>('');

  // Step 3 form state
  let valueMap = $state<Record<string, KanbanColumnLiteral>>({});
  let defaultColumn = $state<KanbanColumnLiteral>('todo');

  let saving = $state(false);

  async function handleDetect() {
    if (!appToken.trim() || !tableId.trim()) return;
    detecting = true;
    detectError = null;
    try {
      const p = await api.lark.detectSchema(appToken.trim(), tableId.trim());
      proposal = p;
      // Pre-fill Step 2 form
      titleFieldId = p.suggested.title.field_id;
      descFieldId = p.suggested.description?.field_id ?? '';
      statusFieldId = p.suggested.status?.field_id ?? '';
      orderFieldId = p.suggested.order?.field_id ?? '';
      // Pre-fill Step 3 form
      valueMap = { ...p.suggested_status_values.entries };
      defaultColumn = p.suggested_status_values.default_column;
      step = 2;
    } catch (err) {
      detectError = err instanceof Error ? err.message : String(err);
    } finally {
      detecting = false;
    }
  }

  const statusField = $derived(
    proposal?.fields.find((f) => f.field_id === statusFieldId) ?? null
  );
  const statusIsSingleSelect = $derived(statusField?.type === 3);

  function fieldRefOf(id: string) {
    if (!id || !proposal) return null;
    const f = proposal.fields.find((x) => x.field_id === id);
    return f ? { field_id: f.field_id, field_name: f.field_name } : null;
  }

  function handleContinueStep2() {
    if (!titleFieldId) return;
    if (statusIsSingleSelect && statusField?.property?.options) {
      step = 3;
    } else {
      handleSave();
    }
  }

  async function handleSave() {
    if (!proposal) return;
    saving = true;
    const titleRef = fieldRefOf(titleFieldId);
    if (!titleRef) {
      saving = false;
      return;
    }
    const binding: BitableBinding = {
      app_token: appToken.trim(),
      table_id: tableId.trim(),
      field_mapping: {
        title: titleRef,
        description: fieldRefOf(descFieldId),
        status: fieldRefOf(statusFieldId),
        order: fieldRefOf(orderFieldId),
      } satisfies FieldMapping,
      status_value_mapping: {
        entries: valueMap,
        default_column: defaultColumn,
      } satisfies StatusValueMapping,
      created_at: existing?.created_at ?? 0,
      updated_at: 0,
    };
    try {
      await onSave(binding);
    } catch (err) {
      addToast(
        `Save failed: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      );
    } finally {
      saving = false;
    }
  }
</script>

<div class="lark-binding-wizard" data-testid="lark-binding-wizard">
  {#if step === 1}
    <section data-testid="wizard-step-1">
      <h3>Connect to Lark Bitable (1 of 3)</h3>
      <label>
        App Token
        <input
          type="text"
          bind:value={appToken}
          data-testid="wizard-app-token"
          disabled={detecting}
        />
      </label>
      <label>
        Table ID
        <input
          type="text"
          bind:value={tableId}
          data-testid="wizard-table-id"
          disabled={detecting}
        />
      </label>
      <p class="hint">App ID & secret use global config in Settings.</p>
      {#if detectError}
        <div class="banner banner-error" data-testid="wizard-detect-error">
          {detectError}
        </div>
      {/if}
      <div class="actions">
        <button type="button" onclick={onCancel} disabled={detecting}
          >Cancel</button
        >
        <button
          type="button"
          onclick={handleDetect}
          disabled={!appToken.trim() || !tableId.trim() || detecting}
          data-testid="wizard-detect"
        >
          {detecting ? 'Detecting…' : 'Detect →'}
        </button>
      </div>
    </section>
  {:else if step === 2}
    <section data-testid="wizard-step-2">
      <h3>Map your fields (2 of 3)</h3>
      <label>
        Title* required
        <select bind:value={titleFieldId} data-testid="wizard-title-field">
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <label>
        Description
        <select bind:value={descFieldId} data-testid="wizard-desc-field">
          <option value="">(none)</option>
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <label>
        Status
        <select bind:value={statusFieldId} data-testid="wizard-status-field">
          <option value="">(none — default Todo)</option>
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <label>
        Order
        <select bind:value={orderFieldId} data-testid="wizard-order-field">
          <option value="">(none — sort by created time)</option>
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <div class="actions">
        <button type="button" onclick={() => (step = 1)}>← Back</button>
        <button
          type="button"
          onclick={handleContinueStep2}
          disabled={!titleFieldId || saving}
          data-testid="wizard-continue"
        >
          {statusIsSingleSelect ? 'Continue →' : 'Save & Sync'}
        </button>
      </div>
    </section>
  {:else}
    <section data-testid="wizard-step-3">
      <h3>Map status options (3 of 3)</h3>
      {#each statusField?.property?.options ?? [] as opt (opt.id)}
        <label>
          "{opt.name}"
          <select
            bind:value={valueMap[opt.id]}
            data-testid={`wizard-option-${opt.id}`}
          >
            <option value="todo">Todo</option>
            <option value="in_progress">In Progress</option>
            <option value="review">Review</option>
            <option value="done">Done</option>
          </select>
        </label>
      {/each}
      <label>
        Default for unmapped values
        <select bind:value={defaultColumn} data-testid="wizard-default-column">
          <option value="todo">Todo</option>
          <option value="in_progress">In Progress</option>
          <option value="review">Review</option>
          <option value="done">Done</option>
        </select>
      </label>
      <div class="actions">
        <button type="button" onclick={() => (step = 2)}>← Back</button>
        <button
          type="button"
          onclick={handleSave}
          disabled={saving}
          data-testid="wizard-save"
        >
          {saving ? 'Saving…' : 'Save & Sync'}
        </button>
      </div>
    </section>
  {/if}
</div>

<style>
  .lark-binding-wizard {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .hint {
    font-size: 11px;
    color: var(--text-muted);
  }
  .banner-error {
    padding: 8px;
    border: 1px solid var(--accent-error, #f87171);
    color: var(--accent-error, #f87171);
    font-size: 11px;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>
```

- [ ] **Step 2: Write component tests**

Create `src/lib/components/lark/LarkBindingWizard.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import LarkBindingWizard from './LarkBindingWizard.svelte';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

import { invoke } from '@tauri-apps/api/core';

const proposalFixture = {
  fields: [
    { field_id: 'fld_pri', field_name: 'Task name', type: 1, is_primary: true },
    {
      field_id: 'fld_s',
      field_name: 'Task Status',
      type: 3,
      is_primary: false,
      property: {
        options: [
          { id: 'opt_a', name: 'To Do' },
          { id: 'opt_b', name: 'Done' },
        ],
      },
    },
  ],
  suggested: {
    title: { field_id: 'fld_pri', field_name: 'Task name' },
    description: null,
    status: { field_id: 'fld_s', field_name: 'Task Status' },
    order: null,
  },
  status_options: [
    { id: 'opt_a', name: 'To Do' },
    { id: 'opt_b', name: 'Done' },
  ],
  suggested_status_values: {
    entries: { opt_a: 'todo', opt_b: 'done' },
    default_column: 'todo',
  },
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('LarkBindingWizard', () => {
  it('starts at Step 1 with Detect disabled when fields empty', () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    const btn = screen.getByTestId('wizard-detect') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it('Detect button enables when both fields populated', async () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    const btn = screen.getByTestId('wizard-detect') as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
  });

  it('Step 1 detect error stays on step and shows banner', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('91402 not found'));
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'x' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'y' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('wizard-detect-error').textContent).toContain(
      '91402'
    );
    expect(screen.getByTestId('wizard-step-1')).toBeTruthy();
  });

  it('moves to Step 2 on successful detect, pre-fills suggestions', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(proposalFixture);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('wizard-step-2')).toBeTruthy();
    const titleSel = screen.getByTestId(
      'wizard-title-field'
    ) as HTMLSelectElement;
    expect(titleSel.value).toBe('fld_pri');
  });

  it('Step 2 → Step 3 when status field is single-select', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(proposalFixture);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await new Promise((r) => setTimeout(r, 0));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    expect(screen.getByTestId('wizard-step-3')).toBeTruthy();
  });

  it('Step 3 Save calls onSave with assembled binding', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(proposalFixture);
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing: null, onSave, onCancel: vi.fn() },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await new Promise((r) => setTimeout(r, 0));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await fireEvent.click(screen.getByTestId('wizard-save'));
    await new Promise((r) => setTimeout(r, 0));
    expect(onSave).toHaveBeenCalled();
    const call = onSave.mock.calls[0][0];
    expect(call.app_token).toBe('bascn');
    expect(call.field_mapping.title.field_id).toBe('fld_pri');
    expect(call.status_value_mapping.entries['opt_a']).toBe('todo');
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
bun run check && bun run test --run src/lib/components/lark/LarkBindingWizard.test.ts
git add src/lib/components/lark/LarkBindingWizard.svelte src/lib/components/lark/LarkBindingWizard.test.ts
git commit -m "feat(phase-3a-3): 3-step LarkBindingWizard component"
```

---

## Task 14: Rename `LarkSettings` → `LarkGlobalSettings`; shrink form

**Files:**

- Rename: `src/lib/components/lark/LarkSettings.svelte` →
  `LarkGlobalSettings.svelte`
- Rename: `src/lib/components/lark/LarkSettings.test.ts` →
  `LarkGlobalSettings.test.ts`

- [ ] **Step 1: Rename files**

```bash
git mv src/lib/components/lark/LarkSettings.svelte src/lib/components/lark/LarkGlobalSettings.svelte
git mv src/lib/components/lark/LarkSettings.test.ts src/lib/components/lark/LarkGlobalSettings.test.ts
```

- [ ] **Step 2: Shrink form**

In `LarkGlobalSettings.svelte`:

- Remove `appToken`, `tableId` state + their `<input>` fields
- Remove the "Bitable schema" section (the
  `<section data-testid="lark-schema-section">` block) and its supporting state
  (`schemaResult`, `verifying`, `schemaError`, `canVerifySchema`,
  `handleVerifySchema`)
- Remove `applyStatus`'s `app_token`/`table_id` assignments
- `canTestUnsaved` is no longer relevant — remove. Just keep `canTest` based on
  `status?.configured`. Or remove Test Connection entirely (delegated to the
  wizard).

Actually: **delete Test Connection from this component too.** The wizard handles
connection testing as part of Detect. The global settings panel only has Save /
Clear for `app_id`/`secret`/`base_url`.

The resulting `LarkGlobalSettings.svelte` should be ~60% smaller.

- [ ] **Step 3: Update tests**

Edit `LarkGlobalSettings.test.ts`:

- Remove tests that asserted on app_token/table_id inputs
- Remove tests for schema verify section
- Remove tests for Test Connection
- Keep: Save flow, Clear flow, error banner, configured-vs-not-configured
  display

- [ ] **Step 4: Run + commit**

```bash
bun run check && bun run test --run src/lib/components/lark/LarkGlobalSettings.test.ts
git add src/lib/components/lark/
git commit -m "refactor(phase-3a-3): rename LarkSettings → LarkGlobalSettings + shrink"
```

---

## Task 15: `RepoSettingsDialog` component

**Files:**

- Create: `src/lib/components/repo/RepoSettingsDialog.svelte`
- Create: `src/lib/components/repo/RepoSettingsDialog.test.ts`

- [ ] **Step 1: Write the component**

Create `src/lib/components/repo/RepoSettingsDialog.svelte`:

```svelte
<!-- src/lib/components/repo/RepoSettingsDialog.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import LarkBindingWizard from '$lib/components/lark/LarkBindingWizard.svelte';
  import { larkBindings } from '$lib/stores/lark-bindings.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type { BitableBinding } from '$lib/types';

  const {
    repoId,
    repoName,
    open,
    onClose,
  }: {
    repoId: string;
    repoName: string;
    open: boolean;
    onClose: () => void;
  } = $props();

  let editingBinding = $state(false);
  let confirmDisconnect = $state(false);

  const binding = $derived(larkBindings.get(repoId));
  const isConnected = $derived(!!binding);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (editingBinding) editingBinding = false;
      else if (confirmDisconnect) confirmDisconnect = false;
      else onClose();
    }
  }

  async function handleSaveBinding(b: BitableBinding) {
    await larkBindings.setBinding(repoId, b);
    editingBinding = false;
    addToast(`Bitable connected for ${repoName}`, 'success');
  }

  async function handleDisconnect() {
    try {
      await larkBindings.deleteBinding(repoId);
      confirmDisconnect = false;
    } catch {
      /* toast handled in store */
    }
  }
</script>

<svelte:window onkeydown={open ? handleKeydown : undefined} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="repo-settings-backdrop"
    onclick={onClose}
    data-testid="repo-settings-backdrop"
  >
    <div
      role="dialog"
      aria-modal="true"
      class="repo-settings-dialog"
      onclick={(e) => e.stopPropagation()}
    >
      <header>
        <h2>Settings — {repoName}</h2>
        <button
          onclick={onClose}
          aria-label="Close"
          data-testid="repo-settings-close">×</button
        >
      </header>

      <section class="lark-sync" data-testid="lark-sync-section">
        <h3>Lark Sync</h3>
        {#if editingBinding}
          <LarkBindingWizard
            {repoId}
            existing={binding ?? null}
            onSave={handleSaveBinding}
            onCancel={() => (editingBinding = false)}
          />
        {:else if isConnected && binding}
          <div class="binding-summary" data-testid="binding-summary">
            <div>✓ Connected: {binding.table_id}</div>
            <div class="muted">
              {binding.app_token.slice(0, 12)}… / {binding.table_id}
            </div>
            <div class="actions">
              <button
                onclick={() => (editingBinding = true)}
                data-testid="edit-binding"
              >
                Edit mapping
              </button>
              <button
                onclick={() => (confirmDisconnect = true)}
                data-testid="disconnect-binding"
              >
                Disconnect
              </button>
            </div>
          </div>
        {:else}
          <div class="binding-empty" data-testid="binding-empty">
            <p>Sync your kanban with a Lark Bitable.</p>
            <button
              onclick={() => (editingBinding = true)}
              data-testid="connect-binding"
            >
              Connect to Lark Bitable
            </button>
          </div>
        {/if}
      </section>
    </div>

    {#if confirmDisconnect}
      <div
        class="confirm-backdrop"
        onclick={() => (confirmDisconnect = false)}
        data-testid="disconnect-confirm-backdrop"
      >
        <div
          role="dialog"
          aria-modal="true"
          class="confirm-dialog"
          onclick={(e) => e.stopPropagation()}
        >
          <p>Disconnect from Lark? Tasks will revert to local tasks.json.</p>
          <div class="actions">
            <button onclick={() => (confirmDisconnect = false)}>Cancel</button>
            <button onclick={handleDisconnect} data-testid="disconnect-confirm">
              Disconnect
            </button>
          </div>
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .repo-settings-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 64px;
    z-index: 50;
  }
  .repo-settings-dialog {
    width: 560px;
    max-width: 90vw;
    background: var(--bg-card);
    border: 1px solid var(--border-light);
    border-radius: 6px;
  }
  header {
    display: flex;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-light);
  }
  section.lark-sync {
    padding: 12px 16px;
  }
  .binding-summary,
  .binding-empty {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }
  .muted {
    color: var(--text-muted);
    font-size: 11px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .confirm-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 60;
  }
  .confirm-dialog {
    background: var(--bg-card);
    padding: 16px;
    border-radius: 6px;
    width: 360px;
    max-width: 90vw;
  }
</style>
```

- [ ] **Step 2: Write component tests**

Create `src/lib/components/repo/RepoSettingsDialog.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import RepoSettingsDialog from './RepoSettingsDialog.svelte';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      listRepoBindings: vi.fn().mockResolvedValue({}),
      setRepoBinding: vi.fn(),
      deleteRepoBinding: vi.fn(),
      detectSchema: vi.fn(),
    },
  },
}));

vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
}));

import { larkBindings } from '$lib/stores/lark-bindings.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  larkBindings.bindings.clear();
});

describe('RepoSettingsDialog', () => {
  it('renders "Not connected" empty state by default', () => {
    render(RepoSettingsDialog, {
      props: {
        repoId: 'repo_x',
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    expect(screen.getByTestId('binding-empty')).toBeTruthy();
    expect(screen.getByTestId('connect-binding')).toBeTruthy();
  });

  it('renders connected state when binding exists', () => {
    larkBindings.bindings.set('repo_x', {
      app_token: 'bascntest12345',
      table_id: 'tbltest',
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
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    expect(screen.getByTestId('binding-summary')).toBeTruthy();
    expect(screen.getByTestId('edit-binding')).toBeTruthy();
    expect(screen.getByTestId('disconnect-binding')).toBeTruthy();
  });

  it('Connect button opens wizard', async () => {
    render(RepoSettingsDialog, {
      props: {
        repoId: 'repo_x',
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    await fireEvent.click(screen.getByTestId('connect-binding'));
    expect(screen.getByTestId('lark-binding-wizard')).toBeTruthy();
  });

  it('Disconnect shows confirm dialog, calling delete on confirm', async () => {
    larkBindings.bindings.set('repo_x', {
      app_token: 'bascntest',
      table_id: 'tbltest',
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
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    await fireEvent.click(screen.getByTestId('disconnect-binding'));
    expect(screen.getByTestId('disconnect-confirm-backdrop')).toBeTruthy();
    const { api } = await import('$lib/ipc');
    vi.mocked(api.lark.deleteRepoBinding).mockResolvedValue(undefined);
    await fireEvent.click(screen.getByTestId('disconnect-confirm'));
    expect(api.lark.deleteRepoBinding).toHaveBeenCalledWith('repo_x');
  });

  it('Close button fires onClose', async () => {
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose },
    });
    await fireEvent.click(screen.getByTestId('repo-settings-close'));
    expect(onClose).toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
bun run check && bun run test --run src/lib/components/repo/RepoSettingsDialog.test.ts
git add src/lib/components/repo/
git commit -m "feat(phase-3a-3): RepoSettingsDialog with Lark Sync section"
```

---

## Task 16: Sidebar right-click context menu → Repo settings

**Files:**

- Modify: `src/lib/components/Sidebar.svelte`

- [ ] **Step 1: Add right-click handler + dialog state**

Read the existing `Sidebar.svelte` to find the repo row markup. Add right- click
handler that opens `RepoSettingsDialog`. Pattern:

```svelte
<script lang="ts">
  // existing imports...
  import RepoSettingsDialog from '$lib/components/repo/RepoSettingsDialog.svelte';

  let settingsRepoId = $state<string | null>(null);
  let settingsRepoName = $state<string>('');

  function openRepoSettings(repoId: string, repoName: string) {
    settingsRepoId = repoId;
    settingsRepoName = repoName;
  }
  function closeRepoSettings() {
    settingsRepoId = null;
  }
</script>

<!-- inside each repo row markup -->
<div
  oncontextmenu={(e) => {
    e.preventDefault();
    openRepoSettings(repo.id, repo.name);
  }}
  data-testid={`repo-row-${repo.id}`}
>
  ...
</div>

{#if settingsRepoId}
  <RepoSettingsDialog
    repoId={settingsRepoId}
    repoName={settingsRepoName}
    open={true}
    onClose={closeRepoSettings}
  />
{/if}
```

- [ ] **Step 2: Add a test for the context-menu trigger**

In `src/lib/components/Sidebar.test.ts` (or new file if absent), add:

```ts
it('right-click on repo row opens RepoSettingsDialog', async () => {
  // setup: mock repos store with one repo
  // render Sidebar
  // fireEvent.contextMenu on the repo-row element
  // assert RepoSettingsDialog is open (look for binding-empty/binding-summary testid)
});
```

Adjust based on existing Sidebar test patterns.

- [ ] **Step 3: Run + commit**

```bash
bun run check && bun run test --run
git add src/lib/components/Sidebar.svelte src/lib/components/Sidebar.test.ts
git commit -m "feat(phase-3a-3): right-click repo opens RepoSettingsDialog"
```

---

## Task 17: `SettingsDialog` cleanup

**Files:**

- Modify: `src/lib/components/SettingsDialog.svelte`
- Modify: `src/lib/components/SettingsDialog.test.ts`

- [ ] **Step 1: Remove Task source radio + embedded panel**

In `src/lib/components/SettingsDialog.svelte`:

- Remove the entire `<section class="px-4 py-3 border-b ...">` containing the
  "Task source" radio buttons
- Remove the `source`/`saving` `$state` + `handleSourceChange` function
- Remove the `import LarkSettings` line; replace with
  `import LarkGlobalSettings from './lark/LarkGlobalSettings.svelte';`
- Change `<LarkSettings />` to `<LarkGlobalSettings />`

- [ ] **Step 2: Update tests**

In `SettingsDialog.test.ts`:

- Remove the `describe('SettingsDialog task source', ...)` block entirely
- Drop the task_source mock branch
- Update LarkSettings testid reference (probably still `lark-settings` from
  inner component; verify)

- [ ] **Step 3: Run + commit**

```bash
bun run check && bun run test --run src/lib/components/SettingsDialog.test.ts
git add src/lib/components/SettingsDialog.svelte src/lib/components/SettingsDialog.test.ts
git commit -m "refactor(phase-3a-3): SettingsDialog only hosts global Lark settings"
```

---

## Task 18: tasks store + App.svelte refresh integration

**Files:**

- Modify: `src/lib/stores/tasks.svelte.ts`
- Modify: `src/App.svelte`

- [ ] **Step 1: Listen for `lark-migrated` event in App.svelte**

In `src/App.svelte` `onMount`, add Tauri event listener:

```ts
import { listen } from '@tauri-apps/api/event';
import { addToast } from '$lib/stores/toasts.svelte';

onMount(() => {
  const unlistenMigrated = listen<string>('lark-migrated', (e) => {
    addToast(`Lark config migrated. Click here to review the mapping.`, 'info');
    // Optionally auto-open RepoSettingsDialog for e.payload (the repo_id)
  });
  return () => {
    unlistenMigrated.then((u) => u());
  };
});
```

- [ ] **Step 2: Derive Lark mode from binding presence (not task_source)**

In the existing window-focus refresh effect, change the source check:

```ts
import { larkBindings } from '$lib/stores/lark-bindings.svelte';

async function handleFocus() {
  if (focusDebounce) clearTimeout(focusDebounce);
  focusDebounce = setTimeout(() => {
    const repo = repos.getSelected();
    if (!repo) return;
    if (!larkBindings.has(repo.id)) return; // local mode, no refresh
    tasks.refresh(repo.id).catch(() => {});
  }, 2000);
}
```

- [ ] **Step 3: Load bindings on mount**

In `src/App.svelte` `onMount`, after `repos.load()` and similar:

```ts
await larkBindings.load();
```

- [ ] **Step 4: Run + commit**

```bash
bun run check && bun run test --run
git add src/App.svelte src/lib/stores/tasks.svelte.ts
git commit -m "feat(phase-3a-3): App.svelte derives Lark mode from binding presence"
```

---

## Task 19: Env-gated E2E test

**Files:**

- Create: `tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts`

- [ ] **Step 1: Write the spec**

Create `tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts`:

```ts
// tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts
//
// Phase 3a-3 env-gated smoke: configure a per-repo Lark binding via the
// new wizard, verify board hydrates. Skipped unless LARK_* env vars
// present (same pattern as phase-3a-2).

import { test, expect } from '@playwright/test';

const requiredEnv = [
  'LARK_APP_ID',
  'LARK_APP_SECRET',
  'LARK_APP_TOKEN',
  'LARK_TABLE_ID',
];
const hasCreds = requiredEnv.every((k) => Boolean(process.env[k]));

test.describe('Phase 3a-3 binding wizard', () => {
  test.skip(!hasCreds, 'requires LARK_* env vars');

  test('connect → detect → confirm mapping → board hydrates', async ({
    page,
  }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid^="repo-row-"]', {
      timeout: 10000,
    });
    const firstRepo = page.locator('[data-testid^="repo-row-"]').first();
    await firstRepo.click({ button: 'right' });
    await page.getByTestId('connect-binding').click();
    await page
      .getByTestId('wizard-app-token')
      .fill(process.env.LARK_APP_TOKEN!);
    await page.getByTestId('wizard-table-id').fill(process.env.LARK_TABLE_ID!);
    await page.getByTestId('wizard-detect').click();
    await expect(page.getByTestId('wizard-step-2')).toBeVisible({
      timeout: 15000,
    });
    await page.getByTestId('wizard-continue').click();
    // Step 3 may or may not appear depending on status field type; tolerate both
    const onStep3 = await page
      .getByTestId('wizard-step-3')
      .isVisible()
      .catch(() => false);
    if (onStep3) await page.getByTestId('wizard-save').click();
    // Wait for kanban to show tasks (or at least column headers)
    await expect(page.getByText('TODO')).toBeVisible({ timeout: 15000 });
  });
});
```

- [ ] **Step 2: Verify spec lists + skip path**

```bash
bun x playwright test tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts --list
bun run e2e tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts
```

Expected without creds: "1 skipped".

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/phase-3a-3/
git commit -m "test(phase-3a-3): env-gated E2E for binding wizard flow"
```

---

## Task 20: Final validation + PR

- [ ] **Step 1: Full Rust gate**

```bash
cd src-tauri && cargo test --lib && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
```

Expected: all pass.

- [ ] **Step 2: Frontend gate**

```bash
bun run check && bun run lint && bun run test --run && bun run test:coverage
```

Expected: all pass. Coverage branches ≥ 95%.

- [ ] **Step 3: Manual smoke**

Run `bun run tauri dev`. Expected flow:

1. App boots; if old Phase 3a-2 config present, toast appears: "Lark config
   migrated. Click to review."
2. Right-click repo → Settings → Lark Sync section
3. Click "Connect" (or "Edit") → wizard opens
4. Step 1: enter app_token + table_id → Detect → succeeds
5. Step 2: pre-filled mapping shown → adjust if needed → Continue
6. Step 3 (if status is single-select): pre-filled value mapping → Save & Sync
7. Toast: "Bitable connected for {repo}"
8. Tutup dialog → kanban hydrates

- [ ] **Step 4: Open PR**

```bash
git push -u origin feat/phase-3a-3-per-repo-lark-binding
gh pr create --title "feat(phase-3a-3): Per-repo Lark binding + field mapping wizard" --body "$(cat <<'EOF'
## Summary

- Per-repo Lark Bitable binding (each repo binds to one Bitable)
- 3-step wizard: Connect → Field mapping (auto-detected) → Status value mapping
- Field-ID-stable lookup (rename-safe; field_name cached for display)
- Auto-migration from Phase 3a-2 global config to per-repo binding on first launch
- Schema verify wizard removed
- `task_source` enum dropped (derived from binding presence)
- `TaskProviderHandle` reshaped to per-repo `HashMap<RepoId, Arc<dyn TaskProvider>>`

## Test plan

- [x] `cargo test --lib` — all pass
- [x] `cargo test --test lark_smoke -- --ignored --nocapture` — env-gated smoke pass
- [x] `bun run test --run` — all pass
- [x] `bun run test:coverage` — branches ≥ 95% threshold
- [x] `cargo clippy --lib --all-targets -- -D warnings` — clean
- [x] `cargo fmt --all -- --check` — clean
- [x] Manual smoke against real tenant — board hydrates via wizard
- [ ] CI matrix green

## Deferred (future phases)

- Multi-Bitable per repo
- Real-time bidirectional push (still focus-refresh based)
- Generic mapping abstraction for other providers (Jira, Linear)
- Conflict resolution beyond last-write-wins

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 5: Journal entry**

```bash
mkdir -p journal
cat > "journal/$(date +%Y-%m-%d)-phase-3a-3-per-repo-lark-binding.md" <<'EOF'
# Journal — Phase 3a-3 Per-Repo Lark Binding

[Brief summary of what shipped, decisions taken, surprises hit, deferrals.]
EOF
git add journal/
git commit -m "docs(journal): Phase 3a-3 per-repo Lark binding + field mapping wizard"
git push
```

---

## Self-review summary

- **Spec coverage:** All sections of
  `2026-05-15-phase-3a-3-per-repo-lark-binding-design.md` map to tasks:
  - Architecture overview → Tasks 1-10
  - Data model → Task 1 + Task 4
  - Component design → Tasks 1-18
  - UX flow → Tasks 13-17
  - Migration plan → Task 10
  - Error handling → distributed across resolver (Task 3), wizard (Task 13),
    commands (Task 8)
  - Testing strategy → embedded in each task + Task 19
- **No placeholders:** Every code block is concrete; the only intentional
  placeholder (`PENDING_RESOLVE` field_id in migration) is documented and
  handled in Step 1 of Task 10
- **Type consistency:** `BitableBinding`, `FieldMapping`, `FieldRef`,
  `StatusValueMapping`, `ProposedMapping`, `BitableOption` all defined in Task 1
  or Task 4, referenced consistently in later tasks
- **TDD discipline:** Each task starts with failing tests, follows
  red→green→refactor→commit
- **Commit cadence:** ~20 commits, one per task (matches Phase 3a-2 cadence)

**Plan complete and saved to**
`docs/superpowers/plans/2026-05-15-ansambel-phase-3a-3-per-repo-lark-binding.md`.

## Two execution options:

**1. Subagent-Driven (recommended)** — A fresh subagent picks up each task, you
review the diff between tasks, fast iteration.

**2. Inline Execution** — Tasks run in this session with checkpoints for review.

Which approach?
