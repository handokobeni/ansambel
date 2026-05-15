// Phase 3a-2 — LarkProvider.
//
// Maps Bitable rows ↔ Task structs and routes all CRUD through the
// shared LarkClient (rate-limit-guarded, 429-aware). Task.id = Lark
// record_id (format `rec...`); the local `tk_` prefix doesn't apply
// when this provider is active because hydrate is single-provider.

use super::{CreateTaskArgs, TaskPatch, TaskProvider};
use crate::error::{AppError, Result};
use crate::platform::lark_client::{BitableRecord, LarkClient};
use crate::state::{KanbanColumn, Task};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Debug)]
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
    /// Lazy-loaded Bitable primary-column field name. Fetched once per
    /// provider instance via `bitable_list_fields`; used as a read-only
    /// fallback when the `title` field on a record is empty. Outer init
    /// failures are swallowed (cached as `None`) so a transient schema
    /// fetch error doesn't break task hydrate.
    primary_field_name: OnceCell<Option<String>>,
}

impl LarkProvider {
    pub fn new(client: Arc<LarkClient>, app_token: String, table_id: String) -> Self {
        Self {
            client,
            app_token,
            table_id,
            primary_field_name: OnceCell::new(),
        }
    }

    /// Returns the Bitable primary column's field name, or `None` if the
    /// schema fetch fails or no primary field is reported (rare).
    async fn primary_field_name(&self) -> Option<String> {
        self.primary_field_name
            .get_or_init(|| async {
                match self
                    .client
                    .bitable_list_fields(&self.app_token, &self.table_id)
                    .await
                {
                    Ok(fields) => fields
                        .into_iter()
                        .find(|f| f.is_primary)
                        .map(|f| f.field_name),
                    Err(_) => None,
                }
            })
            .await
            .clone()
    }
}

/// Maps a Bitable `kanban_column` value to our 4-column enum. The exact
/// literals (`todo`, `in_progress`, `review`, `done`) win immediately; any
/// other value runs through a normalize-then-prioritized-substring matcher
/// so existing Bitables with richer status taxonomies (e.g. Jira-style
/// "To Do / In Progress / Waiting Review / In Review / Waiting Fix /
/// Waiting Deploy / Delivered") collapse into the right kanban column
/// without forcing the user to remap option labels in Bitable.
///
/// Priority order matters when an input contains multiple keywords:
///   Done > Review > InProgress > Todo
/// chosen because a terminal state should always win, then handoff state,
/// then active state, with todo as the catch-all backlog state. For
/// example, "Review Done" → Done, "Waiting Review" → Review, "Waiting
/// Fix" → InProgress.
///
/// Returns `None` for inputs that don't match any pattern; the caller
/// surfaces the raw value in the error so the user can debug.
pub(super) fn parse_kanban_column(value: &str) -> Option<KanbanColumn> {
    match value {
        "todo" => return Some(KanbanColumn::Todo),
        "in_progress" => return Some(KanbanColumn::InProgress),
        "review" => return Some(KanbanColumn::Review),
        "done" => return Some(KanbanColumn::Done),
        _ => {}
    }

    let normalized: String = value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.is_empty() {
        return None;
    }

    const DONE_PATTERNS: &[&str] = &[
        "done", "deliver", "deploy", "released", "release", "shipped", "ship", "merged",
        "complete", "closed", "resolved", "finished",
    ];
    if DONE_PATTERNS.iter().any(|p| normalized.contains(p)) {
        return Some(KanbanColumn::Done);
    }

    const REVIEW_PATTERNS: &[&str] = &["review", "qa", "verify", "verification", "testing"];
    if REVIEW_PATTERNS.iter().any(|p| normalized.contains(p)) {
        return Some(KanbanColumn::Review);
    }

    const IN_PROGRESS_PATTERNS: &[&str] = &[
        "inprogress",
        "progress",
        "doing",
        "wip",
        "fix",
        "fixing",
        "working",
        "active",
        "started",
    ];
    if IN_PROGRESS_PATTERNS.iter().any(|p| normalized.contains(p)) {
        return Some(KanbanColumn::InProgress);
    }

    const TODO_PATTERNS: &[&str] = &[
        "todo", "backlog", "pending", "new", "open", "ready", "draft", "triage", "icebox",
    ];
    if TODO_PATTERNS.iter().any(|p| normalized.contains(p)) {
        return Some(KanbanColumn::Todo);
    }

    None
}

