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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BindingsFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub bindings: HashMap<String, BitableBinding>,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for BindingsFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            bindings: HashMap::new(),
        }
    }
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
    write_atomic(&path, file)
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
