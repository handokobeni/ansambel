# Phase 3a-3 Workspace State Publisher — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish each active workspace's state (status, message preview, diff
summary, PR link) to a shared Lark Bitable in near real-time so teammates can
see who's working on what. Read-side (sidebar / watch view) is the next phase.

**Architecture:** A dedicated team-activity Bitable table (separate from
per-repo task bindings). Backend `state_publisher` async task subscribes to a
`broadcast::Sender<WorkspaceEvent>` on AppState, aggregates events per-workspace
with a 3-second debounce, sanitises the message preview, and upserts a row keyed
by `workspace_id`. Reuses existing global Lark credentials.

**Tech Stack:** Rust (tokio broadcast channel, regex), Svelte 5 (settings UI),
existing `LarkClient` (extended with `bitable_upsert_row`), atomic JSON
persistence.

**Spec:**
`docs/superpowers/specs/2026-05-19-phase-3a-3-workspace-state-publisher-design.md`

---

## Task 1: Canonical repo URL helper

**Files:**

- Create: `src-tauri/src/platform/repo_identity.rs`
- Modify: `src-tauri/src/platform/mod.rs` (add `pub mod repo_identity;`)
- Test: in the same file under `#[cfg(test)] mod tests`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_repo_url_strips_trailing_dot_git() {
        assert_eq!(
            canonicalise_remote_url("git@github.com:handokobeni/ansambel.git"),
            "git@github.com:handokobeni/ansambel"
        );
    }

    #[test]
    fn canonical_repo_url_lowercases_host_only() {
        assert_eq!(
            canonicalise_remote_url("https://GitHub.com/Handoko/Repo"),
            "https://github.com/Handoko/Repo"
        );
    }

    #[test]
    fn canonical_repo_url_passes_through_when_empty() {
        assert_eq!(canonicalise_remote_url(""), "");
    }

    #[test]
    fn canonical_repo_url_trims_whitespace() {
        assert_eq!(
            canonicalise_remote_url("  https://github.com/x/y.git\n"),
            "https://github.com/x/y"
        );
    }
}
```

- [ ] **Step 2: Run tests — expect compile error**

Run: `cd src-tauri && cargo test --lib platform::repo_identity::tests` Expected:
FAIL with "cannot find function `canonicalise_remote_url`".

- [ ] **Step 3: Implement**

```rust
//! Canonical identifier for a git repository, agreed across machines.
//! Used by the team-activity publisher so engineer A's `repo_abc` and
//! engineer B's `repo_xyz` for the same upstream resolve to the same row.

use crate::error::{AppError, Result};
use std::path::Path;
use std::process::Command;

/// Returns the canonical remote URL for the repository at `repo_path`, or
/// an empty string when the repo has no `origin` remote (solo / not-yet-
/// pushed work).
pub fn read_origin_url(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| AppError::Git(format!("git remote get-url origin: {e}")))?;
    if !output.status.success() {
        // No origin configured — surface as empty, not an error.
        return Ok(String::new());
    }
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(canonicalise_remote_url(&raw))
}

/// Pure normaliser: trim whitespace, strip trailing `.git`, lowercase host.
/// Host detection is best-effort — handles `https://`, `http://`, and SSH
/// `git@host:path` forms.
pub fn canonicalise_remote_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    // https://Host/path → https://host/path
    if let Some(rest) = without_git.strip_prefix("https://") {
        if let Some((host, path)) = rest.split_once('/') {
            return format!("https://{}/{}", host.to_ascii_lowercase(), path);
        }
        return format!("https://{}", rest.to_ascii_lowercase());
    }
    if let Some(rest) = without_git.strip_prefix("http://") {
        if let Some((host, path)) = rest.split_once('/') {
            return format!("http://{}/{}", host.to_ascii_lowercase(), path);
        }
        return format!("http://{}", rest.to_ascii_lowercase());
    }
    // SSH form is case-sensitive on the path portion; leave alone.
    without_git.to_string()
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cd src-tauri && cargo test --lib platform::repo_identity::tests` Expected:
4 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/repo_identity.rs src-tauri/src/platform/mod.rs
git commit -m "feat(phase-3a-3-publisher): canonical repo URL for cross-machine identity"
```

