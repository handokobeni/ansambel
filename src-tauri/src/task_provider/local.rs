// Phase 3a-2 — LocalProvider.
//
// Trait impl that backs the in-process tasks.json store. Each method
// is a small read-modify-write over the JSON file, serialized by an
// internal `Mutex<()>`. ID format remains `tk_<nanoid>` for
// compatibility with persisted state pre-3a-2.

use super::{CreateTaskArgs, TaskPatch, TaskProvider};
use crate::commands::helpers::now_unix;
use crate::error::{AppError, Result};
use crate::ids::task_id;
use crate::persistence::tasks::{load_tasks, save_tasks};
use crate::state::{KanbanColumn, Task};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug)]
pub struct LocalProvider {
    data_dir: PathBuf,
    /// Serializes read-modify-write of tasks.json across concurrent
    /// trait calls. A standard Mutex is fine because all the work
    /// inside the critical section is sync (file I/O + JSON).
    inner_lock: Mutex<()>,
}

impl LocalProvider {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            inner_lock: Mutex::new(()),
        }
    }
}

/// Returns a stable numeric rank for sorting by `KanbanColumn`.
/// `KanbanColumn` does not derive `Ord`, so we map explicitly.
fn column_rank(col: &KanbanColumn) -> u8 {
    match col {
        KanbanColumn::Todo => 0,
        KanbanColumn::InProgress => 1,
        KanbanColumn::Review => 2,
        KanbanColumn::Done => 3,
    }
}

