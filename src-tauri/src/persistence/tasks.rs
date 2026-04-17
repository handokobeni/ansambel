use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::tasks_file;
use crate::state::Task;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct TasksFile {
    schema_version: u32,
    tasks: HashMap<String, Task>,
}

pub fn load_tasks(data_dir: &Path) -> Result<HashMap<String, Task>> {
    let file: TasksFile = load_or_default(&tasks_file(data_dir))?;
    Ok(file.tasks)
}

pub fn save_tasks(data_dir: &Path, tasks: &HashMap<String, Task>) -> Result<()> {
    let file = TasksFile {
        schema_version: 1,
        tasks: tasks.clone(),
    };
    write_atomic(&tasks_file(data_dir), &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, Task};

    fn make_task(id: &str) -> Task {
        Task {
            id: id.into(),
            repo_id: "repo_xyz".into(),
            workspace_id: None,
            title: "Sample task".into(),
            description: "A description".into(),
            column: KanbanColumn::Todo,
            order: 1024,
            created_at: 1_776_000_000,
            updated_at: 1_776_000_001,
        }
    }

    #[test]
    fn save_and_load_tasks_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = std::collections::HashMap::new();
        map.insert("tk_abc".into(), make_task("tk_abc"));
        save_tasks(tmp.path(), &map).unwrap();

        let loaded = load_tasks(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["tk_abc"].title, "Sample task");
    }

    #[test]
    fn load_tasks_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_tasks(tmp.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_tasks_writes_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let map: std::collections::HashMap<String, Task> = std::collections::HashMap::new();
        save_tasks(tmp.path(), &map).unwrap();

        let content =
            std::fs::read_to_string(crate::platform::paths::tasks_file(tmp.path())).unwrap();
        assert!(content.contains("\"schema_version\""));
        assert!(content.contains("\"tasks\""));
    }

    #[test]
    fn save_tasks_preserves_workspace_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut t = make_task("tk_ws");
        t.workspace_id = Some("ws_yyy".into());
        let mut map = std::collections::HashMap::new();
        map.insert("tk_ws".into(), t);
        save_tasks(tmp.path(), &map).unwrap();

        let loaded = load_tasks(tmp.path()).unwrap();
        assert_eq!(loaded["tk_ws"].workspace_id, Some("ws_yyy".into()));
    }
}
