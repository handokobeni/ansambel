//! Workspace state publisher (Phase 3a-3): subscribes to the
//! `WorkspaceEvent` broadcast, aggregates per-workspace updates over a
//! configurable debounce window, and dispatches snapshots to an injected
//! uploader closure. Task 6 of the publisher plan — this file owns the
//! event loop; Task 8 binds the uploader to `LarkClient::bitable_upsert_row`;
//! Task 9 spawns the loop at app startup.

use crate::error::Result;
use crate::sanitize::sanitize_message_preview;
use crate::state::{TeamActivityConfig, WorkspaceEvent, WorkspaceStatus};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{Duration, Instant};

const MESSAGE_PREVIEW_MAX: usize = 200;

/// Hard ceiling per uploader call. If the underlying Lark HTTP request
/// hangs (DNS stall, dropped TCP, server wedge) the publisher's
/// fire-and-forget `tokio::spawn` would leak the task forever — so
/// every call goes through `tokio::time::timeout` with this budget.
/// Chosen to comfortably exceed Lark's typical p99 (~2-3s) plus one
/// 429 backoff while still being short enough that a hung publish
/// doesn't accumulate spawned tasks under sustained traffic.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// Soft cap that upstream emitters (Tasks 10-13) should apply before
/// pushing a message snippet into `WorkspaceEvent::MessageAppended.text_preview`.
/// The publisher still runs the credential redaction + 200-char trim of its
/// own, so this is purely a "don't ship megabytes through the broadcast"
/// guideline. Centralised here so emitters and the publisher reference one
/// constant.
#[allow(dead_code)]
pub const EVENT_TEXT_PREVIEW_INPUT_MAX: usize = 400;

/// What ends up in a Bitable row. Owned shape (no borrows) so it can cross
/// the closure boundary into the uploader future.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct RowSnapshot {
    pub workspace_id: String,
    pub repo_remote_url: Option<String>,
    pub repo_display_name: Option<String>,
    pub task_title: Option<String>,
    pub assignee_machine: Option<String>,
    pub ansambel_status: Option<String>,
    pub last_activity_at: Option<i64>, // epoch ms
    pub last_message_preview: Option<String>,
    pub branch_name: Option<String>,
    pub diff_summary: Option<String>,
    pub pr_url: Option<String>,
    pub private: bool,
}

/// One row in the Phase 3a-4 sidebar / mirror view, mirroring the columns
/// the publisher (`RowSnapshot` → `snapshot_to_fields`) writes. All-string
/// for IPC simplicity; the i64 `last_activity_at` is epoch ms (the same
/// shape the publisher writes via `last_activity_at = Some(now_ms)`).
///
/// Missing or wrong-typed fields default to empty / 0 / false instead of
/// erroring; teammates running older publisher versions may emit partial
/// rows, and a partial row is more useful than no row at all.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TeamActivityRow {
    pub workspace_id: String,
    pub repo_remote_url: String,
    pub repo_display_name: String,
    pub task_title: String,
    pub assignee_machine: String,
    pub ansambel_status: String,
    pub last_activity_at: i64,
    pub last_message_preview: String,
    pub branch_name: String,
    pub diff_summary: String,
    pub pr_url: String,
    pub private: bool,
}

/// What `fetch_team_activity_rows` returns. Tagged enum so the frontend
/// can pattern-match without an extra discriminator. Each non-Rows variant
/// drives a distinct sidebar empty/disabled state; see the spec's error
/// handling matrix.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FetchResult {
    /// `team_activity_config.json` absent or `app_token` empty.
    Disabled,
    /// Config exists but `machine_label` is blank — filter would over-match.
    MachineLabelEmpty,
    /// User has no local repos with an origin remote, so there's nothing to
    /// scope the team-activity rows against.
    NoOverlapRepos,
    /// Success. Rows may be empty (filter matched zero records).
    Rows { rows: Vec<TeamActivityRow> },
}