---

## Task 2: Sanitiser (Rust)

**Files:**

- Create: `src-tauri/src/sanitize.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod sanitize;`)
- Test: same file

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_openai_style_api_key() {
        let s = "Token: sk-proj-abcdefghijklmnopqrstuvwx";
        assert_eq!(
            sanitize_message_preview(s, 200),
            "Token: [REDACTED-API-KEY]"
        );
    }

    #[test]
    fn redacts_bearer_token() {
        let s = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let out = sanitize_message_preview(s, 200);
        assert!(out.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_named_credentials_case_insensitive() {
        let s = "api_key: secret123 and password = topsecret";
        let out = sanitize_message_preview(s, 200);
        assert!(!out.contains("secret123"));
        assert!(!out.contains("topsecret"));
    }

    #[test]
    fn truncates_to_max_chars_with_ellipsis() {
        let s = "a".repeat(500);
        let out = sanitize_message_preview(&s, 200);
        assert_eq!(out.len(), 201); // 200 + ellipsis
        assert!(out.ends_with('…'));
    }

    #[test]
    fn short_strings_pass_through_untruncated() {
        let s = "Hello world";
        assert_eq!(sanitize_message_preview(s, 200), "Hello world");
    }
}
```

- [ ] **Step 2: Run — expect compile error**

Run: `cd src-tauri && cargo test --lib sanitize::tests` Expected: FAIL with
"cannot find function `sanitize_message_preview`".

- [ ] **Step 3: Implement**

```rust
//! Sanitises text before it leaves the local machine for Lark Bitable.
//! Two-layer defence: the frontend mirror (src/lib/sanitize.ts) also runs
//! the same redactions before persisting to messages.jsonl, so anything
//! reaching this function should already be clean — but we never trust
//! that.

use once_cell::sync::Lazy;
use regex::Regex;

static API_KEY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"sk-[A-Za-z0-9_\-]{20,}").unwrap());
static BEARER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Bearer\s+[A-Za-z0-9._\-]+").unwrap());
static JWT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"eyJ[A-Za-z0-9._\-]{20,}").unwrap());
static NAMED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(api[_-]?key|secret|password|token)\s*[:=]\s*\S+")
        .unwrap()
});

/// Redacts common credential patterns, then truncates the result to
/// `max_chars` characters (Unicode-safe), appending `…` when truncated.
pub fn sanitize_message_preview(input: &str, max_chars: usize) -> String {
    let mut s = API_KEY.replace_all(input, "[REDACTED-API-KEY]").into_owned();
    s = BEARER.replace_all(&s, "Bearer [REDACTED]").into_owned();
    s = JWT.replace_all(&s, "[REDACTED-JWT]").into_owned();
    s = NAMED
        .replace_all(&s, |caps: &regex::Captures| {
            format!("{}: [REDACTED]", &caps[1])
        })
        .into_owned();
    truncate_chars(&s, max_chars)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        let mut out = head;
        out.push('…');
        out
    } else {
        head
    }
}
```

Add to `Cargo.toml` if missing: `once_cell` (likely already a transitive dep —
verify before adding). `regex` should also be available; check with
`cargo tree -e normal | grep ^regex`.

- [ ] **Step 4: Run — expect pass**

Run: `cd src-tauri && cargo test --lib sanitize::tests` Expected: 5 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/sanitize.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(phase-3a-3-publisher): credential sanitiser for outbound message previews"
```

---

## Task 3: WorkspaceEvent + broadcast channel

**Files:**

- Modify: `src-tauri/src/state.rs` (add `WorkspaceEvent` enum + `event_tx` on
  AppState)
