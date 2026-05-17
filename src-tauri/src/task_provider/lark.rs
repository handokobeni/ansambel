// Phase 3a-3 — LarkProvider accepts FieldMapping.
//
// Maps Bitable rows ↔ Task structs via a user-configured `FieldMapping`
// (produced by the binding wizard) instead of hard-coded field names.
// CRUD routes through the shared LarkClient (rate-limit-guarded, 429-aware).
// Task.id = Lark record_id (format `rec...`); the local `tk_` prefix doesn't
// apply when this provider is active because hydrate is single-provider.

use super::{CreateTaskArgs, TaskPatch, TaskProvider};
use crate::error::Result;
use crate::platform::lark_client::{BitableOption, BitableRecord, LarkClient};
use crate::state::{BitableBinding, FieldMapping, KanbanColumn, StatusValueMapping, Task};
use crate::task_provider::lark_field_resolver::{
    resolve_description, resolve_order, resolve_status, resolve_title,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Debug)]
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
    field_mapping: FieldMapping,
    status_value_mapping: StatusValueMapping,
    /// Lazy-loaded Bitable primary-column field name. Fetched once per
    /// provider instance via `bitable_list_fields`; used as a read-only
    /// fallback when the mapped `title` field on a record is empty. Outer init
    /// failures are swallowed (cached as `None`) so a transient schema
    /// fetch error doesn't break task hydrate.
    primary_field_name: OnceCell<Option<String>>,
    /// Lazy-loaded option list for the mapped status field. Needed by the
    /// write path: Lark Bitable's `records/update` endpoint expects the
    /// option's NAME (plain string) for single-select fields, not its id.
    /// Without this lookup we'd send `{"id": "opt_xxx"}` and Lark would
    /// reject with `1254062 SingleSelectFieldConvFail`. Cached as empty
    /// `Vec` when no status field is mapped or the fetch fails.
    status_options: OnceCell<Vec<BitableOption>>,
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
            status_options: OnceCell::new(),
        }
    }

    pub fn from_binding(client: Arc<LarkClient>, binding: BitableBinding) -> Self {
        Self::new(
            client,
            binding.app_token,
            binding.table_id,
            binding.field_mapping,
            binding.status_value_mapping,
        )
    }

    /// Returns the Bitable primary column's field name, or `None` if the
    /// schema fetch fails or no primary field is reported (rare).
    async fn primary_field_name(&self) -> Option<String> {
        self.primary_field_name
            .get_or_init(|| async {
                self.client
                    .bitable_list_fields(&self.app_token, &self.table_id)
                    .await
                    .ok()
                    .and_then(|fields| {
                        fields
                            .into_iter()
                            .find(|f| f.is_primary)
                            .map(|f| f.field_name)
                    })
            })
            .await
            .clone()
    }

    /// One-shot: kanban column → option name (via reverse-lookup of the
    /// mapped option id, then schema lookup of its name). Returns `None`
    /// when no mapping exists for the column, or the option id is stale.
    async fn resolve_status_name(&self, column: KanbanColumn) -> Option<String> {
        let opt_id = reverse_lookup_option(&self.status_value_mapping, column)?;
        self.status_option_name(&opt_id).await
    }

    /// Resolves an option id to its name (text) via the cached status
    /// field options. Returns `None` when no status field is mapped, the
    /// schema fetch fails, or the option id is stale (e.g. user renamed
    /// the option in Bitable after binding was saved). Callers fall back
    /// to a canonical literal in that case.
    async fn status_option_name(&self, option_id: &str) -> Option<String> {
        let opts = self
            .status_options
            .get_or_init(|| async {
                let Some(status_ref) = self.field_mapping.status.as_ref() else {
                    return Vec::new();
                };
                let Ok(fields) = self
                    .client
                    .bitable_list_fields(&self.app_token, &self.table_id)
                    .await
                else {
                    return Vec::new();
                };
                fields
                    .into_iter()
                    .find(|f| f.field_id == status_ref.field_id)
                    .map(|f| f.options())
                    .unwrap_or_default()
            })
            .await;
        opts.iter()
            .find(|o| o.id == option_id)
            .map(|o| o.name.clone())
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
pub(crate) fn parse_kanban_column(value: &str) -> Option<KanbanColumn> {
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

fn column_to_str(c: KanbanColumn) -> &'static str {
    match c {
        KanbanColumn::Todo => "todo",
        KanbanColumn::InProgress => "in_progress",
        KanbanColumn::Review => "review",
        KanbanColumn::Done => "done",
    }
}

/// Reverse-lookup a `KanbanColumn` in the `StatusValueMapping` and return
/// the first matching option id. Used when writing status back to Bitable.
fn reverse_lookup_option(values: &StatusValueMapping, target: KanbanColumn) -> Option<String> {
    values
        .entries
        .iter()
        .find(|(_, col)| **col == target)
        .map(|(id, _)| id.clone())
}

/// Lark-API row → Task. Returns an error if a required field is
/// missing/malformed; this surfaces "title not resolvable" cleanly.
///
/// `primary_field_name` is consulted as a fallback when the mapped title
/// field is missing or empty — that lets existing Bitables whose data lives
/// in the locked primary column render without forcing the user to duplicate
/// every row's title.
///
/// `default_repo_id` is used when there is no repo_id field in the mapping —
/// typically passed as the current repo filter so existing Bitables without a
/// per-row repo concept still hydrate into Ansambel's active repo.
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

#[async_trait]
impl TaskProvider for LarkProvider {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>> {
        // Fetch all records and filter client-side so rows with missing
        // repo_id can fall back to the caller's filter value. The Lark-side
        // filter expression would silently drop those rows.
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, None, None)
            .await?;
        let primary = self.primary_field_name().await;
        let total = records.len();
        let mut skipped = 0usize;
        let mut tasks: Vec<Task> = records
            .iter()
            .filter_map(|r| {
                match record_to_task(
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
                }
            })
            .collect();
        if skipped > 0 {
            tracing::warn!(
                skipped,
                total,
                "skipped {skipped}/{total} Bitable records that could not be parsed (run with RUST_LOG=debug for per-record details)"
            );
        }
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
        let column = args.column.clone().unwrap_or_default();
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
            // Lark Bitable's single-select WRITE endpoint expects the
            // option's name as a plain string, NOT `{"id": ...}` (that
            // shape is read-only). We reverse-lookup the mapped option id
            // then resolve it to a name via the cached schema. Fallback to
            // the canonical literal when either lookup misses, so a
            // freshly-created provider still writes a parseable value.
            let status_value = if let Some(name) = self.resolve_status_name(column.clone()).await {
                serde_json::Value::String(name)
            } else {
                serde_json::Value::String(column_to_str(column.clone()).to_string())
            };
            fields.insert(status_ref.field_name.clone(), status_value);
        }
        // Write order as 0 on creation via mapped field when available.
        if let Some(order_ref) = &self.field_mapping.order {
            fields.insert(order_ref.field_name.clone(), serde_json::json!(0));
        }
        let record = self
            .client
            .bitable_create_record(
                &self.app_token,
                &self.table_id,
                serde_json::Value::Object(fields),
            )
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

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task> {
        let mut fields = serde_json::Map::new();
        if let Some(title) = patch.title {
            fields.insert(
                self.field_mapping.title.field_name.clone(),
                serde_json::Value::String(title),
            );
        }
        if let Some(description) = patch.description {
            if let Some(desc_ref) = &self.field_mapping.description {
                fields.insert(
                    desc_ref.field_name.clone(),
                    serde_json::Value::String(description),
                );
            }
        }
        if let Some(order) = patch.order {
            if let Some(order_ref) = &self.field_mapping.order {
                fields.insert(order_ref.field_name.clone(), serde_json::json!(order));
            }
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
        record_to_task(
            &rec,
            &self.field_mapping,
            &self.status_value_mapping,
            primary.as_deref(),
            None,
        )
    }

    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task> {
        let mut fields = serde_json::Map::new();
        if let Some(status_ref) = &self.field_mapping.status {
            // See create_task comment: Bitable expects the option name as
            // a plain string, not `{"id": ...}`.
            let status_value = if let Some(name) = self.resolve_status_name(column.clone()).await {
                serde_json::Value::String(name)
            } else {
                serde_json::Value::String(column_to_str(column).to_string())
            };
            fields.insert(status_ref.field_name.clone(), status_value);
        }
        if let Some(order_ref) = &self.field_mapping.order {
            fields.insert(order_ref.field_name.clone(), serde_json::json!(order));
        }
        self.client
            .bitable_update_record(
                &self.app_token,
                &self.table_id,
                id,
                serde_json::Value::Object(fields),
            )
            .await?;
        let rec = self
            .client
            .bitable_get_record(&self.app_token, &self.table_id, id)
            .await?;
        let primary = self.primary_field_name().await;
        record_to_task(
            &rec,
            &self.field_mapping,
            &self.status_value_mapping,
            primary.as_deref(),
            None,
        )
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
    use crate::state::FieldRef;
    use wiremock::matchers::{body_json, method, path};
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

    fn canonical_mapping() -> FieldMapping {
        FieldMapping {
            title: FieldRef {
                field_id: "fld_t".into(),
                field_name: "title".into(),
            },
            description: Some(FieldRef {
                field_id: "fld_d".into(),
                field_name: "description".into(),
            }),
            status: Some(FieldRef {
                field_id: "fld_s".into(),
                field_name: "kanban_column".into(),
            }),
            order: Some(FieldRef {
                field_id: "fld_o".into(),
                field_name: "order_within_column".into(),
            }),
        }
    }

    fn canonical_values() -> StatusValueMapping {
        StatusValueMapping::default()
    }

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
    async fn lark_provider_defaults_missing_kanban_column_to_todo() {
        // Real-world: some rows in an existing Bitable have a Task Status
        // field but its value is null for that record. Hydration should
        // place the row in Todo (with a warn log) rather than fail.
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
                        "record_id": "rec_no_status",
                        "fields": {
                            "title": "Untriaged task",
                            "repo_id": "repo_x"
                        }
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].column, KanbanColumn::Todo);
        assert_eq!(tasks[0].id, "rec_no_status");
    }

    #[tokio::test]
    async fn lark_provider_defaults_unmappable_kanban_value_to_todo() {
        // Status value that the fuzzy parser doesn't recognise (e.g.
        // "Cancelled") should still hydrate, defaulting to Todo with a
        // warn log so the user can debug.
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
                        "record_id": "rec_unmappable",
                        "fields": {
                            "title": "Cancelled feature",
                            "kanban_column": "Cancelled",
                            "repo_id": "repo_x"
                        }
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].column, KanbanColumn::Todo);
    }

    #[tokio::test]
    async fn lark_provider_list_assigns_all_records_filter_repo_id() {
        // With per-repo FieldMapping, the provider is bound to one repo.
        // All records from this Bitable belong to the repo the caller
        // passes as repo_filter; the retain keeps all of them because
        // they're all stamped with the same repo_id.
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
                            "record_id": "rec_a",
                            "fields": {"title": "Task A", "kanban_column": "todo"}
                        },
                        {
                            "record_id": "rec_b",
                            "fields": {"title": "Task B", "kanban_column": "done"}
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_xyz")).await.unwrap();
        // Both records belong to the bound repo.
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.repo_id == "repo_xyz"));
    }

    #[tokio::test]
    async fn lark_provider_list_defaults_repo_id_from_filter_when_missing() {
        // The user's real scenario: existing Bitable has no repo_id column.
        // Rows without repo_id should be treated as belonging to the
        // currently selected repo (the filter value).
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
                            "record_id": "rec_a",
                            "fields": {"title": "A", "kanban_column": "todo"}
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_current")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].repo_id, "repo_current");
    }

    #[tokio::test]
    async fn lark_provider_list_finds_status_via_field_name_in_mapping() {
        // When mapping.status.field_name is "Task Status", the provider
        // should read that field from the record and fuzzy-parse it.
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields_with_primary(&server, "Task name").await;

        // Build a provider with "Task Status" as the status field name
        let client = Arc::new(LarkClient::new(make_config(&server.uri())));
        let mapping = FieldMapping {
            title: FieldRef {
                field_id: "fld_pri".into(),
                field_name: "Task name".into(),
            },
            description: None,
            status: Some(FieldRef {
                field_id: "fld_s".into(),
                field_name: "Task Status".into(),
            }),
            order: None,
        };
        let provider = LarkProvider::new(
            client,
            "bascntest".into(),
            "tbltest".into(),
            mapping,
            canonical_values(),
        );

        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "rec_task_status",
                            "fields": {
                                "Task name": "Built feature",
                                "Task Status": "Waiting Review"
                            }
                        },
                        {
                            "record_id": "rec_workflow_status",
                            "fields": {
                                "Task name": "Released feature",
                                "Task Status": "Delivered"
                            }
                        },
                        {
                            "record_id": "rec_stage",
                            "fields": {
                                "Task name": "Stage feature",
                                "Task Status": "In Progress"
                            }
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 3);
        let by_id: std::collections::HashMap<_, _> =
            tasks.iter().map(|t| (t.id.as_str(), t)).collect();
        assert_eq!(by_id["rec_task_status"].column, KanbanColumn::Review);
        assert_eq!(by_id["rec_workflow_status"].column, KanbanColumn::Done);
        assert_eq!(by_id["rec_stage"].column, KanbanColumn::InProgress);
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
    async fn lark_provider_move_sends_mapped_status_and_order() {
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
    async fn lark_provider_move_sends_option_name_for_single_select_status() {
        // Regression for `1254062 SingleSelectFieldConvFail`: when the
        // mapped status is a single-select field, the WRITE payload must
        // be the option's NAME as a plain string. The `{"id": "opt_..."}`
        // shape is read-only and Lark rejects it on write.
        let server = MockServer::start().await;
        mount_token(&server).await;
        // Mount fields so resolve_status_name can find the option name.
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "field_id": "fld_s",
                            "field_name": "Task Status",
                            "type": 3,
                            "is_primary": false,
                            "property": {
                                "options": [
                                    {"id": "optTodo", "name": "To Do"},
                                    {"id": "optDone", "name": "Selesai"}
                                ]
                            }
                        }
                    ]
                }
            })))
            .mount(&server)
            .await;
        // The payload must contain `"Task Status": "Selesai"` — plain
        // string, NOT `{"id": "optDone"}`.
        Mock::given(method("PUT"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_x",
            ))
            .and(body_json(serde_json::json!({
                "fields": {
                    "Task Status": "Selesai",
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
                            "Task Status": {"id": "optDone", "text": "Selesai"},
                            "order_within_column": 256
                        }
                    }
                }
            })))
            .mount(&server)
            .await;

        let client = std::sync::Arc::new(LarkClient::new(make_config(&server.uri())));
        let mut entries = std::collections::HashMap::new();
        entries.insert("optTodo".to_string(), KanbanColumn::Todo);
        entries.insert("optDone".to_string(), KanbanColumn::Done);
        let values = StatusValueMapping {
            entries,
            default_column: KanbanColumn::Todo,
        };
        let mapping = FieldMapping {
            title: crate::state::FieldRef {
                field_id: "fld_t".into(),
                field_name: "title".into(),
            },
            description: None,
            status: Some(crate::state::FieldRef {
                field_id: "fld_s".into(),
                field_name: "Task Status".into(),
            }),
            order: Some(crate::state::FieldRef {
                field_id: "fld_o".into(),
                field_name: "order_within_column".into(),
            }),
        };
        let provider = LarkProvider::new(
            client,
            "bascntest".into(),
            "tbltest".into(),
            mapping,
            values,
        );
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
    async fn lark_provider_skips_malformed_records_instead_of_failing_entire_list() {
        // A single row without title (and no primary fallback available)
        // must not abort the hydrate; it gets skipped (with a summary log)
        // and the rest of the batch loads normally.
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
                            "record_id": "rec_broken",
                            "fields": {
                                "description": "",
                                "kanban_column": "todo",
                                "repo_id": "r"
                            }
                        },
                        {
                            "record_id": "rec_ok",
                            "fields": {
                                "title": "Valid row",
                                "kanban_column": "todo",
                                "repo_id": "r"
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
        let tasks = provider.list_tasks(None).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "rec_ok");
    }

    /// When a record's mapped title field is absent or empty, the provider should
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

    #[tokio::test]
    async fn lark_provider_from_binding_constructs_provider() {
        // Smoke test: from_binding should produce a working provider.
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
                        "record_id": "rec_b1",
                        "fields": {"title": "T", "kanban_column": "todo"}
                    }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let client = Arc::new(LarkClient::new(make_config(&server.uri())));
        let binding = BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            view_id: None,
            field_mapping: canonical_mapping(),
            status_value_mapping: canonical_values(),
            created_at: 0,
            updated_at: 0,
        };
        let provider = LarkProvider::from_binding(client, binding);
        let tasks = provider.list_tasks(Some("x")).await.unwrap();
        // repo_id defaults to filter "x"; record has no repo_id field
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "rec_b1");
    }

    #[test]
    fn reverse_lookup_finds_matching_column() {
        let mut entries = std::collections::HashMap::new();
        entries.insert("opt_todo".into(), KanbanColumn::Todo);
        entries.insert("opt_done".into(), KanbanColumn::Done);
        let values = StatusValueMapping {
            entries,
            default_column: KanbanColumn::Todo,
        };
        let id = reverse_lookup_option(&values, KanbanColumn::Done);
        assert_eq!(id, Some("opt_done".into()));
    }

    #[test]
    fn reverse_lookup_returns_none_when_no_match() {
        let values = StatusValueMapping::default();
        let id = reverse_lookup_option(&values, KanbanColumn::Review);
        assert_eq!(id, None);
    }
}