/// Maps one Bitable record into a `TeamActivityRow`. Defensive against
/// missing keys and wrong types — see the contract note on
/// [`TeamActivityRow`].
pub(crate) fn parse_record_to_row(
    record: crate::platform::lark_client::BitableRecord,
) -> TeamActivityRow {
    use crate::task_provider::lark_field_resolver::{extract_single_select, extract_text};
    let f = &record.fields;
    // The Bitable *search* endpoint returns text fields as segmented
    // rich-text arrays (`[{"type":"text","text":"..."}]`), not the plain
    // strings the list endpoint returns. `extract_text` handles both
    // shapes; a naive `as_str()` silently drops every search-endpoint text
    // field, which surfaces as empty rows in the Team Activity sidebar.
    let s = |key: &str| -> String { f.get(key).and_then(extract_text).unwrap_or_default() };
    TeamActivityRow {
        workspace_id: s("workspace_id"),
        repo_remote_url: s("repo_remote_url"),
        repo_display_name: s("repo_display_name"),
        task_title: s("task_title"),
        assignee_machine: s("assignee_machine"),
        // Single-select arrives as a bare string, an option object
        // (`{"id","text"}`), or an array of either — `extract_single_select`
        // normalises all three to the option's display text.
        ansambel_status: f
            .get("ansambel_status")
            .and_then(extract_single_select)
            .map(|(_, name)| name)
            .unwrap_or_default(),
        last_activity_at: f
            .get("last_activity_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        last_message_preview: s("last_message_preview"),
        branch_name: s("branch_name"),
        diff_summary: s("diff_summary"),
        pr_url: s("pr_url"),
        private: f.get("private").and_then(|v| v.as_bool()).unwrap_or(false),
    }
}

/// Per-workspace aggregation state held while the loop debounces.
#[derive(Clone, Default)]
struct AggregatedState {
    snapshot: RowSnapshot,
    /// Dirty means "an event has arrived since the last flush".
    dirty: bool,
    /// Once `PrivacyChanged { is_private: true }` arrives, subsequent
    /// non-Privacy events update `last_activity_at` + `dirty` but do
    /// NOT repopulate sensitive fields. Set false again on
    /// `PrivacyChanged { is_private: false }`.
    private_lock: bool,
}

impl AggregatedState {
    fn merge(&mut self, event: &WorkspaceEvent, machine_label: &str) {
        let now_ms = now_epoch_ms();
        self.snapshot.workspace_id = event.workspace_id().to_string();
        self.snapshot.last_activity_at = Some(now_ms);
        self.dirty = true;

        // Privacy toggle short-circuits below — it owns the lock and the
        // explicit field-clearing semantics.
        if let WorkspaceEvent::PrivacyChanged { is_private, .. } = event {
            if *is_private {
                self.private_lock = true;
                self.snapshot.assignee_machine = None;
                self.snapshot.ansambel_status = None;
                self.snapshot.last_message_preview = None;
                self.snapshot.branch_name = None;
                self.snapshot.diff_summary = None;
                self.snapshot.pr_url = None;
                self.snapshot.private = true;
            } else {
                self.private_lock = false;
                self.snapshot.private = false;
            }
            return;
        }

        // Under privacy lock, every other event only refreshes timestamps.
        if self.private_lock {
            return;
        }

        match event {
            WorkspaceEvent::StatusChanged { new_status, .. } => {
                self.snapshot.ansambel_status = Some(status_to_string(new_status));
                self.snapshot.assignee_machine = Some(machine_label.to_string());
            }
            WorkspaceEvent::MessageAppended { text_preview, .. } => {
                self.snapshot.last_message_preview =
                    Some(sanitize_message_preview(text_preview, MESSAGE_PREVIEW_MAX));
            }
            WorkspaceEvent::FileTouched { .. } => {
                // last_activity_at already updated above
            }
            WorkspaceEvent::PrCreated { url, .. } => {
                self.snapshot.pr_url = Some(url.clone());
                self.snapshot.ansambel_status = Some("pr_ready".into());
            }
            WorkspaceEvent::BranchChanged { branch_name, .. } => {
                self.snapshot.branch_name = Some(branch_name.clone());
            }
            WorkspaceEvent::DiffSummaryUpdated { summary, .. } => {
                self.snapshot.diff_summary = Some(summary.clone());
            }
            WorkspaceEvent::PrivacyChanged { .. } => {
                // Handled above via the early-return. Empty arm guards against
                // refactor regressions: if a future change inlines privacy into
                // this match without keeping the early-return, this becomes a
                // safe no-op rather than a runtime panic.
            }
        }
    }
}

fn status_to_string(s: &WorkspaceStatus) -> String {
    match s {
        WorkspaceStatus::NotStarted | WorkspaceStatus::Waiting => "waiting",
        WorkspaceStatus::Running => "running",
        WorkspaceStatus::Done => "done",
        WorkspaceStatus::Error => "error",
    }
    .into()
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Uploader closure type. The publisher hands it `(workspace_id, snapshot)`
/// and expects the (cached) Bitable record_id back. Task 8 binds this to
/// `LarkClient::bitable_upsert_row`; tests inject a recording stub.
pub type UploaderFn = Arc<
    dyn Fn(String, RowSnapshot) -> Pin<Box<dyn Future<Output = Result<String>> + Send>>
        + Send
        + Sync,
>;

/// Static-per-workspace context the publisher cannot derive from
/// `WorkspaceEvent` alone — it requires looking up the workspace in
/// `AppState`, finding the linked repo, running `git remote get-url`
/// on the repo path, and looking up any linked task.
///
/// Populated by an [`EnricherFn`] right before the snapshot is handed
/// to the uploader. Any field that is `None` falls back to leaving the
/// matching Bitable column unchanged (so test stubs / unconfigured
/// installs keep working without crashing).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct RowEnrichment {
    pub repo_remote_url: Option<String>,
    pub repo_display_name: Option<String>,
    pub task_title: Option<String>,
    /// Workspace branch name. Sourced from `WorkspaceInfo.branch` —
    /// `BranchChanged` is only emitted at workspace creation, so the
    /// publisher would otherwise leave this empty for any workspace
    /// reopened after app restart.
    pub branch_name: Option<String>,
}

/// Closure that maps a workspace_id to the static repo + task fields the
/// publisher cannot derive from events. Wired in `lib.rs` from
/// `Arc<Mutex<AppState>>`; tests pass `None` to opt out of enrichment.
///
/// Synchronous on purpose — callers must keep the call cheap (a single
/// `AppState` lock acquisition + one `git remote get-url origin` shell
/// out per cache miss), since it runs from the publisher's flush path.
pub type EnricherFn = Arc<dyn Fn(&str) -> RowEnrichment + Send + Sync>;

pub struct Publisher {
    pub cfg: TeamActivityConfig,
    /// workspace_id → Bitable record_id, populated as the uploader returns ids.
    pub row_id_cache: Arc<Mutex<HashMap<String, String>>>,
    pub uploader: UploaderFn,
    /// Optional enricher — when set, the publisher fills repo_remote_url /
    /// repo_display_name / task_title on the snapshot right before the
    /// uploader call. `None` means "publish events verbatim", used by tests
    /// that don't need AppState wiring.
    pub enricher: Option<EnricherFn>,
    pub debounce: Duration,
}

impl Publisher {
    pub async fn run(self, mut rx: broadcast::Receiver<WorkspaceEvent>) {
        let mut aggregated: HashMap<String, AggregatedState> = HashMap::new();
        let mut last_flush: HashMap<String, Instant> = HashMap::new();
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    let event = match msg {
                        Ok(e) => e,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped,
                                "team-activity publisher: broadcast channel lagged, events dropped"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // Channel sender dropped — app is shutting down.
                            return;
                        }
                    };
                    let ws_id = event.workspace_id().to_string();
                    aggregated
                        .entry(ws_id)
                        .or_default()
                        .merge(&event, &self.cfg.machine_label);
                }
                _ = tokio::time::sleep(self.debounce) => {
                    let now = Instant::now();
                    let due: Vec<String> = aggregated
                        .iter()
                        .filter(|(ws_id, st)| {
                            st.dirty
                                && last_flush
                                    .get(*ws_id)
                                    .map(|t| now.duration_since(*t) >= self.debounce)
                                    .unwrap_or(true)
                        })
                        .map(|(k, _)| k.clone())
                        .collect();
                    for ws_id in due {
                        let Some(state) = aggregated.get_mut(&ws_id) else {
                            continue;
                        };
                        let mut snap = state.snapshot.clone();
                        // Apply static enrichment (repo + task lookups) just
                        // before upload. Skipping under privacy lock keeps
                        // the row stripped — even repo_remote_url / display
                        // / task_title are user-identifying once correlated.
                        if !state.private_lock {
                            if let Some(enricher) = &self.enricher {
                                let e = enricher(&ws_id);
                                if snap.repo_remote_url.is_none() {
                                    snap.repo_remote_url = e.repo_remote_url;
                                }
                                if snap.repo_display_name.is_none() {
                                    snap.repo_display_name = e.repo_display_name;
                                }
                                if snap.task_title.is_none() {
                                    snap.task_title = e.task_title;
                                }
                                if snap.branch_name.is_none() {
                                    snap.branch_name = e.branch_name;
                                }
                            }
                        }
                        state.dirty = false;
                        last_flush.insert(ws_id.clone(), now);
                        let uploader = self.uploader.clone();
                        let cache = self.row_id_cache.clone();
                        tokio::spawn(async move {
                            match upload_with_retry(uploader, ws_id.clone(), snap).await {
                                Ok(rec_id) => {
                                    cache.lock().await.insert(ws_id, rec_id);
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "team-activity publish failed");
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}

/// Wraps a single uploader call with:
/// - one retry if the first call fails with `AppError::LarkRateLimit`,
///   sleeping for the `retry_after_secs` hinted by Lark before retrying;
/// - a 30-second hard timeout per call (see [`UPLOAD_TIMEOUT`]),
///   mapping a hang to `AppError::LarkTimeout` so the publisher's
///   fire-and-forget `tokio::spawn` cannot leak.
///
/// Used by [`Publisher::run`] for every dispatched snapshot.
pub async fn upload_with_retry(
    uploader: UploaderFn,
    ws_id: String,
    snap: RowSnapshot,
) -> Result<String> {
    match call_once(&uploader, &ws_id, &snap).await {
        Ok(rec_id) => Ok(rec_id),
        Err(crate::error::AppError::LarkRateLimit { retry_after_secs }) => {
            tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
            call_once(&uploader, &ws_id, &snap).await
        }
        Err(e) => Err(e),
    }
}

async fn call_once(uploader: &UploaderFn, ws_id: &str, snap: &RowSnapshot) -> Result<String> {
    match tokio::time::timeout(UPLOAD_TIMEOUT, (uploader)(ws_id.to_string(), snap.clone())).await {
        Ok(inner) => inner,
        Err(_) => Err(crate::error::AppError::LarkTimeout {
            timeout_secs: UPLOAD_TIMEOUT.as_secs(),
        }),
    }
}

/// Maps a `RowSnapshot` to the Bitable `fields` map for upsert.
///
/// `Option<None>` fields are omitted from the body — Lark treats a missing
/// field as "no update", whereas sending `null` clobbers any existing value
/// in the row. The publisher only knows about fields it actively manages,
/// so a `None` here means "I have no opinion; leave whatever is there".
///
/// `private` is the lone exception: it's a `bool`, not an `Option<bool>`,
/// because privacy is the user-controlled visibility signal — we always
/// want to assert the current value (false or true), never "no opinion".
//
// TODO(phase-3a-3-followup): enrich snap.repo_remote_url / repo_display_name
// / task_title from AppState by looking up workspace_id → repo_id → repo.path
// (then call read_origin_url) and task_id → task.title. Deferred so the
// uploader stays decoupled from AppState; resolved at event-emission time
// or by a wrapping closure that holds AppState.
fn snapshot_to_fields(snap: &RowSnapshot) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("workspace_id".into(), serde_json::json!(snap.workspace_id));
    if let Some(v) = &snap.repo_remote_url {
        m.insert("repo_remote_url".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.repo_display_name {
        m.insert("repo_display_name".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.task_title {
        m.insert("task_title".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.assignee_machine {
        m.insert("assignee_machine".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.ansambel_status {
        m.insert("ansambel_status".into(), serde_json::json!(v));
    }
    if let Some(v) = snap.last_activity_at {
        m.insert("last_activity_at".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.last_message_preview {
        m.insert("last_message_preview".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.branch_name {
        m.insert("branch_name".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.diff_summary {
        m.insert("diff_summary".into(), serde_json::json!(v));
    }
    if let Some(v) = &snap.pr_url {
        // `pr_url` is a Bitable URL field (field_type 15), which the API
        // only accepts as an object `{ "link", "text" }` — a bare string is
        // rejected with 1254068 URLFieldConvFail, failing the whole row
        // upsert. `text` doubles as the visible label; using the URL itself
        // keeps the cell readable.
        m.insert("pr_url".into(), serde_json::json!({ "link": v, "text": v }));
    }
    m.insert("private".into(), serde_json::json!(snap.private));
    m
}

/// Builds the production [`UploaderFn`] that calls
/// [`LarkClient::bitable_upsert_row`](crate::platform::lark_client::LarkClient::bitable_upsert_row).
///
/// Reads `row_id_cache` to decide POST (cache miss → create) vs PUT
/// (cache hit → update by id), then writes the returned `record_id`
/// back into the cache on success.
///
/// **Double-write note:** [`Publisher::run`] also writes the returned id
/// into the cache after `upload_with_retry` succeeds. That's intentional:
/// (a) the uploader's write keeps unit tests deterministic without
/// spinning up the publisher loop, (b) the loop's write is the documented
/// post-spawn contract. Both write the same id, so there's no conflict.
/// If a future refactor moves the cache write entirely into one of the
/// two sites, drop the other to remove the redundancy.
pub fn build_lark_uploader(
    client: Arc<crate::platform::lark_client::LarkClient>,
    cfg: crate::state::TeamActivityConfig,
    row_id_cache: Arc<Mutex<HashMap<String, String>>>,
) -> UploaderFn {
    Arc::new(move |ws_id, snap| {
        let client = client.clone();
        let cfg = cfg.clone();
        let row_id_cache = row_id_cache.clone();
        Box::pin(async move {
            // Acquire the cache lock just long enough to copy the cached
            // record_id, then drop the lock before the HTTP call. Holding
            // the lock across the await would serialize every concurrent
            // publish through one mutex.
            let cached = {
                let guard = row_id_cache.lock().await;
                guard.get(&ws_id).cloned()
            };
            // On cache miss (fresh process / first publish for this workspace
            // since startup), search Lark for an existing row keyed by
            // workspace_id BEFORE deciding POST vs PUT. Without this lookup,
            // every app restart creates a new row — the user reported 5
            // duplicate rows for the same workspace_id after a few restarts.
            let record_id = match cached {
                Some(id) => id,
                None => lookup_existing_record_id(&client, &cfg, &ws_id)
                    .await
                    .unwrap_or_default(),
            };
            let fields = snapshot_to_fields(&snap);
            let new_id = client
                .bitable_upsert_row(&cfg.app_token, &cfg.table_id, &record_id, fields)
                .await?;
            // Write back to cache so subsequent calls take the PUT path.
            row_id_cache.lock().await.insert(ws_id, new_id.clone());
            Ok(new_id)
        })
    })
}

/// Builds the production [`EnricherFn`] backed by `AppState` lookups.
///
/// Resolves each workspace_id to its linked repo (for `repo_remote_url` +
/// `repo_display_name`) and any task whose `workspace_id` points at it
/// (for `task_title`). The canonical remote URL is computed once per
/// repo path and cached in an in-memory map — `git remote get-url
/// origin` shells out, and we don't want to pay that on every 3-second
/// debounce flush for an active workspace.
///
/// Unknown / missing lookups silently return `None` for that field;
/// the publisher's `snapshot_to_fields` skips `None` keys so the row
/// keeps whatever value the column had before, rather than blanking it.
///
/// The state lock is held only for the synchronous read; the (slower)
/// `git remote` call runs after the lock is dropped so the publisher's
/// flush path doesn't serialise on AppState contention.
pub fn build_app_enricher(state: Arc<std::sync::Mutex<crate::state::AppState>>) -> EnricherFn {
    // Per-repo cache of the canonical remote URL. Keyed by repo_id (not
    // repo.path) because the path string is identical anyway — repos
    // don't migrate paths mid-session — and repo_id is shorter and
    // stable. `Arc<Mutex<_>>` so the closure can mutate across calls.
    let remote_url_cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));
    Arc::new(move |ws_id: &str| {
        let (repo_path, repo_id, repo_name, task_title, branch_name) = {
            let s = match state.lock() {
                Ok(s) => s,
                Err(_) => return RowEnrichment::default(),
            };
            let Some(ws) = s.workspaces.get(ws_id) else {
                return RowEnrichment::default();
            };
            let branch_name = if ws.branch.is_empty() {
                None
            } else {
                Some(ws.branch.clone())
            };
            let repo = s.repos.get(&ws.repo_id);
            let (repo_path, repo_id, repo_name) = match repo {
                Some(r) => (
                    Some(r.path.clone()),
                    Some(r.id.clone()),
                    Some(r.name.clone()),
                ),
                None => (None, None, None),
            };
            // Prefer a live linked task's title (reflects post-creation
            // edits in Lark / Jira). Fall back to the workspace's own
            // snapshotted title — local-only workspaces never have a Task
            // entry with their workspace_id, but the title is always
            // stamped on the workspace itself at creation time, so
            // returning empty here would mean the column is empty for
            // every locally-created workspace.
            let task_title = s
                .tasks
                .values()
                .find(|t| t.workspace_id.as_deref() == Some(ws_id))
                .map(|t| t.title.clone())
                .or_else(|| {
                    if ws.title.is_empty() {
                        None
                    } else {
                        Some(ws.title.clone())
                    }
                });
            (repo_path, repo_id, repo_name, task_title, branch_name)
        }; // lock dropped before running git below.
        let remote_url = match (repo_path, repo_id) {
            (Some(path), Some(rid)) => {
                let cached = remote_url_cache
                    .lock()
                    .ok()
                    .and_then(|g| g.get(&rid).cloned());
                match cached {
                    Some(url) => Some(url),
                    None => match crate::platform::repo_identity::read_origin_url(&path) {
                        Ok(url) => {
                            if let Ok(mut g) = remote_url_cache.lock() {
                                g.insert(rid, url.clone());
                            }
                            Some(url)
                        }
                        Err(_) => None,
                    },
                }
            }
            _ => None,
        };
        RowEnrichment {
            repo_remote_url: remote_url,
            repo_display_name: repo_name,
            task_title,
            branch_name,
        }
    })
}

/// Searches Lark for an existing team-activity row matching `workspace_id`.
/// Returns the first matching `record_id` (`Ok(Some(_))`) or `Ok(None)` if no
/// row exists yet. Search errors are downgraded to `Ok(None)` by the caller —
/// the worst case is a one-off duplicate (better than no publish at all if
/// the search endpoint is transiently broken).
async fn lookup_existing_record_id(
    client: &crate::platform::lark_client::LarkClient,
    cfg: &crate::state::TeamActivityConfig,
    ws_id: &str,
) -> Result<String> {
    let filter = crate::state::FilterSpec {
        conjunction: crate::state::FilterConjunction::And,
        conditions: vec![crate::state::FilterCondition {
            field_id: String::new(),
            field_name: "workspace_id".into(),
            operator: crate::state::FilterOperator::Is,
            value: vec![ws_id.into()],
        }],
    };
    let records = client
        .bitable_search_records(&cfg.app_token, &cfg.table_id, &filter)
        .await?;
    if records.len() > 1 {
        tracing::warn!(
            workspace_id = ws_id,
            matches = records.len(),
            "team-activity: multiple rows match workspace_id; using first — manual cleanup recommended"
        );
    }
    Ok(records
        .into_iter()
        .next()
        .map(|r| r.record_id)
        .unwrap_or_default())
}

/// Returns the canonical `repo_remote_url` for `repo_id` — first call shells
/// out to `git -C <path> remote get-url origin` (via
/// [`crate::platform::repo_identity::read_origin_url`]), normalises with
/// [`crate::platform::repo_identity::canonicalise_remote_url`], and caches
/// the result keyed by `repo_id`. Subsequent calls return the cached value
/// without touching the filesystem.
///
/// Empty results (repo has no origin, or the git invocation failed) are
/// cached too — otherwise every 10-second poll would re-shell-out for
/// every repo that doesn't have an origin remote.
///
/// The cache lives for the process lifetime. If the user re-points a
/// repo's `origin` mid-session, the stale URL stays until app restart;
/// acceptable per the spec's "enrichment refresh" deferred follow-up.
///
/// Poisoned-mutex case: returns an empty string and skips the cache
/// write. A poisoned mutex means some other writer panicked, which is
/// already a worse failure mode than a missed cache write here.
pub(crate) fn read_remote_url_cached(
    cache: &Arc<std::sync::Mutex<HashMap<String, String>>>,
    repo_id: &str,
    repo_path: &std::path::Path,
) -> String {
    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.get(repo_id) {
            return cached.clone();
        }
    } else {
        return String::new();
    }
    let raw = crate::platform::repo_identity::read_origin_url(repo_path).unwrap_or_default();
    let canonical = if raw.is_empty() {
        String::new()
    } else {
        crate::platform::repo_identity::canonicalise_remote_url(&raw)
    };
    if let Ok(mut guard) = cache.lock() {
        guard.insert(repo_id.to_string(), canonical.clone());
    }
    canonical
}

/// Pure-Rust core of `fetch_team_activity_rows` — takes its dependencies
/// (config, remote-URL cache, LarkClient) as explicit arguments so the
/// Tauri wrapper stays thin and the inner logic is fully unit-testable
/// against wiremock. See `fetch_team_activity_rows` below for the IPC
/// glue.
///
/// Returns `Ok(FetchResult::…)` for the four reachable outcomes; `Err`
/// is reserved for genuine Lark / network failures (HTTP error, timeout,
/// parse error). The frontend store maps `Err` to `status='error'`.
pub(crate) async fn fetch_team_activity_rows_inner(
    state: Arc<std::sync::Mutex<crate::state::AppState>>,
    cfg: Option<crate::state::TeamActivityConfig>,
    remote_url_cache: &Arc<std::sync::Mutex<HashMap<String, String>>>,
    client: Option<Arc<crate::platform::lark_client::LarkClient>>,
) -> Result<FetchResult> {
    // Config gates: Disabled when missing or token empty.
    let cfg = match cfg {
        Some(c) if !c.app_token.is_empty() => c,
        _ => return Ok(FetchResult::Disabled),
    };
    if cfg.machine_label.is_empty() {
        return Ok(FetchResult::MachineLabelEmpty);
    }
    // Snapshot (id, path) under lock then drop it before any shell-out —
    // read_remote_url_cached may spawn `git` which we don't want under
    // the AppState mutex.
    let repos: Vec<(String, std::path::PathBuf)> = {
        let s = state
            .lock()
            .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
        s.repos
            .values()
            .map(|r| (r.id.clone(), r.path.clone()))
            .collect()
    };
    let mut remote_urls: Vec<String> = Vec::with_capacity(repos.len());
    for (repo_id, repo_path) in repos {
        let url = read_remote_url_cached(remote_url_cache, &repo_id, &repo_path);
        if !url.is_empty() {
            remote_urls.push(url);
        }
    }
    if remote_urls.is_empty() {
        return Ok(FetchResult::NoOverlapRepos);
    }

    let client = client.ok_or_else(|| {
        crate::error::AppError::Other("LarkClient required for non-disabled fetch".into())
    })?;
    // The repo-overlap filter runs client-side (see below), NOT in this
    // server filter. Lark's `is` operator only accepts a single value;
    // `repo_remote_url is [urlA, urlB]` is rejected with 1254018
    // InvalidFilter the moment a second repo is registered. The flat
    // FilterSpec has no nested-OR group to express "is any of", so we let
    // the server narrow by `assignee_machine` (teammate, not self) and
    // intersect with the local repo set in Rust.
    let local_repo_urls: std::collections::HashSet<String> = remote_urls.into_iter().collect();
    let filter = crate::state::FilterSpec {
        conjunction: crate::state::FilterConjunction::And,
        conditions: vec![
            crate::state::FilterCondition {
                field_id: String::new(),
                field_name: "assignee_machine".into(),
                operator: crate::state::FilterOperator::IsNotEmpty,
                value: vec![],
            },
            crate::state::FilterCondition {
                field_id: String::new(),
                field_name: "assignee_machine".into(),
                operator: crate::state::FilterOperator::IsNot,
                value: vec![cfg.machine_label.clone()],
            },
        ],
    };
    let records = client
        .bitable_search_records(&cfg.app_token, &cfg.table_id, &filter)
        .await?;
    // Defense-in-depth privacy guard. The server-side filter excludes
    // private workspaces indirectly: the publisher blanks `assignee_machine`
    // when a workspace goes private, and `assignee_machine IsNotEmpty` drops
    // those rows. That holds only if every writer is a current Ansambel
    // publisher. A row carrying `private = true` with its columns still
    // populated — manual Bitable edit, a publisher flush-lag window, or an
    // older publisher — would otherwise leak. Dropping any flagged-private
    // row here makes the read boundary the final authority on privacy,
    // independent of how the row was written.
    let rows = records
        .into_iter()
        .map(parse_record_to_row)
        .filter(|r| !r.private)
        // Repo-overlap filter (see the server-filter note above): keep only
        // rows whose canonical `repo_remote_url` matches one of the user's
        // locally-registered repos. Both sides are canonicalised by
        // `canonicalise_remote_url`, so a direct set lookup is exact.
        .filter(|r| local_repo_urls.contains(&r.repo_remote_url))
        .collect();
    Ok(FetchResult::Rows { rows })
}

// ─────────────────────────────────────────────────────────────────────
// IPC commands (Task 15 + Task 4)
// ─────────────────────────────────────────────────────────────────────

/// Shared process-wide cache for canonical remote URLs used by the
/// Phase 3a-4 reader. Distinct from the publisher's enricher cache so
/// the read path doesn't depend on the write path's state.
fn reader_remote_url_cache() -> &'static Arc<std::sync::Mutex<HashMap<String, String>>> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Arc<std::sync::Mutex<HashMap<String, String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Arc::new(std::sync::Mutex::new(HashMap::new())))
}

/// Phase 3a-4: fetch the rows the Team Activity sidebar and mirror view
/// should display. Builds the FilterSpec internally from AppState +
/// `team_activity_config.json`; the frontend passes no arguments.
///
/// Returns a tagged [`FetchResult`] enum — see the spec's error handling
/// matrix for what each variant maps to in the UI.
#[tauri::command]
pub async fn fetch_team_activity_rows(
    state: tauri::State<'_, Arc<std::sync::Mutex<crate::state::AppState>>>,
    app: tauri::AppHandle,
) -> std::result::Result<FetchResult, String> {
    let data_dir = data_dir_from(&app)?;
    let cfg = crate::persistence::team_activity_config::load_team_activity_config(&data_dir)
        .map_err(|e| format!("load team_activity_config: {e}"))?;

    // Early exit before constructing LarkClient: cheap path when disabled.
    let needs_client =
        matches!(&cfg, Some(c) if !c.app_token.is_empty() && !c.machine_label.is_empty());
    let client = if needs_client {
        let store = crate::commands::lark_auth::KeyringStore;
        match crate::commands::lark_auth::load_lark_config_inner(&data_dir, &store) {
            Ok(mut lark_cfg) => {
                if let Some(c) = &cfg {
                    lark_cfg.app_token = c.app_token.clone();
                    lark_cfg.table_id = c.table_id.clone();
                }
                Some(Arc::new(crate::platform::lark_client::LarkClient::new(
                    lark_cfg,
                )))
            }
            Err(_) => None, // global lark creds missing → inner treats as Disabled-equivalent
        }
    } else {
        None
    };

    let cache = reader_remote_url_cache();
    fetch_team_activity_rows_inner(state.inner().clone(), cfg, cache, client)
        .await
        .map_err(|e| e.to_string())
}

/// Returns the persisted `TeamActivityConfig`, or `Ok(None)` when no
/// config file exists or its `app_token` is empty (the persister treats
/// empty `app_token` as "publisher disabled"; see
/// [`crate::persistence::team_activity_config::load_team_activity_config`]).
///
/// Wraps the inner persistence call so the Tauri command layer's
/// `Result<_, String>` contract is honoured.
pub(crate) fn get_team_activity_config_inner(
    data_dir: &std::path::Path,
) -> crate::error::Result<Option<crate::state::TeamActivityConfig>> {
    crate::persistence::team_activity_config::load_team_activity_config(data_dir)
}

#[tauri::command]
pub async fn get_team_activity_config(
    app_handle: tauri::AppHandle,
) -> std::result::Result<Option<crate::state::TeamActivityConfig>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    get_team_activity_config_inner(&data_dir).map_err(|e| e.to_string())
}

/// Persists the team-activity config atomically.
///
/// TODO(phase-3a-3-followup): setting the config should ALSO restart the
/// publisher with the new config so a fresh app_token/table_id/machine_label
/// takes effect without a full app restart. This requires re-architecting
/// the publisher to be restartable (current shape is fire-and-forget
/// `tauri::async_runtime::spawn` from `lib.rs::setup`). For now, the
/// change is persisted-only — the running publisher keeps its initial
/// config until the user restarts the app.
pub(crate) fn set_team_activity_config_inner(
    data_dir: &std::path::Path,
    cfg: &crate::state::TeamActivityConfig,
) -> crate::error::Result<()> {
    crate::persistence::team_activity_config::save_team_activity_config(data_dir, cfg)
}

#[tauri::command]
pub async fn set_team_activity_config(
    cfg: crate::state::TeamActivityConfig,
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    set_team_activity_config_inner(&data_dir, &cfg).map_err(|e| e.to_string())
}

fn data_dir_from(app_handle: &tauri::AppHandle) -> std::result::Result<std::path::PathBuf, String> {
    use tauri::Manager;
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))
}

// ─────────────────────────────────────────────────────────────────────
// Task 16: setup_team_activity_table — idempotent column auto-provisioner
// ─────────────────────────────────────────────────────────────────────

/// Canonical column spec for the team-activity Bitable. The order is
/// significant for first-time provisioning (only the order of the POSTs;
/// Lark renders columns in creation order). `workspace_id` is the
/// primary key.
///
/// Lark `ui_type` numeric codes used:
/// - 1  = Text
/// - 3  = SingleSelect
/// - 5  = DateTime
/// - 7  = Checkbox
/// - 15 = URL
struct ColumnSpec {
    name: &'static str,
    field_type: u32,
    property: Option<serde_json::Value>,
}

fn team_activity_columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec {
            name: "workspace_id",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "repo_remote_url",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "repo_display_name",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "task_title",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "assignee_machine",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "ansambel_status",
            field_type: 3,
            property: Some(serde_json::json!({
                "options": [
                    {"name": "idle"},
                    {"name": "running"},
                    {"name": "waiting"},
                    {"name": "blocked"},
                    {"name": "pr_ready"},
                    {"name": "done"}
                ]
            })),
        },
        ColumnSpec {
            name: "last_activity_at",
            field_type: 5,
            property: None,
        },
        ColumnSpec {
            name: "last_message_preview",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "branch_name",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "diff_summary",
            field_type: 1,
            property: None,
        },
        ColumnSpec {
            name: "pr_url",
            field_type: 15,
            property: None,
        },
        ColumnSpec {
            name: "private",
            field_type: 7,
            property: None,
        },
    ]
}

/// Diff the canonical 12 columns against what's already in the table and
/// POST `bitable_create_field` for each missing one. Existing columns are
/// left untouched — type drift is NOT corrected (that would be destructive).
///
/// Returns the list of column names that were created (empty when the
/// table is already fully provisioned).
pub(crate) async fn setup_team_activity_table_inner(
    client: std::sync::Arc<crate::platform::lark_client::LarkClient>,
    app_token: &str,
    table_id: &str,
) -> crate::error::Result<Vec<String>> {
    let existing = client.bitable_list_fields(app_token, table_id).await?;
    let existing_names: std::collections::HashSet<String> =
        existing.iter().map(|f| f.field_name.clone()).collect();

    let mut created = Vec::new();
    for col in team_activity_columns() {
        if existing_names.contains(col.name) {
            tracing::debug!(
                column = col.name,
                "team-activity setup: column already exists"
            );
            continue;
        }
        client
            .bitable_create_field(app_token, table_id, col.name, col.field_type, col.property)
            .await?;
        tracing::info!(column = col.name, "team-activity setup: created column");
        created.push(col.name.to_string());
    }
    Ok(created)
}

#[tauri::command]
pub async fn setup_team_activity_table(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<String>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(&data_dir, &store)
        .map_err(|e| format!("global Lark credentials missing: {e}"))?;
    cfg.app_token = app_token.clone();
    cfg.table_id = table_id.clone();
    let client = std::sync::Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    setup_team_activity_table_inner(client, &app_token, &table_id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{TeamActivityConfig, WorkspaceEvent, WorkspaceStatus};
    use std::sync::Arc;
    use tokio::sync::{broadcast, Mutex};
    use tokio::time::Duration;

    fn make_cfg() -> TeamActivityConfig {
        TeamActivityConfig {
            app_token: "bascntest".into(),
            table_id: "tbl".into(),
            machine_label: "handoko@laptop-1".into(),
        }
    }

    // A test uploader records the snapshots it was called with into a Vec.
    fn recording_uploader() -> (UploaderFn, Arc<Mutex<Vec<RowSnapshot>>>) {
        let log: Arc<Mutex<Vec<RowSnapshot>>> = Arc::new(Default::default());
        let log_clone = log.clone();
        let uploader: UploaderFn = Arc::new(move |_ws_id, snap| {
            let log = log_clone.clone();
            Box::pin(async move {
                log.lock().await.push(snap);
                Ok::<String, crate::error::AppError>("recX".into())
            })
        });
        (uploader, log)
    }

    fn spawn_publisher(
        cfg: TeamActivityConfig,
        uploader: UploaderFn,
        debounce: Duration,
    ) -> broadcast::Sender<WorkspaceEvent> {
        let (tx, rx) = broadcast::channel::<WorkspaceEvent>(32);
        let publisher = Publisher {
            cfg,
            row_id_cache: Default::default(),
            uploader,
            enricher: None,
            debounce,
        };
        tokio::spawn(publisher.run(rx));
        tx
    }

    fn spawn_publisher_with_enricher(
        cfg: TeamActivityConfig,
        uploader: UploaderFn,
        enricher: EnricherFn,
        debounce: Duration,
    ) -> broadcast::Sender<WorkspaceEvent> {
        let (tx, rx) = broadcast::channel::<WorkspaceEvent>(32);
        let publisher = Publisher {
            cfg,
            row_id_cache: Default::default(),
            uploader,
            enricher: Some(enricher),
            debounce,
        };
        tokio::spawn(publisher.run(rx));
        tx
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_applies_enrichment_before_uploader_call() {
        // End-to-end: send a single StatusChanged event with an enricher
        // attached. After debounce, the recording uploader should observe
        // the snapshot with repo_remote_url + repo_display_name +
        // task_title filled in.
        let (uploader, log) = recording_uploader();
        let enricher: EnricherFn = Arc::new(|ws_id| RowEnrichment {
            repo_remote_url: Some(format!("https://github.com/x/{ws_id}")),
            repo_display_name: Some("my-repo".into()),
            task_title: Some("Fix bug".into()),
            branch_name: Some("feat/branch".into()),
        });
        let tx =
            spawn_publisher_with_enricher(make_cfg(), uploader, enricher, Duration::from_secs(3));
        let _ = tx.send(WorkspaceEvent::StatusChanged {
            workspace_id: "ws_enrich".into(),
            new_status: crate::state::WorkspaceStatus::Running,
        });
        tokio::time::sleep(Duration::from_secs(4)).await;
        let snapshots = log.lock().await;
        assert_eq!(snapshots.len(), 1);
        let snap = &snapshots[0];
        assert_eq!(snap.workspace_id, "ws_enrich");
        assert_eq!(
            snap.repo_remote_url.as_deref(),
            Some("https://github.com/x/ws_enrich")
        );
        assert_eq!(snap.repo_display_name.as_deref(), Some("my-repo"));
        assert_eq!(snap.task_title.as_deref(), Some("Fix bug"));
        assert_eq!(snap.branch_name.as_deref(), Some("feat/branch"));
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_skips_enrichment_under_privacy_lock() {
        // When the workspace is in private mode, enrichment must NOT
        // populate repo_remote_url / repo_display_name / task_title —
        // those fields are user-identifying once correlated with the
        // workspace_id, and the privacy escape promises a stripped row.
        let (uploader, log) = recording_uploader();
        let enricher: EnricherFn = Arc::new(|_| RowEnrichment {
            repo_remote_url: Some("https://github.com/leak/leak".into()),
            repo_display_name: Some("leak".into()),
            task_title: Some("leak".into()),
            branch_name: Some("leak".into()),
        });
        let tx =
            spawn_publisher_with_enricher(make_cfg(), uploader, enricher, Duration::from_secs(3));
        let _ = tx.send(WorkspaceEvent::PrivacyChanged {
            workspace_id: "ws_priv".into(),
            is_private: true,
        });
        tokio::time::sleep(Duration::from_secs(4)).await;
        let snapshots = log.lock().await;
        assert_eq!(snapshots.len(), 1);
        let snap = &snapshots[0];
        assert!(snap.private);
        assert!(snap.repo_remote_url.is_none(), "remote_url must stay None");
        assert!(
            snap.repo_display_name.is_none(),
            "display_name must stay None"
        );
        assert!(snap.task_title.is_none(), "task_title must stay None");
        assert!(snap.branch_name.is_none(), "branch_name must stay None");
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_debounces_rapid_status_changes_to_one_call() {
        let (uploader, log) = recording_uploader();
        let tx = spawn_publisher(make_cfg(), uploader, Duration::from_secs(3));

        for _ in 0..5 {
            tx.send(WorkspaceEvent::StatusChanged {
                workspace_id: "ws_a".into(),
                new_status: WorkspaceStatus::Running,
            })
            .unwrap();
        }
        // Advance past one full debounce window
        tokio::time::sleep(Duration::from_secs(4)).await;
        let calls = log.lock().await;
        assert_eq!(
            calls.len(),
            1,
            "expected exactly one upsert; got {}",
            calls.len()
        );
        assert_eq!(calls[0].workspace_id, "ws_a");
        assert_eq!(calls[0].ansambel_status.as_deref(), Some("running"));
        assert_eq!(
            calls[0].assignee_machine.as_deref(),
            Some("handoko@laptop-1")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_clears_sensitive_fields_when_private_flagged() {
        let (uploader, log) = recording_uploader();
        let tx = spawn_publisher(make_cfg(), uploader, Duration::from_secs(3));

        tx.send(WorkspaceEvent::StatusChanged {
            workspace_id: "ws_b".into(),
            new_status: WorkspaceStatus::Running,
        })
        .unwrap();
        tx.send(WorkspaceEvent::PrivacyChanged {
            workspace_id: "ws_b".into(),
            is_private: true,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_secs(4)).await;

        let calls = log.lock().await;
        let last = calls.last().expect("expected a snapshot to be published");
        assert!(last.assignee_machine.is_none(), "machine should be cleared");
        assert!(last.ansambel_status.is_none(), "status should be cleared");
        assert!(
            last.last_message_preview.is_none(),
            "preview should be cleared"
        );
        assert!(last.branch_name.is_none());
        assert!(last.diff_summary.is_none());
        assert!(last.pr_url.is_none());
        assert!(last.private, "private flag should be true");
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_truncates_and_sanitises_message_preview() {
        let (uploader, log) = recording_uploader();
        let tx = spawn_publisher(make_cfg(), uploader, Duration::from_secs(3));

        let leaky = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9".to_string()
            + " and api_key=sk-proj-aaaaaaaaaaaaaaaaaaaa";
        tx.send(WorkspaceEvent::MessageAppended {
            workspace_id: "ws_c".into(),
            role: "assistant".into(),
            text_preview: leaky,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_secs(4)).await;

        let calls = log.lock().await;
        let last = calls.last().expect("expected a snapshot");
        let preview = last
            .last_message_preview
            .as_deref()
            .expect("preview should be set");
        assert!(preview.contains("Bearer [REDACTED]"));
        assert!(preview.contains("[REDACTED-API-KEY]"));
        assert!(preview.chars().count() <= 201);
    }

    #[tokio::test(start_paused = true)]
    async fn private_lock_suppresses_status_events_until_toggled_off() {
        let (uploader, log) = recording_uploader();
        let tx = spawn_publisher(make_cfg(), uploader, Duration::from_secs(3));

        // Make workspace private FIRST, then send a status update.
        tx.send(WorkspaceEvent::PrivacyChanged {
            workspace_id: "ws_d".into(),
            is_private: true,
        })
        .unwrap();
        tx.send(WorkspaceEvent::StatusChanged {
            workspace_id: "ws_d".into(),
            new_status: WorkspaceStatus::Running,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_secs(4)).await;

        let calls = log.lock().await;
        let last = calls.last().expect("expected a snapshot");
        // The status update arrived AFTER private_lock was set, so
        // assignee_machine and ansambel_status must remain None.
        assert!(
            last.assignee_machine.is_none(),
            "private_lock should have suppressed assignee_machine update"
        );
        assert!(
            last.ansambel_status.is_none(),
            "private_lock should have suppressed status update"
        );
        assert!(last.private, "private flag should still be true");
    }

    #[tokio::test(start_paused = true)]
    async fn toggling_private_off_restores_field_updates() {
        let (uploader, log) = recording_uploader();
        let tx = spawn_publisher(make_cfg(), uploader, Duration::from_secs(3));

        // Sequence: private on → flush → private off → status → flush
        tx.send(WorkspaceEvent::PrivacyChanged {
            workspace_id: "ws_e".into(),
            is_private: true,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_secs(4)).await;

        tx.send(WorkspaceEvent::PrivacyChanged {
            workspace_id: "ws_e".into(),
            is_private: false,
        })
        .unwrap();
        tx.send(WorkspaceEvent::StatusChanged {
            workspace_id: "ws_e".into(),
            new_status: WorkspaceStatus::Running,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_secs(4)).await;

        let calls = log.lock().await;
        // Two snapshots should have been published.
        assert!(
            calls.len() >= 2,
            "expected at least 2 snapshots after on→off cycle; got {}",
            calls.len()
        );
        let last = calls.last().expect("expected a final snapshot");
        assert!(
            !last.private,
            "private flag should be false after toggling off"
        );
        assert_eq!(
            last.assignee_machine.as_deref(),
            Some("handoko@laptop-1"),
            "assignee_machine should be re-populated after lock released"
        );
        assert_eq!(
            last.ansambel_status.as_deref(),
            Some("running"),
            "status should be re-populated after lock released"
        );
    }

    // ── upload_with_retry ────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn upload_with_retry_succeeds_on_first_call() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let uploader: UploaderFn = Arc::new(move |_ws_id, _snap| {
            a.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok::<String, crate::error::AppError>("rec1".into()) })
        });
        let result = upload_with_retry(uploader, "ws_x".into(), RowSnapshot::default()).await;
        assert_eq!(result.unwrap(), "rec1");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn upload_with_retry_retries_once_after_429() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let uploader: UploaderFn = Arc::new(move |_ws_id, _snap| {
            let count = a.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if count == 0 {
                    Err(crate::error::AppError::LarkRateLimit {
                        retry_after_secs: 1,
                    })
                } else {
                    Ok::<String, crate::error::AppError>("recOK".into())
                }
            })
        });
        let result = upload_with_retry(uploader, "ws_y".into(), RowSnapshot::default()).await;
        assert_eq!(result.unwrap(), "recOK");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn upload_with_retry_surfaces_second_429() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let uploader: UploaderFn = Arc::new(move |_ws_id, _snap| {
            a.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Err::<String, crate::error::AppError>(crate::error::AppError::LarkRateLimit {
                    retry_after_secs: 1,
                })
            })
        });
        let result = upload_with_retry(uploader, "ws_z".into(), RowSnapshot::default()).await;
        assert!(matches!(
            result,
            Err(crate::error::AppError::LarkRateLimit { .. })
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn upload_with_retry_times_out_when_uploader_hangs() {
        let uploader: UploaderFn = Arc::new(|_ws_id, _snap| {
            Box::pin(async move {
                // Hang forever
                tokio::time::sleep(Duration::from_secs(3600)).await;
                Ok::<String, crate::error::AppError>("never".into())
            })
        });
        let result = upload_with_retry(uploader, "ws_q".into(), RowSnapshot::default()).await;
        assert!(matches!(
            result,
            Err(crate::error::AppError::LarkTimeout { .. })
        ));
    }

    // ── snapshot_to_fields ───────────────────────────────────────

    #[test]
    fn snapshot_to_fields_omits_none_values() {
        let snap = RowSnapshot {
            workspace_id: "ws_a".into(),
            repo_remote_url: Some("https://github.com/x/y".into()),
            repo_display_name: None, // omit
            task_title: Some("Fix login bug".into()),
            assignee_machine: Some("handoko@laptop-1".into()),
            ansambel_status: Some("running".into()),
            last_activity_at: Some(1_700_000_000_000),
            last_message_preview: None, // omit
            branch_name: Some("feat/x".into()),
            diff_summary: None, // omit
            pr_url: None,       // omit
            private: false,
        };
        let fields = snapshot_to_fields(&snap);
        // workspace_id is the primary key — always present
        assert_eq!(fields.get("workspace_id"), Some(&serde_json::json!("ws_a")));
        assert_eq!(
            fields.get("repo_remote_url"),
            Some(&serde_json::json!("https://github.com/x/y"))
        );
        assert!(
            fields.get("repo_display_name").is_none(),
            "None must be omitted"
        );
        assert!(fields.get("last_message_preview").is_none());
        assert!(fields.get("diff_summary").is_none());
        assert!(fields.get("pr_url").is_none());
        // private is a bool — always present even when false
        assert_eq!(fields.get("private"), Some(&serde_json::json!(false)));
    }

    #[test]
    fn snapshot_to_fields_includes_private_true() {
        let snap = RowSnapshot {
            workspace_id: "ws_b".into(),
            private: true,
            ..Default::default()
        };
        let fields = snapshot_to_fields(&snap);
        assert_eq!(fields.get("private"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn snapshot_to_fields_serializes_pr_url_as_lark_url_field_object() {
        // `pr_url` is created as a Bitable URL field (field_type 15). Writing
        // a bare string makes Lark reject the *entire* upsert with
        // 1254068 URLFieldConvFail — which also blocks every other column in
        // the same row from updating. The URL field requires an object
        // payload `{ "link", "text" }`.
        let snap = RowSnapshot {
            workspace_id: "ws_pr".into(),
            pr_url: Some("https://github.com/o/r/pull/2".into()),
            ..Default::default()
        };
        let fields = snapshot_to_fields(&snap);
        assert_eq!(
            fields.get("pr_url"),
            Some(&serde_json::json!({
                "link": "https://github.com/o/r/pull/2",
                "text": "https://github.com/o/r/pull/2",
            }))
        );
    }

    // ── build_lark_uploader integration (wiremock) ───────────────

    #[tokio::test]
    async fn build_lark_uploader_creates_then_updates_same_workspace() {
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Token endpoint
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_xyz",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        // POST → returns recNEW
        Mock::given(method("POST"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "record": { "record_id": "recNEW", "fields": {} } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        // PUT to recNEW → returns recNEW
        Mock::given(method("PUT"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/recNEW$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "record": { "record_id": "recNEW", "fields": {} } }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app_test_id".into(),
                app_secret: "app_test_secret".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cfg = TeamActivityConfig {
            app_token: "bascn".into(),
            table_id: "tbl".into(),
            machine_label: "test@machine".into(),
        };
        let cache: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let uploader = build_lark_uploader(client, cfg, cache.clone());
        let snap = RowSnapshot {
            workspace_id: "ws_t".into(),
            ansambel_status: Some("running".into()),
            ..Default::default()
        };
        // First call → POST (cache empty)
        let rec = (uploader)("ws_t".into(), snap.clone()).await.unwrap();
        assert_eq!(rec, "recNEW");
        assert_eq!(cache.lock().await.get("ws_t"), Some(&"recNEW".to_string()));
        // Second call: cache now has "ws_t" → "recNEW", uploader takes PUT path.
        let rec2 = (uploader)("ws_t".into(), snap).await.unwrap();
        assert_eq!(rec2, "recNEW");
    }

    #[tokio::test]
    async fn build_lark_uploader_reuses_existing_record_id_on_cache_miss() {
        // Regression: after app restart, the in-memory row_id_cache is empty.
        // The previous implementation always took POST on cache miss, which
        // created duplicate Bitable rows for the same workspace. The fix
        // searches Lark for an existing row keyed by workspace_id first.
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok", "tenant_access_token": "t_xyz", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // Search returns one existing record → uploader must NOT call POST.
        Mock::given(method("POST"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/search$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{ "record_id": "recEXISTING", "fields": {} }],
                    "has_more": false,
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        // PUT to recEXISTING is the expected path.
        Mock::given(method("PUT"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/recEXISTING$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "record": { "record_id": "recEXISTING", "fields": {} } }
            })))
            .expect(1)
            .mount(&server)
            .await;
        // Bare POST to /records must NOT be hit — duplicate creation would
        // be the bug we're guarding against.
        Mock::given(method("POST"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "record": { "record_id": "recDUPLICATE", "fields": {} } }
            })))
            .expect(0)
            .mount(&server)
            .await;

        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app_test_id".into(),
                app_secret: "app_test_secret".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cfg = TeamActivityConfig {
            app_token: "bascn".into(),
            table_id: "tbl".into(),
            machine_label: "test@machine".into(),
        };
        let cache: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let uploader = build_lark_uploader(client, cfg, cache.clone());
        let snap = RowSnapshot {
            workspace_id: "ws_t".into(),
            ansambel_status: Some("running".into()),
            ..Default::default()
        };
        let rec = (uploader)("ws_t".into(), snap).await.unwrap();
        assert_eq!(rec, "recEXISTING");
        assert_eq!(
            cache.lock().await.get("ws_t"),
            Some(&"recEXISTING".to_string())
        );
    }

    #[tokio::test]
    async fn build_lark_uploader_falls_back_to_post_when_search_returns_empty() {
        // Counterpart to the reuse test: when search finds no existing row,
        // uploader proceeds to POST (creating the row for the first time).
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok", "tenant_access_token": "t_xyz", "expire": 7200,
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/search$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [], "has_more": false }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "record": { "record_id": "recNEW", "fields": {} } }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app_test_id".into(),
                app_secret: "app_test_secret".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cfg = TeamActivityConfig {
            app_token: "bascn".into(),
            table_id: "tbl".into(),
            machine_label: "test@machine".into(),
        };
        let cache: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let uploader = build_lark_uploader(client, cfg, cache.clone());
        let snap = RowSnapshot {
            workspace_id: "ws_t".into(),
            ansambel_status: Some("running".into()),
            ..Default::default()
        };
        let rec = (uploader)("ws_t".into(), snap).await.unwrap();
        assert_eq!(rec, "recNEW");
    }

    // ── build_app_enricher (Phase 3a-3 follow-up) ───────────────────

    fn make_appstate_with_workspace_and_repo(
        ws_id: &str,
        repo_id: &str,
        repo_name: &str,
        repo_path: std::path::PathBuf,
    ) -> Arc<std::sync::Mutex<crate::state::AppState>> {
        use crate::state::{AppState, KanbanColumn, RepoInfo, WorkspaceInfo, WorkspaceStatus};
        let mut s = AppState::default();
        s.repos.insert(
            repo_id.into(),
            RepoInfo {
                id: repo_id.into(),
                name: repo_name.into(),
                path: repo_path,
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );
        s.workspaces.insert(
            ws_id.into(),
            WorkspaceInfo {
                id: ws_id.into(),
                repo_id: repo_id.into(),
                title: "Workspace title".into(),
                description: String::new(),
                branch: "feat/x".into(),
                base_branch: "main".into(),
                custom_branch: false,
                status: WorkspaceStatus::NotStarted,
                column: KanbanColumn::InProgress,
                created_at: 0,
                updated_at: 0,
                worktree_dir: std::path::PathBuf::new(),
                team_activity_private: false,
                task_ids: Vec::new(),
            },
        );
        Arc::new(std::sync::Mutex::new(s))
    }

    fn init_git_repo_with_origin(dir: &std::path::Path, origin_url: &str) {
        use std::process::Command;
        Command::new("git").arg("init").arg(dir).output().unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", origin_url])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn build_app_enricher_returns_repo_name_and_canonical_remote_url() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "git@github.com:Foo/Bar.git");
        let state = make_appstate_with_workspace_and_repo(
            "ws_e",
            "repo_e",
            "kelola-app",
            tmp.path().to_path_buf(),
        );
        let enricher = build_app_enricher(state);
        let out = enricher("ws_e");
        assert_eq!(out.repo_display_name.as_deref(), Some("kelola-app"));
        // canonicalise_remote_url strips `.git`; the host case is preserved
        // here because the canonicaliser only lowercases the host part.
        assert!(
            out.repo_remote_url
                .as_deref()
                .unwrap_or("")
                .contains("github.com"),
            "remote_url should contain canonical host, got {:?}",
            out.repo_remote_url
        );
        // Falls back to the workspace's own title (stamped at creation time
        // from the task card). The test helper sets title = "Workspace
        // title", which is what we expect here when there is no live Task
        // entry pointing to this workspace.
        assert_eq!(out.task_title.as_deref(), Some("Workspace title"));
        // The test helper stamps branch = "feat/x" on the workspace; the
        // enricher reads it directly from WorkspaceInfo.branch.
        assert_eq!(out.branch_name.as_deref(), Some("feat/x"));
    }

    #[test]
    fn build_app_enricher_falls_back_to_workspace_title_when_no_linked_task() {
        let state = make_appstate_with_workspace_and_repo(
            "ws_fallback",
            "repo_f",
            "f",
            std::path::PathBuf::from("/tmp/nonexistent_fb"),
        );
        let enricher = build_app_enricher(state);
        let out = enricher("ws_fallback");
        // No task entry was inserted; the workspace's own title field is
        // the right value to publish so locally-created workspaces don't
        // leave the column empty.
        assert_eq!(out.task_title.as_deref(), Some("Workspace title"));
    }

    #[test]
    fn build_app_enricher_prefers_linked_task_title_over_workspace_title() {
        // When the workspace has been edited in Lark/Jira after creation,
        // the live Task entry holds the up-to-date title. The workspace's
        // own snapshotted title becomes stale. We prefer Task.title.
        use crate::state::{KanbanColumn, Task};
        let state = make_appstate_with_workspace_and_repo(
            "ws_p",
            "repo_p",
            "p",
            std::path::PathBuf::from("/tmp/nonexistent_p"),
        );
        {
            let mut s = state.lock().unwrap();
            s.tasks.insert(
                "tk_p".into(),
                Task {
                    id: "tk_p".into(),
                    repo_id: "repo_p".into(),
                    workspace_id: Some("ws_p".into()),
                    title: "Live updated title".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let enricher = build_app_enricher(state);
        let out = enricher("ws_p");
        assert_eq!(out.task_title.as_deref(), Some("Live updated title"));
    }

    #[test]
    fn build_app_enricher_returns_task_title_when_linked() {
        use crate::state::{KanbanColumn, Task};
        let state = make_appstate_with_workspace_and_repo(
            "ws_t",
            "repo_t",
            "kelola-app",
            std::path::PathBuf::from("/tmp/nonexistent_for_remote_url"),
        );
        {
            let mut s = state.lock().unwrap();
            s.tasks.insert(
                "tk_1".into(),
                Task {
                    id: "tk_1".into(),
                    repo_id: "repo_t".into(),
                    workspace_id: Some("ws_t".into()),
                    title: "Fix login bug".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let enricher = build_app_enricher(state);
        let out = enricher("ws_t");
        assert_eq!(out.task_title.as_deref(), Some("Fix login bug"));
    }

    #[test]
    fn build_app_enricher_returns_default_when_workspace_unknown() {
        let state = make_appstate_with_workspace_and_repo(
            "ws_known",
            "repo_x",
            "x",
            std::path::PathBuf::from("/tmp/nonexistent"),
        );
        let enricher = build_app_enricher(state);
        let out = enricher("ws_does_not_exist");
        assert!(out.repo_remote_url.is_none());
        assert!(out.repo_display_name.is_none());
        assert!(out.task_title.is_none());
    }

    #[test]
    fn build_app_enricher_caches_remote_url_across_calls() {
        // After the first call, the remote_url_cache should be populated.
        // Subsequent calls reuse it even if the repo no longer has an
        // origin remote on disk (we simulate by checking the cache hit
        // returns the same value after deleting the repo dir).
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "git@github.com:Foo/Bar.git");
        let state = make_appstate_with_workspace_and_repo(
            "ws_c",
            "repo_c",
            "kelola-app",
            tmp.path().to_path_buf(),
        );
        let enricher = build_app_enricher(state);
        let first = enricher("ws_c");
        // Drop the repo directory; second call must still return the
        // cached canonical URL rather than failing the git shell-out.
        drop(tmp);
        let second = enricher("ws_c");
        assert_eq!(first.repo_remote_url, second.repo_remote_url);
        assert!(second.repo_remote_url.is_some());
    }

    // ── Phase 3a-4: parse_record_to_row ─────────────────────────────

    #[test]
    fn parse_record_to_row_extracts_all_fields_with_correct_types() {
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recA".into(),
            fields: serde_json::json!({
                "workspace_id": "ws_a",
                "repo_remote_url": "https://github.com/x/y",
                "repo_display_name": "y",
                "task_title": "Fix bug",
                "assignee_machine": "alice@laptop",
                "ansambel_status": "running",
                "last_activity_at": 1_700_000_000_000_i64,
                "last_message_preview": "doing thing",
                "branch_name": "feat/x",
                "diff_summary": "+10 -3",
                "pr_url": "https://github.com/x/y/pull/42",
                "private": false,
            }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.workspace_id, "ws_a");
        assert_eq!(row.repo_remote_url, "https://github.com/x/y");
        assert_eq!(row.repo_display_name, "y");
        assert_eq!(row.task_title, "Fix bug");
        assert_eq!(row.assignee_machine, "alice@laptop");
        assert_eq!(row.ansambel_status, "running");
        assert_eq!(row.last_activity_at, 1_700_000_000_000);
        assert_eq!(row.last_message_preview, "doing thing");
        assert_eq!(row.branch_name, "feat/x");
        assert_eq!(row.diff_summary, "+10 -3");
        assert_eq!(row.pr_url, "https://github.com/x/y/pull/42");
        assert!(!row.private);
    }

    #[test]
    fn parse_record_to_row_extracts_segmented_array_shape_from_search_endpoint() {
        // The Bitable *search* endpoint (the one the reader uses) returns
        // text fields as segmented rich-text arrays — `[{"type":"text",
        // "text":"..."}]` — not plain strings like the list endpoint. A
        // naive `as_str()` parse silently drops every text field, which is
        // exactly the "row shows up empty" bug. Single-select arrives as a
        // bare option string. Date stays a millisecond number.
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recSeg".into(),
            fields: serde_json::json!({
                "workspace_id": [{"type": "text", "text": "ws_seg"}],
                "repo_remote_url": [{"type": "text", "text": "https://github.com/x/y"}],
                "repo_display_name": [{"type": "text", "text": "y"}],
                "task_title": [{"type": "text", "text": "Fix bug"}],
                "assignee_machine": [{"type": "text", "text": "alice@laptop"}],
                "ansambel_status": "running",
                "last_activity_at": 1_700_000_000_000_i64,
                "last_message_preview": [{"type": "text", "text": "doing thing"}],
                "branch_name": [{"type": "text", "text": "feat/x"}],
                "diff_summary": [{"type": "text", "text": "+10 -3"}],
                "pr_url": [{"type": "text", "text": "https://github.com/x/y/pull/42"}],
                "private": false,
            }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.workspace_id, "ws_seg");
        assert_eq!(row.repo_remote_url, "https://github.com/x/y");
        assert_eq!(row.repo_display_name, "y");
        assert_eq!(row.task_title, "Fix bug");
        assert_eq!(row.assignee_machine, "alice@laptop");
        assert_eq!(row.ansambel_status, "running");
        assert_eq!(row.last_activity_at, 1_700_000_000_000);
        assert_eq!(row.last_message_preview, "doing thing");
        assert_eq!(row.branch_name, "feat/x");
        assert_eq!(row.diff_summary, "+10 -3");
        assert_eq!(row.pr_url, "https://github.com/x/y/pull/42");
        assert!(!row.private);
    }

    #[test]
    fn parse_record_to_row_extracts_single_select_object_shape() {
        // Single-select can also arrive as an option object
        // `{"id":"opt_x","text":"pr_ready"}` (native search shape). The
        // status must resolve to the option text, not empty.
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recSel".into(),
            fields: serde_json::json!({
                "workspace_id": [{"type": "text", "text": "ws_sel"}],
                "ansambel_status": {"id": "optABC", "text": "pr_ready"},
            }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.workspace_id, "ws_sel");
        assert_eq!(row.ansambel_status, "pr_ready");
    }

    #[test]
    fn parse_record_to_row_defaults_missing_strings_to_empty() {
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recM".into(),
            fields: serde_json::json!({ "workspace_id": "ws_m" }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.workspace_id, "ws_m");
        assert_eq!(row.repo_remote_url, "");
        assert_eq!(row.repo_display_name, "");
        assert_eq!(row.task_title, "");
        assert_eq!(row.assignee_machine, "");
        assert_eq!(row.ansambel_status, "");
        assert_eq!(row.last_activity_at, 0);
        assert_eq!(row.last_message_preview, "");
        assert_eq!(row.branch_name, "");
        assert_eq!(row.diff_summary, "");
        assert_eq!(row.pr_url, "");
        assert!(!row.private);
    }

    #[test]
    fn parse_record_to_row_coerces_datetime_epoch_ms_to_i64() {
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recT".into(),
            fields: serde_json::json!({
                "workspace_id": "ws_t",
                "last_activity_at": 1_705_000_000_000_i64,
            }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.last_activity_at, 1_705_000_000_000);
    }

    #[test]
    fn parse_record_to_row_handles_private_true_value() {
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recP".into(),
            fields: serde_json::json!({
                "workspace_id": "ws_p",
                "private": true,
            }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert!(row.private);
    }

    #[test]
    fn parse_record_to_row_handles_malformed_record_gracefully() {
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recX".into(),
            fields: serde_json::json!({
                "workspace_id": "ws_x",
                "last_activity_at": "not a number",
                "private": "not a bool",
            }),
            extra: Default::default(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.workspace_id, "ws_x");
        assert_eq!(row.last_activity_at, 0);
        assert!(!row.private);
    }

    // ── Phase 3a-4: read_remote_url_cached ─────────────────────────

    #[test]
    fn read_remote_url_cached_returns_canonical_url_for_repo() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "git@github.com:Foo/Bar.git");
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let url = read_remote_url_cached(&cache, "repo_a", tmp.path());
        assert!(
            url.contains("github.com"),
            "expected canonical github.com host, got {url:?}"
        );
        assert!(
            !url.ends_with(".git"),
            "canonicaliser strips .git, got {url:?}"
        );
    }

    #[test]
    fn read_remote_url_cached_returns_empty_string_when_no_origin() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .arg("init")
            .arg(tmp.path())
            .output()
            .unwrap();
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let url = read_remote_url_cached(&cache, "repo_b", tmp.path());
        assert_eq!(url, "");
    }

    #[test]
    fn read_remote_url_cached_hits_cache_on_second_call() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "git@github.com:Foo/Bar.git");
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let first = read_remote_url_cached(&cache, "repo_c", tmp.path());
        drop(tmp);
        let second = read_remote_url_cached(&cache, "repo_c", std::path::Path::new("/tmp/gone"));
        assert_eq!(first, second);
        assert!(!second.is_empty());
    }

    #[test]
    fn read_remote_url_cached_caches_empty_negative_results_too() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .arg("init")
            .arg(tmp.path())
            .output()
            .unwrap();
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let first = read_remote_url_cached(&cache, "repo_d", tmp.path());
        assert_eq!(first, "");
        assert_eq!(cache.lock().unwrap().get("repo_d"), Some(&"".to_string()));
    }

    // ── Phase 3a-4: fetch_team_activity_rows_inner ─────────────────

    fn make_team_cfg(app_token: &str, machine_label: &str) -> TeamActivityConfig {
        TeamActivityConfig {
            app_token: app_token.into(),
            table_id: "tbl_test".into(),
            machine_label: machine_label.into(),
        }
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_disabled_when_config_none() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let result = fetch_team_activity_rows_inner(state, None, &cache, None)
            .await
            .unwrap();
        assert_eq!(result, FetchResult::Disabled);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_disabled_when_app_token_empty() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None)
            .await
            .unwrap();
        assert_eq!(result, FetchResult::Disabled);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_machine_label_empty_when_label_blank() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn_x", "");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None)
            .await
            .unwrap();
        assert_eq!(result, FetchResult::MachineLabelEmpty);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_no_overlap_when_state_has_no_repos() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn_x", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None)
            .await
            .unwrap();
        assert_eq!(result, FetchResult::NoOverlapRepos);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_no_overlap_when_repos_have_no_origin() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .arg("init")
            .arg(tmp.path())
            .output()
            .unwrap();
        let state = Arc::new(std::sync::Mutex::new(crate::state::AppState {
            repos: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "repo_x".into(),
                    crate::state::RepoInfo {
                        id: "repo_x".into(),
                        name: "x".into(),
                        path: tmp.path().to_path_buf(),
                        gh_profile: None,
                        default_branch: "main".into(),
                        created_at: 0,
                        updated_at: 0,
                        scripts: Vec::new(),
                    },
                );
                m
            },
            ..Default::default()
        }));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn_x", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None)
            .await
            .unwrap();
        assert_eq!(result, FetchResult::NoOverlapRepos);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_rows_on_lark_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok", "tenant_access_token": "tkn", "expire": 7200,
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(wiremock::matchers::path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/search$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "rec1",
                            "fields": {
                                "workspace_id": "ws_remote",
                                "repo_remote_url": "https://github.com/foo/bar",
                                "repo_display_name": "bar",
                                "task_title": "Hello",
                                "assignee_machine": "bob@laptop",
                                "ansambel_status": "running",
                                "last_activity_at": 1_700_000_000_000_i64,
                                "last_message_preview": "doing things",
                                "branch_name": "feat/x",
                                "diff_summary": "",
                                "pr_url": "",
                                "private": false,
                            }
                        }
                    ],
                    "has_more": false,
                }
            })))
            .mount(&server)
            .await;

        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "https://github.com/foo/bar.git");
        let state = Arc::new(std::sync::Mutex::new(crate::state::AppState {
            repos: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "repo_real".into(),
                    crate::state::RepoInfo {
                        id: "repo_real".into(),
                        name: "bar".into(),
                        path: tmp.path().to_path_buf(),
                        gh_profile: None,
                        default_branch: "main".into(),
                        created_at: 0,
                        updated_at: 0,
                        scripts: Vec::new(),
                    },
                );
                m
            },
            ..Default::default()
        }));
        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app".into(),
                app_secret: "sec".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, Some(client))
            .await
            .unwrap();
        match result {
            FetchResult::Rows { rows } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].workspace_id, "ws_remote");
                assert_eq!(rows[0].assignee_machine, "bob@laptop");
            }
            other => panic!("expected Rows, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_filters_repos_client_side_with_multiple_repos() {
        // Regression: Lark's `is` operator rejects a multi-value array
        // (`repo_remote_url is [urlA, urlB]` → 1254018 InvalidFilter), which
        // broke the whole poll the moment a second repo was registered. The
        // repo-overlap filter must therefore run client-side: the server
        // filter keeps only the two `assignee_machine` conditions, and rows
        // are narrowed to the local repo set in Rust.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok", "tenant_access_token": "tkn", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // The mock ignores the filter and returns three rows: two for local
        // repos (bar, baz) and one for a repo the user does NOT have (other).
        Mock::given(method("POST"))
            .and(wiremock::matchers::path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/search$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        { "record_id": "recBar", "fields": {
                            "workspace_id": "ws_bar",
                            "repo_remote_url": "https://github.com/foo/bar",
                            "assignee_machine": "bob@laptop", "private": false } },
                        { "record_id": "recBaz", "fields": {
                            "workspace_id": "ws_baz",
                            "repo_remote_url": "https://github.com/foo/baz",
                            "assignee_machine": "carol@laptop", "private": false } },
                        { "record_id": "recOther", "fields": {
                            "workspace_id": "ws_other",
                            "repo_remote_url": "https://github.com/foo/other",
                            "assignee_machine": "dave@laptop", "private": false } },
                    ],
                    "has_more": false,
                }
            })))
            .mount(&server)
            .await;

        let tmp_a = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp_a.path(), "https://github.com/foo/bar.git");
        let tmp_b = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp_b.path(), "https://github.com/foo/baz.git");
        let mk = |id: &str, name: &str, path: std::path::PathBuf| crate::state::RepoInfo {
            id: id.into(),
            name: name.into(),
            path,
            gh_profile: None,
            default_branch: "main".into(),
            created_at: 0,
            updated_at: 0,
            scripts: Vec::new(),
        };
        let state = Arc::new(std::sync::Mutex::new(crate::state::AppState {
            repos: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "repo_a".into(),
                    mk("repo_a", "bar", tmp_a.path().to_path_buf()),
                );
                m.insert(
                    "repo_b".into(),
                    mk("repo_b", "baz", tmp_b.path().to_path_buf()),
                );
                m
            },
            ..Default::default()
        }));
        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app".into(),
                app_secret: "sec".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, Some(client))
            .await
            .unwrap();
        match result {
            FetchResult::Rows { mut rows } => {
                rows.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
                let ids: Vec<&str> = rows.iter().map(|r| r.workspace_id.as_str()).collect();
                assert_eq!(
                    ids,
                    vec!["ws_bar", "ws_baz"],
                    "only rows for locally-registered repos should survive; ws_other must be dropped"
                );
            }
            other => panic!("expected Rows, got {other:?}"),
        }

        // Lock the regression: the server filter must NOT carry a
        // `repo_remote_url` condition (multi-value `is` → 1254018).
        let reqs = server.received_requests().await.unwrap();
        let search = reqs
            .iter()
            .find(|r| r.url.path().ends_with("/records/search"))
            .expect("a search request was sent");
        let body: serde_json::Value = serde_json::from_slice(&search.body).unwrap();
        let conditions = body["filter"]["conditions"].as_array().unwrap();
        assert!(
            conditions
                .iter()
                .all(|c| c["field_name"].as_str() != Some("repo_remote_url")),
            "repo_remote_url must be filtered client-side, not via a multi-value server `is`: {body}"
        );
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_drops_rows_flagged_private() {
        // Defense-in-depth: a row whose `private` checkbox is set must never
        // reach the sidebar, even when its sensitive columns are still
        // populated (manual Bitable edit, a publisher lag window, or an
        // older publisher version). The normal privacy path relies on the
        // publisher blanking `assignee_machine`, which the server-side
        // `IsNotEmpty` filter then excludes; this read-boundary guard backs
        // that up so a flagged-private row can never leak regardless of how
        // it landed in the table.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok", "tenant_access_token": "tkn", "expire": 7200,
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(wiremock::matchers::path_regex(
                r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/search$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "recPriv",
                            "fields": {
                                "workspace_id": "ws_priv",
                                "repo_remote_url": "https://github.com/foo/bar",
                                "repo_display_name": "bar",
                                "task_title": "secret work",
                                "assignee_machine": "bob@laptop",
                                "ansambel_status": "running",
                                "last_activity_at": 1_700_000_000_000_i64,
                                "last_message_preview": "should not leak",
                                "branch_name": "feat/secret",
                                "diff_summary": "",
                                "pr_url": "",
                                "private": true,
                            }
                        }
                    ],
                    "has_more": false,
                }
            })))
            .mount(&server)
            .await;

        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "https://github.com/foo/bar.git");
        let state = Arc::new(std::sync::Mutex::new(crate::state::AppState {
            repos: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "repo_real".into(),
                    crate::state::RepoInfo {
                        id: "repo_real".into(),
                        name: "bar".into(),
                        path: tmp.path().to_path_buf(),
                        gh_profile: None,
                        default_branch: "main".into(),
                        created_at: 0,
                        updated_at: 0,
                        scripts: Vec::new(),
                    },
                );
                m
            },
            ..Default::default()
        }));
        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app".into(),
                app_secret: "sec".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, Some(client))
            .await
            .unwrap();
        match result {
            FetchResult::Rows { rows } => {
                assert!(
                    rows.is_empty(),
                    "private-flagged row leaked into the sidebar: {rows:?}"
                );
            }
            other => panic!("expected Rows, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_propagates_lark_auth_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "code": 99991663, "msg": "invalid app_secret",
            })))
            .mount(&server)
            .await;

        let tmp = tempfile::tempdir().unwrap();
        init_git_repo_with_origin(tmp.path(), "https://github.com/foo/bar.git");
        let state = Arc::new(std::sync::Mutex::new(crate::state::AppState {
            repos: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "repo_real".into(),
                    crate::state::RepoInfo {
                        id: "repo_real".into(),
                        name: "bar".into(),
                        path: tmp.path().to_path_buf(),
                        gh_profile: None,
                        default_branch: "main".into(),
                        created_at: 0,
                        updated_at: 0,
                        scripts: Vec::new(),
                    },
                );
                m
            },
            ..Default::default()
        }));
        let client = Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app".into(),
                app_secret: "sec".into(),
                app_token: "bascn".into(),
                table_id: "tbl".into(),
                base_url: server.uri(),
            },
        ));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, Some(client)).await;
        assert!(result.is_err(), "expected auth error, got {result:?}");
    }

    // ── get/set_team_activity_config (Task 15) ────────────────────

    #[test]
    fn get_team_activity_config_returns_none_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let result = get_team_activity_config_inner(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn set_then_get_team_activity_config_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: "bascn_set".into(),
            table_id: "tbl_set".into(),
            machine_label: "alice@host-1".into(),
        };
        set_team_activity_config_inner(tmp.path(), &cfg).unwrap();
        let loaded = get_team_activity_config_inner(tmp.path()).unwrap();
        assert_eq!(loaded, Some(cfg));
    }

    #[test]
    fn set_team_activity_config_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let first = TeamActivityConfig {
            app_token: "bascn_a".into(),
            table_id: "tbl_a".into(),
            machine_label: "old@host".into(),
        };
        set_team_activity_config_inner(tmp.path(), &first).unwrap();
        let second = TeamActivityConfig {
            app_token: "bascn_b".into(),
            table_id: "tbl_b".into(),
            machine_label: "new@host".into(),
        };
        set_team_activity_config_inner(tmp.path(), &second).unwrap();
        let loaded = get_team_activity_config_inner(tmp.path()).unwrap();
        assert_eq!(loaded, Some(second));
    }

    // ── setup_team_activity_table (Task 16) ───────────────────────

    async fn mount_token(server: &wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_xyz",
                "expire": 7200,
            })))
            .mount(server)
            .await;
    }

    fn make_client_for(uri: &str) -> Arc<crate::platform::lark_client::LarkClient> {
        Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "app_test_id".into(),
                app_secret: "app_test_secret".into(),
                app_token: "appA".into(),
                table_id: "tblA".into(),
                base_url: uri.into(),
            },
        ))
    }

    #[tokio::test]
    async fn setup_team_activity_table_creates_all_columns_when_table_empty() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        mount_token(&server).await;
        // Empty field list → all 12 columns must be created.
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "data": { "items": [], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;
        // Expect exactly 12 POSTs (one per canonical column).
        Mock::given(method("POST"))
            .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {
                        "field_id": "fld_new",
                        "field_name": "stub",
                        "type": 1
                    }
                }
            })))
            .expect(12)
            .mount(&server)
            .await;

        let client = make_client_for(&server.uri());
        let created = setup_team_activity_table_inner(client, "appA", "tblA")
            .await
            .expect("setup ok");
        assert_eq!(
            created.len(),
            12,
            "expected all 12 canonical columns to be created"
        );
        // Spot-check: primary key + the SingleSelect status column + the
        // bool privacy column appear in the created list.
        assert!(created.contains(&"workspace_id".to_string()));
        assert!(created.contains(&"ansambel_status".to_string()));
        assert!(created.contains(&"private".to_string()));
    }

    #[tokio::test]
    async fn setup_team_activity_table_is_idempotent_when_columns_already_exist() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        mount_token(&server).await;
        // Return all 12 canonical columns as already-present. The field_id /
        // type values don't have to match the canonical spec — the inner
        // only diffs by field_name (a name-collision is treated as "exists").
        let existing_items: Vec<serde_json::Value> = [
            "workspace_id",
            "repo_remote_url",
            "repo_display_name",
            "task_title",
            "assignee_machine",
            "ansambel_status",
            "last_activity_at",
            "last_message_preview",
            "branch_name",
            "diff_summary",
            "pr_url",
            "private",
        ]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            serde_json::json!({
                "field_id": format!("fld_existing_{i}"),
                "field_name": name,
                "type": 1,
                "is_primary": *name == "workspace_id",
                "property": null,
            })
        })
        .collect();
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "data": { "items": existing_items, "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;
        // Zero POSTs expected — any create call is a regression.
        Mock::given(method("POST"))
            .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let client = make_client_for(&server.uri());
        let created = setup_team_activity_table_inner(client, "appA", "tblA")
            .await
            .expect("setup ok");
        assert!(
            created.is_empty(),
            "expected zero columns created on idempotent call; got {created:?}"
        );
    }

    #[tokio::test]
    async fn setup_team_activity_table_only_creates_missing_columns() {
        // Pre-existing: workspace_id + ansambel_status + private. Setup
        // should create the other 9.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        mount_token(&server).await;
        let existing_items: Vec<serde_json::Value> = ["workspace_id", "ansambel_status", "private"]
            .iter()
            .map(|name| {
                serde_json::json!({
                    "field_id": format!("fld_{name}"),
                    "field_name": name,
                    "type": 1,
                    "is_primary": *name == "workspace_id",
                    "property": null,
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "data": { "items": existing_items, "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/open-apis/bitable/v1/apps/appA/tables/tblA/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {
                        "field_id": "fld_new",
                        "field_name": "stub",
                        "type": 1
                    }
                }
            })))
            .expect(9)
            .mount(&server)
            .await;

        let client = make_client_for(&server.uri());
        let created = setup_team_activity_table_inner(client, "appA", "tblA")
            .await
            .expect("setup ok");
        assert_eq!(created.len(), 9);
        // None of the three already-present columns should be in `created`.
        for already_there in ["workspace_id", "ansambel_status", "private"] {
            assert!(
                !created.contains(&already_there.to_string()),
                "did not expect {already_there} in created list"
            );
        }
    }

    #[test]
    fn team_activity_columns_spec_has_12_canonical_entries() {
        // Lock the canonical column count + names so a future edit to
        // `team_activity_columns()` that adds/drops a column is caught by
        // the test suite, not by production drift.
        let cols = team_activity_columns();
        assert_eq!(cols.len(), 12);
        let names: Vec<&str> = cols.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec![
                "workspace_id",
                "repo_remote_url",
                "repo_display_name",
                "task_title",
                "assignee_machine",
                "ansambel_status",
                "last_activity_at",
                "last_message_preview",
                "branch_name",
                "diff_summary",
                "pr_url",
                "private",
            ]
        );
        // ansambel_status must be SingleSelect (type 3) with 6 options.
        let status_col = cols
            .iter()
            .find(|c| c.name == "ansambel_status")
            .expect("ansambel_status present");
        assert_eq!(status_col.field_type, 3);
        let opts = status_col
            .property
            .as_ref()
            .and_then(|p| p.get("options"))
            .and_then(|o| o.as_array())
            .expect("ansambel_status has options array");
        assert_eq!(opts.len(), 6);
    }

    #[test]
    fn get_team_activity_config_returns_none_when_app_token_empty() {
        // Mirrors the persistence-layer semantics: empty app_token = disabled =
        // surfaced as `None` to the IPC caller so the UI can treat it the
        // same as "no config".
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: String::new(),
            table_id: "tbl".into(),
            machine_label: "m".into(),
        };
        set_team_activity_config_inner(tmp.path(), &cfg).unwrap();
        let loaded = get_team_activity_config_inner(tmp.path()).unwrap();
        assert!(loaded.is_none());
    }

    // ── Phase 3a-4: Tauri command wiring ───────────────────────────

    #[test]
    fn fetch_team_activity_rows_is_a_tauri_command() {
        // Symbol-presence check: confirms the function exists and is
        // reachable as a public symbol. The #[tauri::command] macro wraps
        // the function in generated glue so a direct fn-pointer coercion
        // would not compile; we use the same type_name_of_val pattern as
        // the other command registration tests.
        let _ = std::any::type_name_of_val(&fetch_team_activity_rows);
    }
}