- Test: `src-tauri/src/state.rs` `#[cfg(test)]` block

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn workspace_event_broadcasts_to_multiple_subscribers() {
    use tokio::sync::broadcast;
    let (tx, _) = broadcast::channel::<WorkspaceEvent>(32);
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();
    let event = WorkspaceEvent::StatusChanged {
        workspace_id: "ws_test".into(),
        new_status: crate::state::WorkspaceStatus::Running,
    };
    tx.send(event.clone()).unwrap();
    assert_eq!(rx1.recv().await.unwrap(), event);
    assert_eq!(rx2.recv().await.unwrap(), event);
}
```

- [ ] **Step 2: Run — expect compile error**

Run: `cd src-tauri && cargo test --lib state::tests::workspace_event_broadcasts`
Expected: FAIL with "cannot find type `WorkspaceEvent`".

- [ ] **Step 3: Implement**

```rust
// Append after the existing Task / Workspace declarations.

#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceEvent {
    StatusChanged {
        workspace_id: String,
        new_status: WorkspaceStatus,
    },
    MessageAppended {
        workspace_id: String,
        role: String,        // "user" | "assistant" | "system" | "tool"
        text_preview: String, // already sanitised by caller
    },
    FileTouched {
        workspace_id: String,
    },
    PrCreated {
        workspace_id: String,
        url: String,
    },
    BranchChanged {
        workspace_id: String,
        branch_name: String,
    },
    DiffSummaryUpdated {
        workspace_id: String,
        summary: String,
    },
    PrivacyChanged {
        workspace_id: String,
        is_private: bool,
    },
}

impl WorkspaceEvent {
    pub fn workspace_id(&self) -> &str {
        match self {
            WorkspaceEvent::StatusChanged { workspace_id, .. }
            | WorkspaceEvent::MessageAppended { workspace_id, .. }
            | WorkspaceEvent::FileTouched { workspace_id }
            | WorkspaceEvent::PrCreated { workspace_id, .. }
            | WorkspaceEvent::BranchChanged { workspace_id, .. }
            | WorkspaceEvent::DiffSummaryUpdated { workspace_id, .. }
            | WorkspaceEvent::PrivacyChanged { workspace_id, .. } => workspace_id,
        }
    }
}

/// Broadcast channel sender registered as separate Tauri state so command
/// handlers can emit without holding the AppState lock. Created in
/// `lib.rs::run()` setup() with capacity 256.
pub type WorkspaceEventTx = std::sync::Arc<tokio::sync::broadcast::Sender<WorkspaceEvent>>;
```

In `src-tauri/src/lib.rs::run()`'s `.setup()`:

```rust
let (event_tx, _) = tokio::sync::broadcast::channel::<crate::state::WorkspaceEvent>(256);
let event_tx: crate::state::WorkspaceEventTx = std::sync::Arc::new(event_tx);
app.manage(event_tx.clone());
```

- [ ] **Step 4: Run — expect pass**

Run: `cd src-tauri && cargo test --lib state::tests` Expected: existing tests +
1 new PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "feat(phase-3a-3-publisher): WorkspaceEvent enum + broadcast channel on AppState"
```

---

## Task 4: TeamActivityConfig type + persistence

**Files:**

- Modify: `src-tauri/src/state.rs` (add `TeamActivityConfig` struct)
- Create: `src-tauri/src/persistence/team_activity_config.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Test: same persistence file

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TeamActivityConfig;

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: "bascntest".into(),
            table_id: "tblTeamActivity".into(),
            machine_label: "handoko@laptop-1".into(),
        };
        save_team_activity_config(tmp.path(), &cfg).unwrap();
        let loaded = load_team_activity_config(tmp.path()).unwrap();
        assert_eq!(loaded, Some(cfg));
    }

    #[test]
    fn load_returns_none_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_team_activity_config(tmp.path()).unwrap(), None);
    }

    #[test]
    fn load_returns_none_when_app_token_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: String::new(),
            table_id: "tbl".into(),
            machine_label: "m".into(),
        };
        save_team_activity_config(tmp.path(), &cfg).unwrap();
        // Empty app_token = disabled = treat as no config
        assert_eq!(load_team_activity_config(tmp.path()).unwrap(), None);
    }
}
```

- [ ] **Step 2: Run — expect compile error**

Run: `cd src-tauri && cargo test --lib persistence::team_activity_config::tests`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `state.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct TeamActivityConfig {
    pub app_token: String,
    pub table_id: String,
    pub machine_label: String,
}
```

