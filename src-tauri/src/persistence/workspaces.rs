use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::workspaces_file;
use crate::state::{WorkspaceInfo, WorkspaceStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct WorkspacesFile {
    schema_version: u32,
    workspaces: HashMap<String, WorkspaceInfo>,
}

pub fn load_workspaces(data_dir: &Path) -> Result<HashMap<String, WorkspaceInfo>> {
    let file: WorkspacesFile = load_or_default(&workspaces_file(data_dir))?;
    Ok(file.workspaces)
}

pub fn save_workspaces(data_dir: &Path, workspaces: &HashMap<String, WorkspaceInfo>) -> Result<()> {
    let file = WorkspacesFile {
        schema_version: 1,
        workspaces: workspaces.clone(),
    };
    write_atomic(&workspaces_file(data_dir), &file)
}

/// Load workspaces and coerce any `Running` status to `Waiting`.
/// Guards against dead-agent state surviving a restart.
pub fn load_and_reset_running(data_dir: &Path) -> Result<HashMap<String, WorkspaceInfo>> {
    let mut map = load_workspaces(data_dir)?;
    for ws in map.values_mut() {
        if ws.status == WorkspaceStatus::Running {
            ws.status = WorkspaceStatus::Waiting;
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::KanbanColumn;

    fn make_workspace(id: &str, status: WorkspaceStatus) -> WorkspaceInfo {
        WorkspaceInfo {
            id: id.into(),
            repo_id: "repo_xyz".into(),
            branch: format!("ws/{}", id),
            base_branch: "main".into(),
            custom_branch: false,
            title: "Test workspace".into(),
            description: String::new(),
            status,
            column: KanbanColumn::Todo,
            created_at: 1_000_000,
            updated_at: 1_000_001,
        }
    }

    #[test]
    fn save_and_load_workspaces_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = HashMap::new();
        map.insert(
            "ws_abc".into(),
            make_workspace("ws_abc", WorkspaceStatus::Waiting),
        );
        save_workspaces(tmp.path(), &map).unwrap();

        let loaded = load_workspaces(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["ws_abc"].status, WorkspaceStatus::Waiting);
    }

    #[test]
    fn load_workspaces_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_workspaces(tmp.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_and_reset_running_coerces_running_to_waiting() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = HashMap::new();
        map.insert(
            "ws_1".into(),
            make_workspace("ws_1", WorkspaceStatus::NotStarted),
        );
        map.insert(
            "ws_2".into(),
            make_workspace("ws_2", WorkspaceStatus::Running),
        );
        map.insert("ws_3".into(), make_workspace("ws_3", WorkspaceStatus::Done));
        save_workspaces(tmp.path(), &map).unwrap();

        let reset = load_and_reset_running(tmp.path()).unwrap();
        assert_eq!(reset["ws_1"].status, WorkspaceStatus::NotStarted);
        assert_eq!(reset["ws_2"].status, WorkspaceStatus::Waiting);
        assert_eq!(reset["ws_3"].status, WorkspaceStatus::Done);
    }

    #[test]
    fn load_and_reset_running_preserves_waiting_and_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = HashMap::new();
        map.insert(
            "ws_w".into(),
            make_workspace("ws_w", WorkspaceStatus::Waiting),
        );
        map.insert(
            "ws_e".into(),
            make_workspace("ws_e", WorkspaceStatus::Error),
        );
        save_workspaces(tmp.path(), &map).unwrap();

        let reset = load_and_reset_running(tmp.path()).unwrap();
        assert_eq!(reset["ws_w"].status, WorkspaceStatus::Waiting);
        assert_eq!(reset["ws_e"].status, WorkspaceStatus::Error);
    }

    #[test]
    fn load_and_reset_running_from_raw_json_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = r#"{
            "schema_version": 1,
            "workspaces": {
                "ws_a": {
                    "id": "ws_a", "repo_id": "repo_1",
                    "branch": "ws/a", "base_branch": "main",
                    "custom_branch": false, "title": "A", "description": "",
                    "status": "not_started", "column": "todo",
                    "created_at": 1000, "updated_at": 1001
                },
                "ws_b": {
                    "id": "ws_b", "repo_id": "repo_1",
                    "branch": "ws/b", "base_branch": "main",
                    "custom_branch": false, "title": "B", "description": "",
                    "status": "running", "column": "in_progress",
                    "created_at": 1000, "updated_at": 1001
                },
                "ws_c": {
                    "id": "ws_c", "repo_id": "repo_1",
                    "branch": "ws/c", "base_branch": "main",
                    "custom_branch": false, "title": "C", "description": "",
                    "status": "done", "column": "done",
                    "created_at": 1000, "updated_at": 1001
                }
            }
        }"#;
        std::fs::write(crate::platform::paths::workspaces_file(tmp.path()), fixture).unwrap();

        let map = load_and_reset_running(tmp.path()).unwrap();
        assert_eq!(map["ws_a"].status, WorkspaceStatus::NotStarted);
        assert_eq!(map["ws_b"].status, WorkspaceStatus::Waiting); // was running
        assert_eq!(map["ws_c"].status, WorkspaceStatus::Done);
    }
}