#[async_trait]
impl TaskProvider for LocalProvider {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>> {
        let _g = self
            .inner_lock
            .lock()
            .map_err(|e| AppError::InvalidState(format!("local_provider lock poisoned: {e}")))?;
        let map = load_tasks(&self.data_dir)?;
        let mut out: Vec<Task> = map
            .into_values()
            .filter(|t| repo_filter.is_none_or(|r| t.repo_id == r))
            .collect();
        // Sort by (column_rank, -order) so callers see a deterministic kanban.
        out.sort_by(
            |a, b| match column_rank(&a.column).cmp(&column_rank(&b.column)) {
                std::cmp::Ordering::Equal => b.order.cmp(&a.order),
                o => o,
            },
        );
        Ok(out)
    }

    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task> {
        let _g = self
            .inner_lock
            .lock()
            .map_err(|e| AppError::InvalidState(format!("local_provider lock poisoned: {e}")))?;
        let mut map = load_tasks(&self.data_dir)?;
        let now = now_unix();
        let max_order = map
            .values()
            .filter(|t| t.repo_id == args.repo_id)
            .map(|t| t.order)
            .max()
            .unwrap_or(0);
        let task = Task {
            id: task_id(),
            repo_id: args.repo_id,
            workspace_id: None,
            title: args.title,
            description: args.description,
            column: args.column.unwrap_or_default(),
            order: max_order + 1024,
            created_at: now,
            updated_at: now,
        };
        map.insert(task.id.clone(), task.clone());
        save_tasks(&self.data_dir, &map)?;
        Ok(task)
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task> {
        let _g = self
            .inner_lock
            .lock()
            .map_err(|e| AppError::InvalidState(format!("local_provider lock poisoned: {e}")))?;
        let mut map = load_tasks(&self.data_dir)?;
        let task = map
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("task {id}")))?;
        if let Some(title) = patch.title {
            task.title = title;
        }
        if let Some(description) = patch.description {
            task.description = description;
        }
        if let Some(order) = patch.order {
            task.order = order;
        }
        task.updated_at = now_unix();
        let updated = task.clone();
        save_tasks(&self.data_dir, &map)?;
        Ok(updated)
    }

    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task> {
        let _g = self
            .inner_lock
            .lock()
            .map_err(|e| AppError::InvalidState(format!("local_provider lock poisoned: {e}")))?;
        let mut map = load_tasks(&self.data_dir)?;
        let task = map
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("task {id}")))?;
        task.column = column;
        task.order = order;
        task.updated_at = now_unix();
        let updated = task.clone();
        save_tasks(&self.data_dir, &map)?;
        Ok(updated)
    }

    async fn delete_task(&self, id: &str) -> Result<()> {
        let _g = self
            .inner_lock
            .lock()
            .map_err(|e| AppError::InvalidState(format!("local_provider lock poisoned: {e}")))?;
        let mut map = load_tasks(&self.data_dir)?;
        if map.remove(id).is_none() {
            return Err(AppError::NotFound(format!("task {id}")));
        }
        save_tasks(&self.data_dir, &map)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> (LocalProvider, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        (LocalProvider::new(tmp.path().to_path_buf()), tmp)
    }

    #[tokio::test]
    async fn local_provider_round_trip_via_tasks_json() {
        let (p, _tmp) = make_provider();
        let created = p
            .create_task(CreateTaskArgs {
                repo_id: "repo_a".into(),
                title: "Hello".into(),
                description: "world".into(),
                column: None,
            })
            .await
            .unwrap();
        assert!(created.id.starts_with("tk_"));
        let listed = p.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Hello");
    }

    #[tokio::test]
    async fn local_provider_list_filters_by_repo() {
        let (p, _tmp) = make_provider();
        for repo in ["repo_a", "repo_a", "repo_b"] {
            p.create_task(CreateTaskArgs {
                repo_id: repo.into(),
                title: "t".into(),
                description: String::new(),
                column: None,
            })
            .await
            .unwrap();
        }
        assert_eq!(p.list_tasks(Some("repo_a")).await.unwrap().len(), 2);
        assert_eq!(p.list_tasks(Some("repo_b")).await.unwrap().len(), 1);
        assert_eq!(p.list_tasks(None).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn local_provider_create_assigns_tk_prefix_id() {
        let (p, _tmp) = make_provider();
        let created = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "t".into(),
                description: String::new(),
                column: Some(KanbanColumn::Review),
            })
            .await
            .unwrap();
        assert!(created.id.starts_with("tk_"), "got id: {}", created.id);
        assert_eq!(created.column, KanbanColumn::Review);
    }

    #[tokio::test]
    async fn local_provider_move_updates_column_and_order() {
        let (p, _tmp) = make_provider();
        let t = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "t".into(),
                description: String::new(),
                column: None,
            })
            .await
            .unwrap();
        let moved = p
            .move_task(&t.id, KanbanColumn::InProgress, 4096)
            .await
            .unwrap();
        assert_eq!(moved.column, KanbanColumn::InProgress);
        assert_eq!(moved.order, 4096);
        assert!(moved.updated_at >= t.updated_at);
    }

    #[tokio::test]
    async fn local_provider_update_patches_only_named_fields() {
        let (p, _tmp) = make_provider();
        let t = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "old title".into(),
                description: "old desc".into(),
                column: None,
            })
            .await
            .unwrap();
        let updated = p
            .update_task(
                &t.id,
                TaskPatch {
                    title: Some("new title".into()),
                    description: None,
                    order: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.title, "new title");
        assert_eq!(updated.description, "old desc"); // unchanged
    }

    #[tokio::test]
    async fn local_provider_delete_removes_from_disk() {
        let (p, _tmp) = make_provider();
        let t = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "t".into(),
                description: String::new(),
                column: None,
            })
            .await
            .unwrap();
        p.delete_task(&t.id).await.unwrap();
        let listed = p.list_tasks(None).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn local_provider_delete_missing_returns_not_found() {
        let (p, _tmp) = make_provider();
        let err = p.delete_task("tk_ghost").await.unwrap_err();
        assert!(err.to_string().contains("Not found"), "{err}");
    }

    #[tokio::test]
    async fn local_provider_update_missing_returns_not_found() {
        let (p, _tmp) = make_provider();
        let err = p
            .update_task("tk_ghost", TaskPatch::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"), "{err}");
    }
}