In `persistence/team_activity_config.rs`:

```rust
use crate::error::Result;
use crate::persistence::atomic::write_atomic;
use crate::state::TeamActivityConfig;
use std::path::{Path, PathBuf};

fn cfg_path(data_dir: &Path) -> PathBuf {
    data_dir.join("team_activity_config.json")
}

pub fn save_team_activity_config(data_dir: &Path, cfg: &TeamActivityConfig) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(cfg)?;
    write_atomic(&cfg_path(data_dir), &bytes)
}

/// Returns Some(cfg) only when the file exists AND `app_token` is non-empty.
/// Empty token = user disabled publishing.
pub fn load_team_activity_config(data_dir: &Path) -> Result<Option<TeamActivityConfig>> {
    let path = cfg_path(data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let cfg: TeamActivityConfig = serde_json::from_slice(&bytes)?;
    if cfg.app_token.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(cfg))
}

pub fn delete_team_activity_config(data_dir: &Path) -> Result<()> {
    let path = cfg_path(data_dir);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
```

Add `pub mod team_activity_config;` to `src-tauri/src/persistence/mod.rs`.

- [ ] **Step 4: Run — expect pass**

Run: `cd src-tauri && cargo test --lib persistence::team_activity_config::tests`
Expected: 3 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/persistence/team_activity_config.rs src-tauri/src/persistence/mod.rs
git commit -m "feat(phase-3a-3-publisher): TeamActivityConfig persistence (atomic JSON)"
```

---

## Task 5: bitable_upsert_row method on LarkClient

**Files:**

- Modify: `src-tauri/src/platform/lark_client.rs`
- Test: in the same file

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn bitable_upsert_row_creates_when_record_id_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(
            r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": { "record": { "record_id": "recNEW", "fields": {} } }
        })))
        .mount(&server)
        .await;
    mount_token(&server).await;
    let client = make_client(&server.uri()).await;
    let mut fields = serde_json::Map::new();
    fields.insert("workspace_id".into(), serde_json::json!("ws_x"));
    let rec_id = client
        .bitable_upsert_row("bascn", "tbl", "", fields)
        .await
        .unwrap();
    assert_eq!(rec_id, "recNEW");
}

#[tokio::test]
async fn bitable_upsert_row_updates_when_record_id_given() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_regex(
            r"^/open-apis/bitable/v1/apps/[^/]+/tables/[^/]+/records/recABC$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": { "record": { "record_id": "recABC", "fields": {} } }
        })))
        .mount(&server)
        .await;
    mount_token(&server).await;
    let client = make_client(&server.uri()).await;
    let mut fields = serde_json::Map::new();
    fields.insert("ansambel_status".into(), serde_json::json!("running"));
    let rec_id = client
        .bitable_upsert_row("bascn", "tbl", "recABC", fields)
        .await
        .unwrap();
    assert_eq!(rec_id, "recABC");
}
```