/// Lark-API row → Task. Returns an error if a required field is
/// missing/malformed; this surfaces "schema not initialized" cleanly.
///
/// `primary_field_name` is consulted as a fallback when the `title` field
/// is missing or empty — that lets existing Bitables whose data lives in
/// the locked primary column (e.g. "Task name") render without forcing
/// the user to duplicate every row's title into the wizard-created field.
fn record_to_task(rec: &BitableRecord, primary_field_name: Option<&str>) -> Result<Task> {
    let fields = rec.fields.as_object().ok_or_else(|| {
        AppError::Lark(format!("record {} fields is not an object", rec.record_id))
    })?;

    let title_from = |name: &str| {
        fields
            .get(name)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    };
    let title = title_from("title")
        .or_else(|| primary_field_name.and_then(title_from))
        .ok_or_else(|| {
            AppError::Lark(format!(
                "record {} missing required field 'title' (also empty in primary column {:?})",
                rec.record_id, primary_field_name
            ))
        })?
        .to_string();

    let description = fields
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let column_str = fields
        .get("kanban_column")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Lark(format!(
                "record {} missing required field 'kanban_column'",
                rec.record_id
            ))
        })?;
    let column = parse_kanban_column(column_str).ok_or_else(|| {
        AppError::Lark(format!(
            "record {} has unknown kanban_column '{column_str}'",
            rec.record_id
        ))
    })?;

    let repo_id = fields
        .get("repo_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Lark(format!(
                "record {} missing required field 'repo_id'",
                rec.record_id
            ))
        })?
        .to_string();

    let order = fields
        .get("order_within_column")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    // Lark returns ms since epoch; our Task uses seconds.
    let created_at = rec
        .extra_i64("created_time")
        .map(|ms| ms / 1000)
        .unwrap_or(0);
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

fn column_rank(c: &KanbanColumn) -> u8 {
    match c {
        KanbanColumn::Todo => 0,
        KanbanColumn::InProgress => 1,
        KanbanColumn::Review => 2,
        KanbanColumn::Done => 3,
    }
}

fn column_to_str(c: KanbanColumn) -> &'static str {
    match c {
        KanbanColumn::Todo => "todo",
        KanbanColumn::InProgress => "in_progress",
        KanbanColumn::Review => "review",
        KanbanColumn::Done => "done",
    }
}

