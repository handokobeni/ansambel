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

pub struct Publisher {
    pub cfg: TeamActivityConfig,
    /// workspace_id → Bitable record_id, populated as the uploader returns ids.
    pub row_id_cache: Arc<Mutex<HashMap<String, String>>>,
    pub uploader: UploaderFn,
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
                        let snap = state.snapshot.clone();
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
            debounce,
        };
        tokio::spawn(publisher.run(rx));
        tx
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
}