- [ ] **Step 2: Run — expect compile error**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_upsert`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `LarkClient` impl block:

```rust
/// Upsert a Bitable row. If `record_id` is empty, POST to create; otherwise
/// PUT to update. Returns the record_id (new or existing).
pub async fn bitable_upsert_row(
    &self,
    app_token: &str,
    table_id: &str,
    record_id: &str,
    fields: serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    let body = serde_json::json!({ "fields": fields });
    if record_id.is_empty() {
        let resp: serde_json::Value = self
            .post_json(
                &format!(
                    "/open-apis/bitable/v1/apps/{}/tables/{}/records",
                    app_token, table_id
                ),
                &body,
            )
            .await?;
        Ok(resp
            .pointer("/data/record/record_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::Lark("bitable create: response missing record_id".into())
            })?
            .to_string())
    } else {
        let resp: serde_json::Value = self
            .put_json(
                &format!(
                    "/open-apis/bitable/v1/apps/{}/tables/{}/records/{}",
                    app_token, table_id, record_id
                ),
                &body,
            )
            .await?;
        Ok(resp
            .pointer("/data/record/record_id")
            .and_then(|v| v.as_str())
            .unwrap_or(record_id)
            .to_string())
    }
}
```

Add `put_json` helper if missing (mirror `post_json`).

- [ ] **Step 4: Run — expect pass**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_upsert`
Expected: 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/lark_client.rs
git commit -m "feat(phase-3a-3-publisher): bitable_upsert_row + put_json on LarkClient"
```

---

## Task 6: state_publisher async task (core loop, no events yet)

**Files:**

- Create: `src-tauri/src/commands/team_activity.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Test: same file

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{TeamActivityConfig, WorkspaceEvent, WorkspaceStatus};
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tokio::time::{sleep, Duration};

    fn make_cfg() -> TeamActivityConfig {
        TeamActivityConfig {
            app_token: "bascntest".into(),
            table_id: "tbl".into(),
            machine_label: "test@machine".into(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_debounces_rapid_status_changes_to_one_call() {
        let (tx, rx) = broadcast::channel::<WorkspaceEvent>(32);
        let publish_log: Arc<tokio::sync::Mutex<Vec<RowSnapshot>>> = Arc::new(Default::default());
        let log_clone = publish_log.clone();
        let publisher = Publisher {
            cfg: make_cfg(),
            row_id_cache: Default::default(),
            uploader: Arc::new(move |_ws_id, snap| {
                let log = log_clone.clone();
                async move {
                    log.lock().await.push(snap);
                    Ok("recX".to_string())
                }
                .boxed()
            }),
            debounce: Duration::from_secs(3),
        };
        tokio::spawn(publisher.run(rx));

        for _ in 0..5 {
            tx.send(WorkspaceEvent::StatusChanged {
                workspace_id: "ws_a".into(),
                new_status: WorkspaceStatus::Running,
            })
            .unwrap();
        }
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert_eq!(publish_log.lock().await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_clears_sensitive_fields_when_private_flagged() {
        let (tx, rx) = broadcast::channel::<WorkspaceEvent>(32);
        let publish_log: Arc<tokio::sync::Mutex<Vec<RowSnapshot>>> = Arc::new(Default::default());
        let log_clone = publish_log.clone();
        let publisher = Publisher {
            cfg: make_cfg(),
            row_id_cache: Default::default(),
            uploader: Arc::new(move |_ws_id, snap| {
                let log = log_clone.clone();
                async move {
                    log.lock().await.push(snap);
                    Ok("recY".to_string())
                }
                .boxed()
            }),
            debounce: Duration::from_secs(3),
        };
        tokio::spawn(publisher.run(rx));

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
        let log = publish_log.lock().await;
        let last = log.last().unwrap();
        assert!(last.assignee_machine.is_none());
        assert!(last.ansambel_status.is_none());
        assert!(last.last_message_preview.is_none());
        assert_eq!(last.private, true);
    }

    #[tokio::test(start_paused = true)]
    async fn publisher_truncates_and_sanitises_message_preview() {
        // … similar setup; assert preview is ≤200 chars and contains [REDACTED-API-KEY]
    }
}
```

- [ ] **Step 2: Run — expect compile error**

Run: `cd src-tauri && cargo test --lib commands::team_activity::tests` Expected:
FAIL.

- [ ] **Step 3: Implement**

```rust
//! Workspace state publisher: subscribes to WorkspaceEvent broadcast,
//! aggregates per-workspace updates over a 3-second debounce window, and
//! upserts the matching row in the team-activity Bitable table.

use crate::error::Result;
use crate::sanitize::sanitize_message_preview;
use crate::state::{TeamActivityConfig, WorkspaceEvent, WorkspaceStatus};
use futures::future::BoxFuture;
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{Duration, Instant};

const MESSAGE_PREVIEW_MAX: usize = 200;

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

#[derive(Clone, Default)]
struct AggregatedState {
    snapshot: RowSnapshot,
    dirty: bool,
    private_lock: bool, // once true, snapshot is cleared on next flush
}

impl AggregatedState {
    fn merge(&mut self, event: &WorkspaceEvent, machine_label: &str) {
        let now_ms = chrono::Utc::now().timestamp_millis();
        self.snapshot.last_activity_at = Some(now_ms);
        self.snapshot.workspace_id = event.workspace_id().to_string();
        self.dirty = true;
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
            WorkspaceEvent::PrivacyChanged { is_private, .. } => {
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

pub type UploaderFn =
    Arc<dyn Fn(String, RowSnapshot) -> BoxFuture<'static, Result<String>> + Send + Sync>;

pub struct Publisher {
    pub cfg: TeamActivityConfig,
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
                    let Ok(event) = msg else { continue };
                    let ws_id = event.workspace_id().to_string();
                    let entry = aggregated.entry(ws_id.clone()).or_default();
                    entry.merge(&event, &self.cfg.machine_label);
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
                        let Some(state) = aggregated.get_mut(&ws_id) else { continue };
                        let snap = state.snapshot.clone();
                        state.dirty = false;
                        last_flush.insert(ws_id.clone(), now);
                        let uploader = self.uploader.clone();
                        let cache = self.row_id_cache.clone();
                        tokio::spawn(async move {
                            match (uploader)(ws_id.clone(), snap).await {
                                Ok(rec_id) => {
                                    cache.lock().await.insert(ws_id, rec_id);
                                }
                                Err(e) => tracing::warn!(error = %e, "publish failed"),
                            }
                        });
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run — expect pass**

Run: `cd src-tauri && cargo test --lib commands::team_activity::tests` Expected:
3 PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/team_activity.rs src-tauri/src/commands/mod.rs
git commit -m "feat(phase-3a-3-publisher): debounced state_publisher async task core"
```

---

## Task 7: 429 retry handling in publisher uploader

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (add retry wrapper)
- Test: same file

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn uploader_retries_once_after_429_with_retry_after() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let attempts = Arc::new(AtomicU32::new(0));
    let a = attempts.clone();
    let uploader: UploaderFn = Arc::new(move |_ws_id, _snap| {
        let count = a.fetch_add(1, Ordering::SeqCst);
        async move {
            if count == 0 {
                Err(AppError::LarkRateLimit { retry_after_secs: 1 })
            } else {
                Ok("recOK".to_string())
            }
        }
        .boxed()
    });
    let result = upload_with_retry(uploader, "ws_x".into(), RowSnapshot::default()).await;
    assert_eq!(result.unwrap(), "recOK");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}
```

- [ ] **Step 2: Run — expect fail**

Expected: function `upload_with_retry` and variant `AppError::LarkRateLimit` not
found.

- [ ] **Step 3: Implement**

Add to `error.rs`:

```rust
#[error("Lark rate limit; retry after {retry_after_secs}s")]
LarkRateLimit { retry_after_secs: u64 },
```

Map 429 responses in `LarkClient` to this variant (extend `send_with_retry` if
needed; current code already retries — confirm + augment to surface remaining
429s as `LarkRateLimit` rather than `Lark(...)`.).

In `team_activity.rs`:

```rust
pub async fn upload_with_retry(
    uploader: UploaderFn,
    ws_id: String,
    snap: RowSnapshot,
) -> Result<String> {
    match (uploader)(ws_id.clone(), snap.clone()).await {
        Ok(rec_id) => Ok(rec_id),
        Err(AppError::LarkRateLimit { retry_after_secs }) => {
            tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
            (uploader)(ws_id, snap).await
        }
        Err(e) => Err(e),
    }
}
```

Wire `upload_with_retry` into the spawn block inside `Publisher::run`.

- [ ] **Step 4: Run — expect pass**

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(phase-3a-3-publisher): single 429 retry with Retry-After backoff"
```

---

## Task 8: Bitable uploader binding (publisher → LarkClient)

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (real uploader fn that calls
  `LarkClient::bitable_upsert_row`)
- Test: integration test against MockServer

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn publisher_creates_then_updates_same_row_for_same_workspace() {
    // MockServer accepts a POST then a PUT — assert POST count == 1, PUT count == 1.
}
```

- [ ] **Step 2-5: implement
      `build_lark_uploader(client: Arc<LarkClient>, cfg: TeamActivityConfig, row_id_cache: ...)`
      which returns an `UploaderFn`; commit.**

```bash
git commit -am "feat(phase-3a-3-publisher): bind publisher uploader to LarkClient.upsert_row"
```

---

## Task 9: Publisher spawn at app startup

**Files:**

- Modify: `src-tauri/src/lib.rs` (`run()` setup hook)
- Test: integration smoke

- [ ] **Step 1: Write failing integration test**

Smoke test that spawning the publisher with a `None` config silently no-ops (the
task exits cleanly).

- [ ] **Step 3: Implement**

In `lib.rs::run()` after the broadcast channel is created:

```rust
// Spawn team-activity publisher if configured.
let tac = crate::persistence::team_activity_config::load_team_activity_config(&data_dir)
    .ok()
    .flatten();
if let Some(cfg) = tac {
    let client = /* build LarkClient with global creds + this cfg.app_token */;
    let row_id_cache = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let uploader = crate::commands::team_activity::build_lark_uploader(
        client, cfg.clone(), row_id_cache.clone(),
    );
    let publisher = crate::commands::team_activity::Publisher {
        cfg, row_id_cache, uploader, debounce: Duration::from_secs(3),
    };
    let rx = event_tx.subscribe();
    tauri::async_runtime::spawn(publisher.run(rx));
} else {
    tracing::info!("team-activity publisher disabled (no config)");
}
```

- [ ] **Step 4: Run check + tests**

```bash
cd src-tauri && cargo check --lib && cargo test --lib
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(phase-3a-3-publisher): spawn state_publisher on app startup"
```

---

## Task 10: Emit StatusChanged from agent core

**Files:**

- Modify: `src-tauri/src/commands/agent_core.rs` (call `event_tx.send` on status
  transitions)
- Test: same file (unit test asserting the send happens)

- [ ] **Step 1-5: TDD red-green-commit. Test mocks broadcast::Sender, asserts
      that running → waiting transition fires `StatusChanged`.**

```bash
git commit -am "feat(phase-3a-3-publisher): emit StatusChanged from agent_core on status flip"
```

---

## Task 11: Emit MessageAppended from agent core

**Files:**

- Modify: `src-tauri/src/commands/agent_core.rs`
- Test: same

- [ ] **TDD: on persisting an assistant message, emit MessageAppended with
      text_preview = first 400 chars of message text. Sanitiser runs at
      publisher layer.**

```bash
git commit -am "feat(phase-3a-3-publisher): emit MessageAppended from agent_core"
```

---

## Task 12: Emit FileTouched + BranchChanged + DiffSummaryUpdated

**Files:**

- Modify: `src-tauri/src/commands/file_io.rs`, `commands/workspace.rs`,
  `commands/git.rs`
- Tests: per-file unit tests

- [ ] **TDD: each emission point has a test mocking the broadcast Sender and
      asserting the right variant fires.**

```bash
git commit -am "feat(phase-3a-3-publisher): emit FileTouched/BranchChanged/DiffSummaryUpdated"
```

---

## Task 13: Emit PrCreated

**Files:**

- Modify: `src-tauri/src/commands/workspace.rs` (PR-create handler)
- Test: same

```bash
git commit -am "feat(phase-3a-3-publisher): emit PrCreated after pr_create succeeds"
```

---

## Task 14: Frontend sanitiser mirror

**Files:**

- Create: `src/lib/sanitize.ts`
- Test: `src/lib/sanitize.test.ts`

- [ ] **TDD: identical regex set + truncate behaviour as Rust side. Same
      redaction outputs (`[REDACTED-API-KEY]`, etc.). Used by message persist
      layer before write to messages.jsonl.**

```bash
git commit -am "feat(phase-3a-3-publisher): frontend sanitize.ts mirror of Rust regex set"
```

---

## Task 15: get/set_team_activity_config Tauri commands

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (add commands)
- Modify: `src-tauri/src/lib.rs` (register handlers)
- Modify: `src/lib/ipc.ts` (typed wrappers)
- Modify: `src/lib/types.ts` (`TeamActivityConfig` type)
- Test: per file

```rust
#[tauri::command]
pub async fn get_team_activity_config(
    state: State<'_, AppDirState>,
) -> Result<Option<TeamActivityConfig>, String> { /* ... */ }

#[tauri::command]
pub async fn set_team_activity_config(
    cfg: TeamActivityConfig,
    state: State<'_, AppDirState>,
    publisher_handle: State<'_, PublisherHandle>,
) -> Result<(), String> {
    // persist, then RECREATE the publisher with new config (drop old task)
}
```

```bash
git commit -am "feat(phase-3a-3-publisher): get/set_team_activity_config IPC commands"
```

---

## Task 16: setup_team_activity_table Tauri command

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs`
- Test: unit test against MockServer

- [ ] **TDD: command takes app_token, creates the 12-column table via Lark
      schema API, returns the new table_id. Idempotent — checks for existing
      columns first.**

```bash
git commit -am "feat(phase-3a-3-publisher): setup_team_activity_table auto-create helper"
```

---

## Task 17: TeamActivitySettings.svelte component

**Files:**

- Create: `src/lib/components/lark/TeamActivitySettings.svelte`
- Modify: `src/lib/components/SettingsDialog.svelte` (mount below LarkSettings)
- Test: `src/lib/components/lark/TeamActivitySettings.test.ts`

- [ ] **TDD: 5 tests minimum**

```ts
- renders disabled state when config absent
- saves app_token + table_id + machine_label
- "Setup table schema" button calls setup_team_activity_table
- disconnect clears config + stops publisher
- status indicator shows last publish time when config active
```

UI per the spec mockup. Reuse the field styling from `LarkBindingWizard`.

```bash
git commit -am "feat(phase-3a-3-publisher): TeamActivitySettings component in Settings dialog"
```

---

## Task 18: Per-workspace privacy toggle

**Files:**

- Modify: `src/lib/components/workspace/WorkspaceView.svelte` (add toggle in
  header area or kebab menu)
- Modify: `src-tauri/src/state.rs` (`Workspace.team_activity_private: bool`)
- Modify: relevant Tauri command for the toggle
- Tests: per file

- [ ] **TDD: 3 tests minimum.**
  - toggle on emits `PrivacyChanged { is_private: true }`
  - persisted across app restart
  - the workspace's row clears in Bitable on toggle (integration)

```bash
git commit -am "feat(phase-3a-3-publisher): per-workspace privacy toggle"
```

---

## Task 19: E2E smoke (workspace publish round-trip)

**Files:**

- Create: `tests/e2e/phase-3a-3-publisher/publisher-roundtrip.spec.ts`

- [ ] **Env-gated under `ANSAMBEL_LARK_FIXTURE=1`. Drives:**
  1. Set team activity config
  2. Create workspace, change its status
  3. Mock Bitable POST; assert row payload contains expected fields

```bash
git commit -am "test(phase-3a-3-publisher): E2E publish round-trip smoke spec"
```

---

## Task 20: Documentation + journal

**Files:**

- Create: `docs/lark-team-activity-schema.md` (12-column table spec for
  engineers setting up manually)
- Create: `journal/2026-05-XX-phase-3a-3-publisher.md`

- [ ] **Spec the columns + types so a Lark admin can provision the table
      manually without running the setup command.**

- [ ] **Journal entry follows the existing format (see
      `journal/2026-05-19-phase-3a-3-1-filter-aware-lark-binding.md`).**

```bash
git commit -am "docs: team-activity schema reference + phase-3a-3 publisher journal"
```

---

## Self-review checklist

After all tasks land:

- [ ] Coverage on changed files ≥ 95% (per CLAUDE.md gate)
- [ ] No `console.log` in production paths (use `logging.ts` / `tracing::*`)
- [ ] No `.unwrap()` or `.expect()` outside `#[cfg(test)]`
- [ ] All `#[tauri::command]` return `Result<T, String>`
- [ ] Two-layer sanitisation works (frontend + backend); test by sending a raw
      `sk-...` through the agent flow and confirming it never reaches the
      Bitable row
- [ ] Publisher gracefully no-ops when config absent (verified manually + via
      Task 9 integration smoke)
- [ ] Rate limit: drive a 100-event burst through one workspace, assert only 1
      Bitable write fires within the 3-second window