#[async_trait]
impl TaskProvider for LarkProvider {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>> {
        let filter = repo_filter.map(|r| format!("CurrentValue.[repo_id]=\"{r}\""));
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, filter.as_deref())
            .await?;
        let primary = self.primary_field_name().await;
        let mut tasks: Vec<Task> = records
            .iter()
            .map(|r| record_to_task(r, primary.as_deref()))
            .collect::<Result<Vec<_>>>()?;
        // Same ordering convention as LocalProvider: column ASC then order DESC.
        tasks.sort_by(
            |a, b| match column_rank(&a.column).cmp(&column_rank(&b.column)) {
                std::cmp::Ordering::Equal => b.order.cmp(&a.order),
                o => o,
            },
        );
        Ok(tasks)
    }

    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task> {
        let column = args.column.unwrap_or_default();
        let fields = serde_json::json!({
            "title": args.title,
            "description": args.description,
            "kanban_column": column_to_str(column),
            "repo_id": args.repo_id,
            "order_within_column": 0
        });
        let record = self
            .client
            .bitable_create_record(&self.app_token, &self.table_id, fields)
            .await?;
        let primary = self.primary_field_name().await;
        record_to_task(&record, primary.as_deref())
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task> {
        let mut fields = serde_json::Map::new();
        if let Some(title) = patch.title {
            fields.insert("title".into(), serde_json::Value::String(title));
        }
        if let Some(description) = patch.description {
            fields.insert("description".into(), serde_json::Value::String(description));
        }
        if let Some(order) = patch.order {
            fields.insert("order_within_column".into(), serde_json::json!(order));
        }
        self.client
            .bitable_update_record(
                &self.app_token,
                &self.table_id,
                id,
                serde_json::Value::Object(fields),
            )
            .await?;
        // Bitable's update endpoint doesn't return row metadata;
        // fetch by ID to surface canonical timestamps.
        let rec = self
            .client
            .bitable_get_record(&self.app_token, &self.table_id, id)
            .await?;
        let primary = self.primary_field_name().await;
        record_to_task(&rec, primary.as_deref())
    }

    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task> {
        let fields = serde_json::json!({
            "kanban_column": column_to_str(column),
            "order_within_column": order
        });
        self.client
            .bitable_update_record(&self.app_token, &self.table_id, id, fields)
            .await?;
        let rec = self
            .client
            .bitable_get_record(&self.app_token, &self.table_id, id)
            .await?;
        let primary = self.primary_field_name().await;
        record_to_task(&rec, primary.as_deref())
    }

    async fn delete_task(&self, id: &str) -> Result<()> {
        self.client
            .bitable_delete_record(&self.app_token, &self.table_id, id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::lark_client::{LarkClient, LarkConfig};
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_config(base: &str) -> LarkConfig {
        LarkConfig {
            app_id: "cli_t".into(),
            app_secret: "s".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: base.into(),
        }
    }

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

    fn make_provider(uri: &str) -> LarkProvider {
        let client = Arc::new(LarkClient::new(make_config(uri)));
        LarkProvider::new(client, "bascntest".into(), "tbltest".into())
    }

    #[tokio::test]
    async fn lark_provider_list_maps_record_id_to_task_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "rec_abc",
                            "fields": {
                                "title": "First",
                                "description": "Desc",
                                "kanban_column": "in_progress",
                                "repo_id": "repo_a",
                                "order_within_column": 4096
                            },
                            "created_time": 1700000000000_i64,
                            "last_modified_time": 1700000001000_i64
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "rec_abc");
        assert_eq!(tasks[0].title, "First");
        assert_eq!(tasks[0].column, KanbanColumn::InProgress);
        assert_eq!(tasks[0].repo_id, "repo_a");
        assert_eq!(tasks[0].order, 4096);
        assert_eq!(tasks[0].created_at, 1_700_000_000);
        assert_eq!(tasks[0].updated_at, 1_700_000_001);
    }

    #[tokio::test]
    async fn lark_provider_list_sorts_by_order_within_column_desc() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "rec_low",
                            "fields": {"title":"L","description":"","kanban_column":"todo","repo_id":"r","order_within_column":100}
                        },
                        {
                            "record_id": "rec_high",
                            "fields": {"title":"H","description":"","kanban_column":"todo","repo_id":"r","order_within_column":900}
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(None).await.unwrap();
        assert_eq!(tasks[0].id, "rec_high");
        assert_eq!(tasks[1].id, "rec_low");
    }

    #[tokio::test]
    async fn lark_provider_list_passes_repo_filter_via_lark_filter_expr() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(query_param("filter", "CurrentValue.[repo_id]=\"repo_xyz\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {"items": [], "has_more": false, "page_token": ""}
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_xyz")).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn lark_provider_create_returns_task_with_record_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(body_json(serde_json::json!({
                "fields": {
                    "title": "New task",
                    "description": "desc",
                    "kanban_column": "todo",
                    "repo_id": "repo_a",
                    "order_within_column": 0
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "record": {
                        "record_id": "rec_new",
                        "fields": {
                            "title": "New task",
                            "description": "desc",
                            "kanban_column": "todo",
                            "repo_id": "repo_a",
                            "order_within_column": 0
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let task = provider
            .create_task(CreateTaskArgs {
                repo_id: "repo_a".into(),
                title: "New task".into(),
                description: "desc".into(),
                column: None,
            })
            .await
            .unwrap();
        assert_eq!(task.id, "rec_new");
        assert_eq!(task.column, KanbanColumn::Todo);
    }

    #[tokio::test]
    async fn lark_provider_move_sends_kanban_column_and_order_only() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("PUT"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_x",
            ))
            .and(body_json(serde_json::json!({
                "fields": {
                    "kanban_column": "done",
                    "order_within_column": 256
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_x",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "record": {
                        "record_id": "rec_x",
                        "fields": {
                            "title": "t",
                            "description": "",
                            "kanban_column": "done",
                            "repo_id": "r",
                            "order_within_column": 256
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let task = provider
            .move_task("rec_x", KanbanColumn::Done, 256)
            .await
            .unwrap();
        assert_eq!(task.column, KanbanColumn::Done);
        assert_eq!(task.order, 256);
    }

    #[tokio::test]
    async fn lark_provider_update_sends_only_named_fields() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("PUT"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_y",
            ))
            .and(body_json(serde_json::json!({
                "fields": {"title": "new"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_y",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "record": {
                        "record_id": "rec_y",
                        "fields": {
                            "title": "new",
                            "description": "",
                            "kanban_column": "todo",
                            "repo_id": "r",
                            "order_within_column": 0
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let task = provider
            .update_task(
                "rec_y",
                TaskPatch {
                    title: Some("new".into()),
                    description: None,
                    order: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(task.title, "new");
    }

    #[tokio::test]
    async fn lark_provider_delete_calls_bitable_delete() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("DELETE"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_d",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok"
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        provider.delete_task("rec_d").await.unwrap();
    }

    #[tokio::test]
    async fn lark_provider_surfaces_missing_field_error_clearly() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{
                        "record_id": "rec_broken",
                        "fields": {
                            "description": "",
                            "kanban_column": "todo",
                            "repo_id": "r"
                        }
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let err = provider.list_tasks(None).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("title"), "{msg}");
        assert!(msg.contains("rec_broken"), "{msg}");
    }

    /// When a record's `title` field is absent or empty, the provider should
    /// fall back to the Bitable primary column (auto-detected via
    /// `is_primary` on the field schema). This lets users point Ansambel at
    /// existing Bitables whose data lives in the locked primary column
    /// without rewriting every row.
    async fn mount_fields_with_primary(server: &MockServer, primary_name: &str) {
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "field_id": "fld_pri",
                            "field_name": primary_name,
                            "type": 1,
                            "is_primary": true
                        },
                        {
                            "field_id": "fld_title",
                            "field_name": "title",
                            "type": 1,
                            "is_primary": false
                        }
                    ]
                }
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn lark_provider_list_falls_back_to_primary_column_when_title_missing() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields_with_primary(&server, "Task name").await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{
                        "record_id": "rec_pgs",
                        "fields": {
                            "Task name": "Kelola: User bisa pencarian",
                            "kanban_column": "todo",
                            "repo_id": "repo_a",
                            "order_within_column": 0
                        }
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Kelola: User bisa pencarian");
        assert_eq!(tasks[0].id, "rec_pgs");
    }

    #[tokio::test]
    async fn lark_provider_list_falls_back_to_primary_when_title_is_empty_string() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields_with_primary(&server, "Task name").await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{
                        "record_id": "rec_empty_title",
                        "fields": {
                            "title": "",
                            "Task name": "Fallback Title",
                            "kanban_column": "todo",
                            "repo_id": "repo_a"
                        }
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(tasks[0].title, "Fallback Title");
    }

    // --- Fuzzy kanban_column parser ------------------------------------

    #[test]
    fn parse_kanban_column_exact_literals() {
        assert_eq!(parse_kanban_column("todo"), Some(KanbanColumn::Todo));
        assert_eq!(
            parse_kanban_column("in_progress"),
            Some(KanbanColumn::InProgress)
        );
        assert_eq!(parse_kanban_column("review"), Some(KanbanColumn::Review));
        assert_eq!(parse_kanban_column("done"), Some(KanbanColumn::Done));
    }

    #[test]
    fn parse_kanban_column_jira_style_taxonomy() {
        // Covers the user's real Bitable Task Status taxonomy.
        assert_eq!(parse_kanban_column("To Do"), Some(KanbanColumn::Todo));
        assert_eq!(
            parse_kanban_column("In Progress"),
            Some(KanbanColumn::InProgress)
        );
        assert_eq!(
            parse_kanban_column("Waiting Review"),
            Some(KanbanColumn::Review)
        );
        assert_eq!(parse_kanban_column("In Review"), Some(KanbanColumn::Review));
        assert_eq!(
            parse_kanban_column("Waiting Fix"),
            Some(KanbanColumn::InProgress)
        );
        assert_eq!(
            parse_kanban_column("Waiting Deploy"),
            Some(KanbanColumn::Done)
        );
        assert_eq!(parse_kanban_column("Delivered"), Some(KanbanColumn::Done));
    }

    #[test]
    fn parse_kanban_column_done_wins_over_other_keywords() {
        // Terminal state should win when multiple keywords present.
        assert_eq!(parse_kanban_column("Review Done"), Some(KanbanColumn::Done));
        assert_eq!(
            parse_kanban_column("Backlog Done"),
            Some(KanbanColumn::Done)
        );
        assert_eq!(parse_kanban_column("Fix Done"), Some(KanbanColumn::Done));
    }

    #[test]
    fn parse_kanban_column_review_wins_over_progress_and_todo() {
        // Handoff state should win over active and backlog.
        assert_eq!(
            parse_kanban_column("Review In Progress"),
            Some(KanbanColumn::Review)
        );
        assert_eq!(
            parse_kanban_column("Backlog Review"),
            Some(KanbanColumn::Review)
        );
    }

    #[test]
    fn parse_kanban_column_progress_wins_over_todo() {
        assert_eq!(
            parse_kanban_column("Backlog Doing"),
            Some(KanbanColumn::InProgress)
        );
    }

    #[test]
    fn parse_kanban_column_case_and_separator_insensitive() {
        assert_eq!(parse_kanban_column("TO-DO"), Some(KanbanColumn::Todo));
        assert_eq!(parse_kanban_column("to_do"), Some(KanbanColumn::Todo));
        assert_eq!(
            parse_kanban_column("IN-PROGRESS"),
            Some(KanbanColumn::InProgress)
        );
        assert_eq!(
            parse_kanban_column("  In Progress  "),
            Some(KanbanColumn::InProgress)
        );
    }

    #[test]
    fn parse_kanban_column_unknown_returns_none() {
        assert_eq!(parse_kanban_column(""), None);
        assert_eq!(parse_kanban_column("   "), None);
        assert_eq!(parse_kanban_column("xyz"), None);
        assert_eq!(parse_kanban_column("foo bar baz"), None);
    }

    #[test]
    fn parse_kanban_column_additional_taxonomies() {
        // Linear-style
        assert_eq!(parse_kanban_column("Backlog"), Some(KanbanColumn::Todo));
        assert_eq!(parse_kanban_column("Cancelled"), None);
        // Asana-style
        assert_eq!(parse_kanban_column("Doing"), Some(KanbanColumn::InProgress));
        assert_eq!(parse_kanban_column("Completed"), Some(KanbanColumn::Done));
        // GitHub-style
        assert_eq!(parse_kanban_column("Closed"), Some(KanbanColumn::Done));
        assert_eq!(parse_kanban_column("Open"), Some(KanbanColumn::Todo));
        // QA-style
        assert_eq!(parse_kanban_column("QA"), Some(KanbanColumn::Review));
        assert_eq!(parse_kanban_column("Testing"), Some(KanbanColumn::Review));
        // Generic
        assert_eq!(parse_kanban_column("WIP"), Some(KanbanColumn::InProgress));
        assert_eq!(parse_kanban_column("Shipped"), Some(KanbanColumn::Done));
        assert_eq!(parse_kanban_column("Released"), Some(KanbanColumn::Done));
    }

    #[tokio::test]
    async fn lark_provider_hydrates_jira_taxonomy_end_to_end() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields_with_primary(&server, "Task name").await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "rec_a",
                            "fields": {
                                "Task name": "Build login",
                                "kanban_column": "Waiting Review",
                                "repo_id": "repo_x",
                                "order_within_column": 100
                            }
                        },
                        {
                            "record_id": "rec_b",
                            "fields": {
                                "Task name": "Ship release",
                                "kanban_column": "Delivered",
                                "repo_id": "repo_x",
                                "order_within_column": 200
                            }
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 2);
        // Sort order: column ASC then order DESC; Review (2) comes before Done (3).
        assert_eq!(tasks[0].title, "Build login");
        assert_eq!(tasks[0].column, KanbanColumn::Review);
        assert_eq!(tasks[1].title, "Ship release");
        assert_eq!(tasks[1].column, KanbanColumn::Done);
    }

    #[tokio::test]
    async fn lark_provider_list_prefers_title_when_both_title_and_primary_present() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields_with_primary(&server, "Task name").await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{
                        "record_id": "rec_both",
                        "fields": {
                            "title": "Explicit Title",
                            "Task name": "Primary Title",
                            "kanban_column": "todo",
                            "repo_id": "repo_a"
                        }
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(tasks[0].title, "Explicit Title");
    }
}
