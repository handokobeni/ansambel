use crate::error::Result;
use crate::persistence::atomic::write_atomic;
use crate::platform::paths::lark_repo_bindings_file;
use crate::state::BitableBinding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// On-disk shape of `lark_repo_bindings.json`. Wraps the bindings map
/// in a versioned envelope so future schema changes can be detected
/// without breaking old installs.

// Note: callers do load → modify → save without inter-process locking.
// Ansambel is a single-process Tauri app and command dispatch is serialised
// by the AppState Mutex, so this is safe within the current architecture.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct BindingsFile {
    #[serde(default = "default_schema_version")]
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) bindings: HashMap<String, BitableBinding>,
}

fn default_schema_version() -> u32 {
    2
}

impl Default for BindingsFile {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            bindings: HashMap::new(),
        }
    }
}

pub(crate) fn load_bindings(data_dir: &Path) -> Result<BindingsFile> {
    crate::persistence::atomic::load_or_default(&lark_repo_bindings_file(data_dir))
}

pub(crate) fn save_bindings(data_dir: &Path, file: &BindingsFile) -> Result<()> {
    let path = lark_repo_bindings_file(data_dir);
    write_atomic(&path, file)
}

/// Idempotent Phase 3a-3.1 migration. Reads the bindings file, and if its
/// `schema_version` is still 1, rewrites it to 2. The `view_id` field is
/// added by `#[serde(default)]` on `BitableBinding` — no per-binding
/// rewrite is required. Returns the resulting (post-migration) schema
/// version for logging.
//
// `allow(dead_code)` until Task 7 wires the call into `lib.rs::setup`.
// Tests in this module exercise the function so coverage is not lost.
#[allow(dead_code)]
pub(crate) fn migrate_v1_to_v2(data_dir: &Path) -> Result<u32> {
    let mut file = load_bindings(data_dir)?;
    if file.schema_version >= 2 {
        return Ok(file.schema_version);
    }
    file.schema_version = 2;
    save_bindings(data_dir, &file)?;
    Ok(2)
}

pub(crate) fn get_binding(data_dir: &Path, repo_id: &str) -> Result<Option<BitableBinding>> {
    let file = load_bindings(data_dir)?;
    Ok(file.bindings.get(repo_id).cloned())
}

pub(crate) fn set_binding(data_dir: &Path, repo_id: &str, binding: BitableBinding) -> Result<()> {
    let mut file = load_bindings(data_dir)?;
    file.bindings.insert(repo_id.to_string(), binding);
    save_bindings(data_dir, &file)
}

pub(crate) fn delete_binding(data_dir: &Path, repo_id: &str) -> Result<bool> {
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
            view_id: None,
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
        assert_eq!(f.schema_version, 2);
    }

    #[test]
    fn round_trip_save_and_load_preserves_binding() {
        let tmp = tempdir().unwrap();
        set_binding(tmp.path(), "repo_x", make_binding()).unwrap();
        let f = load_bindings(tmp.path()).unwrap();
        assert_eq!(f.bindings.len(), 1);
        let b = f.bindings.get("repo_x").unwrap();
        assert_eq!(b.app_token, "bascntest");
        assert_eq!(b.table_id, "tbltest");
        assert_eq!(b.field_mapping.title.field_id, "fld_pri");
        assert_eq!(b.created_at, 1747200000);
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

    #[test]
    fn schema_version_serialized_in_file() {
        let tmp = tempdir().unwrap();
        set_binding(tmp.path(), "repo_x", make_binding()).unwrap();
        let path = lark_repo_bindings_file(tmp.path());
        let raw = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["schema_version"], 2);
        assert!(value["bindings"].is_object());
    }

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
        assert_eq!(
            loaded.bindings.get("repo_x").unwrap().app_token,
            "bascntest"
        );
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
}
