// Phase 3a-2 — TaskProvider abstraction.
//
// Splits "where do tasks live" from the command layer. `LocalProvider`
// wraps tasks.json; `LarkProvider` wraps Lark Bitable. Both implement
// the same async trait so commands/task.rs can stay agnostic.
//
// AppState.tasks remains the in-memory mirror — the command layer
// reads it for list_tasks (cache) and writes it after a provider
// mutation succeeds.

use crate::error::Result;
use crate::state::{KanbanColumn, Task};
use async_trait::async_trait;

pub mod local;
pub mod schema;

/// Argument bundle for create_task. Mirrors what the frontend posts;
/// each provider decides its own ID strategy (LocalProvider generates
/// `tk_<nanoid>`; LarkProvider returns Bitable's `record_id`).
#[derive(Debug, Clone)]
pub struct CreateTaskArgs {
    pub repo_id: String,
    pub title: String,
    pub description: String,
    /// Defaults to `Todo` when None.
    pub column: Option<KanbanColumn>,
}

/// Partial update bundle. Fields left as `None` are not modified.
#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub order: Option<i32>,
}

#[async_trait]
pub trait TaskProvider: Send + Sync + std::fmt::Debug {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>>;
    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task>;
    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task>;
    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task>;
    async fn delete_task(&self, id: &str) -> Result<()>;
}
