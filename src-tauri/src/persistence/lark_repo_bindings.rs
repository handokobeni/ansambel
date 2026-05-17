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
    3
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

/// Migrate the on-disk bindings file from schema v1 to v3.
/// Skips v2 entirely (the view-aware PR #27 schema never shipped).
/// Idempotent: no-op when already at v3 or higher; no-op when file is absent.
pub fn migrate_v1_to_v3(data_dir: &Path) -> Result<u32> {
    let path = lark_repo_bindings_file(data_dir);
    if !path.exists() {
        return Ok(3);
    }
    let mut file = load_bindings(data_dir)?;
    if file.schema_version >= 3 {
        return Ok(file.schema_version);
    }
    file.schema_version = 3;
    save_bindings(data_dir, &file)?;
    Ok(3)
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
            filters: crate::state::FilterSpec::default(),
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
        assert_eq!(f.schema_version, 3);
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
        assert_eq!(value["schema_version"], 3);
        assert!(value["bindings"].is_object());
    }

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
            "status_value_mapping":{"entries":{},"default_column":"todo"},
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
}
