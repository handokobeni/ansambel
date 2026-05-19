# Phase 3a-4 — Team Activity Sidebar + Watch View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface "who on the team is working on what right now" in Ansambel by
reading the dedicated `ansambel_team_activity` Bitable (the table Phase 3a-3
publishes to). Two surfaces: a collapsible sidebar panel that lists active team
workspaces for the user's local repos, and a mirror view (replaces main content)
that opens when a row is clicked.

**Architecture:** Backend exposes one new Tauri command
`fetch_team_activity_rows` that builds a `FilterSpec` from `AppState.repos` +
`team_activity_config.machine_label` (no frontend args), shells
`git remote get-url origin` once per repo (cached), calls
`bitable_search_records`, and returns a tagged `FetchResult` enum. Frontend
store polls every 10s with `document.visibilityState`-aware pause, maintains a
`SvelteMap<workspace_id, TeamActivityRow>`, and exposes a `selectedWorkspaceId`
that App.svelte routes to a `TeamWorkspaceMirror` component (replaces main
content, TitleBar shows "Watching:" label + back button).

**Tech Stack:** Tauri v2 + Svelte 5 runes + Bun + reqwest (Rust HTTP, already a
dep) + wiremock (Rust test, already a dep) + vitest (TS test) + Playwright
(E2E).

**Spec:**
`docs/superpowers/specs/2026-05-19-phase-3a-4-team-activity-sidebar-design.md`.

**Branch:** `feat/phase-3a-4-team-activity-sidebar` (already created off `main`,
spec already committed at `fb72dcb`).

---

## File map

**Created:**

- `src/lib/stores/team-activity.svelte.ts` — read-side store (rows + status +
  selection + poll loop)
- `src/lib/stores/team-activity.svelte.test.ts`
- `src/lib/components/sidebar/TeamActivityPanel.svelte`
- `src/lib/components/sidebar/TeamActivityPanel.svelte.test.ts`
- `src/lib/components/team/TeamWorkspaceMirror.svelte`
- `src/lib/components/team/TeamWorkspaceMirror.svelte.test.ts`
- `src/lib/github-url.ts` — `githubBranchUrl(remoteUrl, branch)` helper
- `src/lib/github-url.test.ts`
- `tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts` — env-gated E2E

**Modified:**

- `src-tauri/src/commands/team_activity.rs` — add `TeamActivityRow`,
  `FetchResult`, `parse_record_to_row`, `fetch_team_activity_rows_inner`,
  `fetch_team_activity_rows` Tauri command, `read_remote_url_cached`
- `src-tauri/src/lib.rs` — register `fetch_team_activity_rows` in the command
  handler
- `src/lib/types.ts` — add `TeamActivityRow` and `FetchResult` TS types matching
  the Rust shapes
- `src/lib/ipc.ts` — add `api.teamActivity.fetchRows()` wrapper
- `src/lib/ipc.test.ts` — assert the wrapper invokes the correct command
- `src/lib/components/Sidebar.svelte` — mount `TeamActivityPanel` below the
  WORKSPACES section + drive `teamActivity.start()`/`stop()`
- `src/lib/components/TitleBar.svelte` — when
  `teamActivity.selectedWorkspaceId !== null`, replace the Plan/Work toggle with
  a "Watching: ..." label + back button
- `src/App.svelte` — route to `TeamWorkspaceMirror` when `selectedWorkspaceId`
  set
- `journal/2026-05-19-phase-3a-4-team-activity-sidebar.md` — phase journal

---

### Task 1: Define `TeamActivityRow`, `FetchResult`, and `parse_record_to_row`

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (append at end of module,
  before `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block (search for
`// ── get/set_team_activity_config (Task 15) ────────────────────` and add
tests above it, or before the closing `}` of the tests module):

```rust
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
            })
            .as_object()
            .unwrap()
            .clone(),
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
    fn parse_record_to_row_defaults_missing_strings_to_empty() {
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recM".into(),
            fields: serde_json::json!({ "workspace_id": "ws_m" })
                .as_object()
                .unwrap()
                .clone(),
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
        // Lark may return datetime as JSON number (i64) or string. We treat
        // it as i64; anything else defaults to 0.
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recT".into(),
            fields: serde_json::json!({
                "workspace_id": "ws_t",
                "last_activity_at": 1_705_000_000_000_i64,
            })
            .as_object()
            .unwrap()
            .clone(),
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
            })
            .as_object()
            .unwrap()
            .clone(),
        };
        let row = parse_record_to_row(record);
        assert!(row.private);
    }

    #[test]
    fn parse_record_to_row_handles_malformed_record_gracefully() {
        // Wrong types (string where number expected, etc.) coerce to defaults
        // rather than panicking.
        let record = crate::platform::lark_client::BitableRecord {
            record_id: "recX".into(),
            fields: serde_json::json!({
                "workspace_id": "ws_x",
                "last_activity_at": "not a number",
                "private": "not a bool",
            })
            .as_object()
            .unwrap()
            .clone(),
        };
        let row = parse_record_to_row(record);
        assert_eq!(row.workspace_id, "ws_x");
        assert_eq!(row.last_activity_at, 0);
        assert!(!row.private);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib parse_record_to_row` Expected: 5
failures, each reporting
`cannot find function parse_record_to_row in this scope` or similar.

- [ ] **Step 3: Implement `TeamActivityRow`, `FetchResult`, and
      `parse_record_to_row`**

Insert after the `RowSnapshot` struct definition (around line 54). Search for
`/// Per-workspace aggregation state held while the loop debounces.` and add the
new types BEFORE that comment:

```rust
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
    let f = &record.fields;
    let s = |key: &str| -> String {
        f.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    TeamActivityRow {
        workspace_id: s("workspace_id"),
        repo_remote_url: s("repo_remote_url"),
        repo_display_name: s("repo_display_name"),
        task_title: s("task_title"),
        assignee_machine: s("assignee_machine"),
        ansambel_status: s("ansambel_status"),
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib parse_record_to_row` Expected:
`test result: ok. 5 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/team_activity.rs
git commit -m "feat(phase-3a-4-team-activity-sidebar): TeamActivityRow + FetchResult + parser"
```

---

### Task 2: Implement per-repo `read_remote_url_cached` helper

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (append after
  `lookup_existing_record_id`, around line 600)

- [ ] **Step 1: Write the failing tests**

Append to the tests module (place these before
`// ── get/set_team_activity_config (Task 15) ────────────────────`):

```rust
    // ── Phase 3a-4: read_remote_url_cached ─────────────────────────

    #[test]
    fn read_remote_url_cached_returns_canonical_url_for_repo() {
        let tmp = tempfile::tempdir().unwrap();
        // Use the same git-init helper the enricher tests use.
        init_git_repo_with_origin(tmp.path(), "git@github.com:Foo/Bar.git");
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let url = read_remote_url_cached(&cache, "repo_a", tmp.path());
        assert!(
            url.contains("github.com"),
            "expected canonical github.com host, got {url:?}"
        );
        assert!(!url.ends_with(".git"), "canonicaliser strips .git, got {url:?}");
    }

    #[test]
    fn read_remote_url_cached_returns_empty_string_when_no_origin() {
        let tmp = tempfile::tempdir().unwrap();
        // git init without `remote add origin`.
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
        // After the first call, drop the repo directory; the second call
        // must still return the cached URL rather than re-shelling out.
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
        // Avoid shell-out churn on repos with no origin: cache the empty
        // string as a negative result so we don't re-run `git remote` every
        // 10 seconds for repos that don't have a remote.
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
        // After the first call, the cache must have the entry.
        assert_eq!(cache.lock().unwrap().get("repo_d"), Some(&"".to_string()));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib read_remote_url_cached` Expected: 4
failures with `cannot find function read_remote_url_cached in this scope`.

- [ ] **Step 3: Implement `read_remote_url_cached`**

Insert in the module body, after the `lookup_existing_record_id` function
(search for `async fn lookup_existing_record_id` and place the new function
right after the closing `}`):

```rust
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
pub(crate) fn read_remote_url_cached(
    cache: &Arc<std::sync::Mutex<HashMap<String, String>>>,
    repo_id: &str,
    repo_path: &std::path::Path,
) -> String {
    {
        let guard = cache.lock().expect("remote_url_cache poisoned");
        if let Some(cached) = guard.get(repo_id) {
            return cached.clone();
        }
    }
    let raw = crate::platform::repo_identity::read_origin_url(repo_path).unwrap_or_default();
    let canonical = if raw.is_empty() {
        String::new()
    } else {
        crate::platform::repo_identity::canonicalise_remote_url(&raw)
    };
    cache
        .lock()
        .expect("remote_url_cache poisoned")
        .insert(repo_id.to_string(), canonical.clone());
    canonical
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib read_remote_url_cached` Expected:
`test result: ok. 4 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/team_activity.rs
git commit -m "feat(phase-3a-4-team-activity-sidebar): per-repo canonical remote URL cache"
```

---

### Task 3: Implement `fetch_team_activity_rows_inner` (testable core)

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (append after
  `read_remote_url_cached`)

- [ ] **Step 1: Write the failing tests**

Append to the tests module:

```rust
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
        // No LarkClient needed when result is Disabled.
        let result = fetch_team_activity_rows_inner(state, None, &cache, None).await.unwrap();
        assert_eq!(result, FetchResult::Disabled);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_disabled_when_app_token_empty() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None).await.unwrap();
        assert_eq!(result, FetchResult::Disabled);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_machine_label_empty_when_label_blank() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn_x", "");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None).await.unwrap();
        assert_eq!(result, FetchResult::MachineLabelEmpty);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_no_overlap_when_state_has_no_repos() {
        let state: Arc<std::sync::Mutex<crate::state::AppState>> =
            Arc::new(std::sync::Mutex::new(crate::state::AppState::default()));
        let cache: Arc<std::sync::Mutex<HashMap<String, String>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let cfg = make_team_cfg("bascn_x", "alice@laptop");
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None).await.unwrap();
        assert_eq!(result, FetchResult::NoOverlapRepos);
    }

    #[tokio::test]
    async fn fetch_team_activity_rows_inner_returns_no_overlap_when_repos_have_no_origin() {
        // Repo exists in AppState but its path has no `git remote origin`,
        // so canonical URL is empty → filtered out → no overlap.
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
        let result = fetch_team_activity_rows_inner(state, Some(cfg), &cache, None).await.unwrap();
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
        let result =
            fetch_team_activity_rows_inner(state, Some(cfg), &cache, Some(client)).await;
        assert!(result.is_err(), "expected auth error, got {result:?}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib fetch_team_activity_rows_inner` Expected:
7 failures, each `cannot find function fetch_team_activity_rows_inner`.

- [ ] **Step 3: Implement `fetch_team_activity_rows_inner`**

Insert after `read_remote_url_cached` from Task 2:

```rust
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
    // Build the remote-URL list for the user's local repos. We snapshot
    // (id, path) tuples while holding the lock, then drop it before any
    // shell-out — `read_remote_url_cached` may spawn `git` which we don't
    // want to do under the AppState mutex.
    let repos: Vec<(String, std::path::PathBuf)> = {
        let s = state.lock().map_err(|e| crate::error::AppError::Other(e.to_string()))?;
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
            crate::state::FilterCondition {
                field_id: String::new(),
                field_name: "repo_remote_url".into(),
                operator: crate::state::FilterOperator::Is,
                value: remote_urls,
            },
        ],
    };
    let records = client
        .bitable_search_records(&cfg.app_token, &cfg.table_id, &filter)
        .await?;
    let rows = records.into_iter().map(parse_record_to_row).collect();
    Ok(FetchResult::Rows { rows })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib fetch_team_activity_rows_inner` Expected:
`test result: ok. 7 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/team_activity.rs
git commit -m "feat(phase-3a-4-team-activity-sidebar): fetch_team_activity_rows_inner FetchResult logic"
```

---

### Task 4: Wire `fetch_team_activity_rows` Tauri command + register handler

**Files:**

- Modify: `src-tauri/src/commands/team_activity.rs` (add Tauri command wrapper)
- Modify: `src-tauri/src/lib.rs` (register in invoke_handler! macro)

- [ ] **Step 1: Write the failing test**

Append to the tests module a registration-presence test that doesn't need a live
Tauri runtime:

```rust
    // ── Phase 3a-4: Tauri command wiring ───────────────────────────

    #[test]
    fn fetch_team_activity_rows_is_a_tauri_command() {
        // Compile-time check: the function exists and is callable as an
        // async function. We can't drive it end-to-end without a live
        // Tauri AppHandle, so we just confirm the signature is what
        // lib.rs's invoke_handler! macro expects.
        let _: fn(
            tauri::State<'_, Arc<std::sync::Mutex<crate::state::AppState>>>,
            tauri::AppHandle,
        ) -> _ = fetch_team_activity_rows;
    }
```

Also append a test that the command is registered in `lib.rs::tests`. Find the
existing test like `team_activity_config_commands_are_registered` (search in
`src-tauri/src/lib.rs` for the `#[cfg(test)] mod tests` block at the bottom):

```rust
    #[test]
    fn fetch_team_activity_rows_is_registered() {
        // Mirror the pattern other Phase 3a-3 commands use to assert
        // their registration with the Tauri invoke handler. We rely on
        // the linker pulling in the function symbol; if the macro
        // forgot to include it, this test would fail to compile.
        let _ = crate::commands::team_activity::fetch_team_activity_rows;
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
`cd src-tauri && cargo test --lib fetch_team_activity_rows_is_a_tauri_command fetch_team_activity_rows_is_registered`
Expected: 2 failures with
`cannot find function fetch_team_activity_rows in this scope`.

- [ ] **Step 3: Implement the Tauri command wrapper**

In `src-tauri/src/commands/team_activity.rs`, near the existing
`#[tauri::command]` blocks (around line 577 for `get_team_activity_config`),
add:

```rust
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
    let needs_client = matches!(&cfg, Some(c) if !c.app_token.is_empty() && !c.machine_label.is_empty());
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
```

In `src-tauri/src/lib.rs`, find the existing `tauri::generate_handler![...]`
macro (search for `set_team_activity_config` or `get_team_activity_config` —
they're already registered). Add `fetch_team_activity_rows` next to them:

```rust
// inside tauri::generate_handler![
//     ...
//     commands::team_activity::get_team_activity_config,
//     commands::team_activity::set_team_activity_config,
//     commands::team_activity::setup_team_activity_table,
+   commands::team_activity::fetch_team_activity_rows,
//     ...
// ]
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib fetch_team_activity_rows` Expected: All
`fetch_team_activity_rows_*` tests pass (inner ones from Task 3 + the two new
wiring tests from this task).

Then sanity check clippy + full test sweep:

Run: `cd src-tauri && cargo clippy --lib --all-targets -- -D warnings` Expected:
`Finished` with no warnings.

Run: `cd src-tauri && cargo test --lib` Expected: All tests pass (existing tests
untouched).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/team_activity.rs src-tauri/src/lib.rs
git commit -m "feat(phase-3a-4-team-activity-sidebar): fetch_team_activity_rows Tauri command"
```

---

### Task 5: Add TS types matching Rust shapes

**Files:**

- Modify: `src/lib/types.ts` (append to end of file)

- [ ] **Step 1: Add the types** (no test — pure type definitions, tested
      implicitly by consumers)

Append to `src/lib/types.ts`:

```typescript
// ── Phase 3a-4: Team Activity reader ─────────────────────────────

/** One row of the Phase 3a-4 sidebar / mirror view, mirroring the Rust
 *  `TeamActivityRow` shape and the columns the Phase 3a-3 publisher
 *  writes. All-string fields default to `''` when missing on the wire;
 *  `last_activity_at` is epoch ms (`0` when missing). */
export type TeamActivityRow = {
  workspace_id: string;
  repo_remote_url: string;
  repo_display_name: string;
  task_title: string;
  assignee_machine: string;
  ansambel_status: string;
  last_activity_at: number; // epoch ms
  last_message_preview: string;
  branch_name: string;
  diff_summary: string;
  pr_url: string;
  private: boolean;
};

/** Tagged-enum mirror of the Rust `FetchResult`. The `kind` discriminator
 *  matches the Rust `#[serde(tag = "kind", rename_all = "snake_case")]`
 *  shape. */
export type FetchResult =
  | { kind: 'disabled' }
  | { kind: 'machine_label_empty' }
  | { kind: 'no_overlap_repos' }
  | { kind: 'rows'; rows: TeamActivityRow[] };
```

- [ ] **Step 2: Type-check**

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): TS types for TeamActivityRow + FetchResult"
```

---

### Task 6: Add `api.teamActivity.fetchRows()` IPC wrapper

**Files:**

- Modify: `src/lib/ipc.ts` (extend the existing `teamActivity` namespace at
  line 279)
- Modify: `src/lib/ipc.test.ts` (add wrapper invocation test)

- [ ] **Step 1: Write the failing test**

Append to `src/lib/ipc.test.ts` (find the existing
`describe('api.teamActivity', ...)` block; if absent, find any `teamActivity`
test and add a new `it(...)` next to it):

```typescript
describe('api.teamActivity.fetchRows', () => {
  it('invokes fetch_team_activity_rows with no arguments', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    const result = await api.teamActivity.fetchRows();
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
    expect(result).toEqual({ kind: 'rows', rows: [] });
  });

  it('passes through the disabled FetchResult variant unchanged', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'disabled' });
    const result = await api.teamActivity.fetchRows();
    expect(result).toEqual({ kind: 'disabled' });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun run vitest run src/lib/ipc.test.ts -t "api.teamActivity.fetchRows"`
Expected: failure — `api.teamActivity.fetchRows is not a function`.

- [ ] **Step 3: Add the wrapper**

In `src/lib/ipc.ts`, find the existing `teamActivity:` namespace (around line
279). Add a `fetchRows` method:

```typescript
    /** Phase 3a-4: fetch rows the Team Activity sidebar + mirror view
     *  should display. Backend builds the FilterSpec from AppState +
     *  team_activity_config — frontend passes no arguments. Returns a
     *  tagged FetchResult: `disabled` / `machine_label_empty` /
     *  `no_overlap_repos` / `rows`. See
     *  `commands/team_activity.rs::fetch_team_activity_rows`. */
    fetchRows: (): Promise<FetchResult> => invoke<FetchResult>('fetch_team_activity_rows'),
```

Add `FetchResult` to the type import at the top of `ipc.ts`:

```typescript
// Find the existing `import type { ... } from './types';` and append FetchResult.
import type {
  // ... existing imports ...
  FetchResult,
  TeamActivityConfig,
  // ...
} from './types';
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun run vitest run src/lib/ipc.test.ts -t "api.teamActivity.fetchRows"`
Expected: 2 tests pass.

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/ipc.ts src/lib/ipc.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): api.teamActivity.fetchRows wrapper"
```

---

### Task 7: Create `team-activity.svelte.ts` store — core state + reconcile + select

**Files:**

- Create: `src/lib/stores/team-activity.svelte.ts`
- Create: `src/lib/stores/team-activity.svelte.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/stores/team-activity.svelte.test.ts`:

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { teamActivity } from './team-activity.svelte';
import { addToast, removeToast, getToasts } from './toasts.svelte';
import type { TeamActivityRow } from '../types';

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_x',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Task X',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: 1_700_000_000_000,
    last_message_preview: 'doing things',
    branch_name: 'feat/x',
    diff_summary: '',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

function clearToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

describe('TeamActivityStore — core state', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.error = null;
    teamActivity.selectedWorkspaceId = null;
    clearToasts();
  });

  it('sets status disabled on FetchResult disabled', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'disabled' });
    await teamActivity.refresh();
    expect(teamActivity.status).toBe('disabled');
    expect(teamActivity.rows.size).toBe(0);
  });

  it('sets status machine_label_empty on FetchResult machine_label_empty', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'machine_label_empty' });
    await teamActivity.refresh();
    expect(teamActivity.status).toBe('machine_label_empty');
  });

  it('sets status no_overlap_repos on FetchResult no_overlap_repos', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'no_overlap_repos' });
    await teamActivity.refresh();
    expect(teamActivity.status).toBe('no_overlap_repos');
  });

  it('reconciles new rows into the SvelteMap', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' }), row({ workspace_id: 'ws_b' })],
    });
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(2);
    expect(teamActivity.rows.has('ws_a')).toBe(true);
    expect(teamActivity.rows.has('ws_b')).toBe(true);
  });

  it('removes rows that were dropped from the server response', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' }), row({ workspace_id: 'ws_b' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(1);
    expect(teamActivity.rows.has('ws_b')).toBe(false);
  });

  it('updates existing rows in place when the server response changes them', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a', ansambel_status: 'running' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a', ansambel_status: 'waiting' })],
    });
    await teamActivity.refresh();
    expect(teamActivity.rows.get('ws_a')?.ansambel_status).toBe('waiting');
  });

  it('clears rows when a disabled response arrives after a rows response', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'disabled' });
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(0);
    expect(teamActivity.status).toBe('disabled');
  });

  it('keeps prior rows when a network error happens', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockRejectedValueOnce(new Error('offline'));
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(1);
    expect(teamActivity.status).toBe('error');
    expect(teamActivity.error).toContain('offline');
  });

  it('select sets selectedWorkspaceId', () => {
    teamActivity.select('ws_a');
    expect(teamActivity.selectedWorkspaceId).toBe('ws_a');
    teamActivity.select(null);
    expect(teamActivity.selectedWorkspaceId).toBe(null);
  });

  it('auto-clears selection and emits toast when the selected row is removed', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    teamActivity.select('ws_a');
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    await teamActivity.refresh();
    expect(teamActivity.selectedWorkspaceId).toBe(null);
    expect(getToasts().size).toBe(1);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `bun run vitest run src/lib/stores/team-activity.svelte.test.ts` Expected:
failures — `Cannot find module './team-activity.svelte'`.

- [ ] **Step 3: Create the store**

Create `src/lib/stores/team-activity.svelte.ts`:

```typescript
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import { addToast } from './toasts.svelte';
import type { FetchResult, TeamActivityRow } from '../types';

type Status =
  | 'idle'
  | 'loading'
  | 'error'
  | 'disabled'
  | 'machine_label_empty'
  | 'no_overlap_repos';

class TeamActivityStore {
  /** Active rows keyed by workspace_id. */
  readonly rows = new SvelteMap<string, TeamActivityRow>();

  /** UI state for the sidebar panel. See Phase 3a-4 spec's error
   *  handling matrix for the mapping from each value to user-visible
   *  copy. */
  status: Status = $state('idle');

  /** Last network / unexpected error message — surfaced as toast or
   *  banner copy. Cleared on the next successful fetch. */
  error: string | null = $state(null);

  /** When non-null, App.svelte routes to `TeamWorkspaceMirror` for this
   *  row. Cleared via `select(null)` from the back button OR
   *  automatically when reconcile detects the row vanished server-side. */
  selectedWorkspaceId: string | null = $state(null);

  /** Manual refresh trigger (Refresh button + visibility-change immediate
   *  fetch + first poll on mount). Also the unit-test entry point — the
   *  full `start()` / `stop()` lifecycle is tested separately in Task 8. */
  async refresh(): Promise<void> {
    let result: FetchResult;
    try {
      result = await api.teamActivity.fetchRows();
    } catch (err) {
      this.error = String(err);
      this.status = 'error';
      return;
    }
    this.error = null;
    switch (result.kind) {
      case 'disabled':
        this.status = 'disabled';
        this.reconcile([]);
        return;
      case 'machine_label_empty':
        this.status = 'machine_label_empty';
        this.reconcile([]);
        return;
      case 'no_overlap_repos':
        this.status = 'no_overlap_repos';
        this.reconcile([]);
        return;
      case 'rows':
        this.status = 'idle';
        this.reconcile(result.rows);
        return;
    }
  }

  /** Open or close the mirror view. Called by `TeamActivityPanel` row
   *  click and `TitleBar` back button. */
  select(workspaceId: string | null): void {
    this.selectedWorkspaceId = workspaceId;
  }

  private reconcile(rows: TeamActivityRow[]): void {
    const newIds = new Set<string>(rows.map((r) => r.workspace_id));
    // Insert / update rows present in the latest response.
    for (const r of rows) this.rows.set(r.workspace_id, r);
    // Remove rows that disappeared.
    for (const id of [...this.rows.keys()]) {
      if (!newIds.has(id)) this.rows.delete(id);
    }
    // Auto-close mirror view if its row vanished (teammate went private
    // or workspace closed).
    if (this.selectedWorkspaceId && !newIds.has(this.selectedWorkspaceId)) {
      this.selectedWorkspaceId = null;
      addToast('Team workspace closed by teammate', 'info', 4000);
    }
  }
}

export const teamActivity = new TeamActivityStore();
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun run vitest run src/lib/stores/team-activity.svelte.test.ts` Expected:
10 tests pass.

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/team-activity.svelte.ts src/lib/stores/team-activity.svelte.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): TeamActivityStore core state + reconcile + select"
```

---

### Task 8: Add poll loop, visibility-aware pause, and inflight guard to the store

**Files:**

- Modify: `src/lib/stores/team-activity.svelte.ts`
- Modify: `src/lib/stores/team-activity.svelte.test.ts`

- [ ] **Step 1: Write the failing tests**

Append to `src/lib/stores/team-activity.svelte.test.ts`:

```typescript
describe('TeamActivityStore — poll loop + visibility', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockReset();
    teamActivity.stop();
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.error = null;
    teamActivity.selectedWorkspaceId = null;
  });

  afterEach(() => {
    teamActivity.stop();
    vi.useRealTimers();
  });

  it('start triggers an immediate fetch (does not wait 10s)', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
  });

  it('polls every 10 seconds after the first fetch', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsAfterStart = vi.mocked(invoke).mock.calls.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart + 1);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart + 2);
  });

  it('skips the tick when document.visibilityState is hidden', async () => {
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'hidden',
    });
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    // start() fires immediately regardless of visibility — but the
    // *subsequent* interval tick should no-op.
    const callsAfterStart = vi.mocked(invoke).mock.calls.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart);
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'visible',
    });
  });

  it('fetches immediately when visibility flips to visible', async () => {
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'visible',
    });
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsBefore = vi.mocked(invoke).mock.calls.length;
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.runOnlyPendingTimersAsync();
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsBefore + 1);
  });

  it('skips overlapping ticks when a previous fetch is inflight', async () => {
    let resolveFirst: (v: FetchResult) => void = () => {};
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise<FetchResult>((res) => {
          resolveFirst = res;
        })
    );
    teamActivity.start();
    // First call kicked off but unresolved. Advance time past one
    // interval; refresh() guards against overlap so no new invoke fires.
    await vi.advanceTimersByTimeAsync(10_000);
    expect(invoke).toHaveBeenCalledTimes(1);
    resolveFirst({ kind: 'rows', rows: [] });
  });

  it('start is idempotent when called twice (no double interval)', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    // Both start() calls combined should only fire ONE immediate fetch.
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('stop clears the interval and removes the visibility listener', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsAfterStart = vi.mocked(invoke).mock.calls.length;
    teamActivity.stop();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart);
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.runOnlyPendingTimersAsync();
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart);
  });
});
```

Make sure the file imports `afterEach` from vitest at the top:

```typescript
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
`bun run vitest run src/lib/stores/team-activity.svelte.test.ts -t "poll loop"`
Expected: failures — `start is not a function` / `stop is not a function`.

- [ ] **Step 3: Add `start()`, `stop()`, `tick()` to the store**

Modify `src/lib/stores/team-activity.svelte.ts` — add the following inside the
`TeamActivityStore` class (between `select` and `reconcile`):

```typescript
  private intervalId: ReturnType<typeof setInterval> | null = null;
  private visibilityHandler: (() => void) | null = null;
  private inflight = false;

  /** Mount-time entry: immediate fetch + setInterval(10s) + a
   *  visibilitychange listener that fires an immediate fetch when the
   *  user returns to the app. Idempotent — calling `start()` twice in a
   *  row is a no-op (the existing interval keeps running). */
  start(): void {
    if (this.intervalId !== null) return; // already running
    void this.tick();
    this.intervalId = setInterval(() => void this.tick(), 10_000);
    this.visibilityHandler = () => {
      if (typeof document !== 'undefined' && document.visibilityState === 'visible') {
        void this.tick();
      }
    };
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', this.visibilityHandler);
    }
  }

  /** Unmount-time cleanup. Removes the visibility listener and clears
   *  the interval. Safe to call when already stopped. */
  stop(): void {
    if (this.intervalId !== null) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }
    if (this.visibilityHandler !== null && typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', this.visibilityHandler);
      this.visibilityHandler = null;
    }
    this.inflight = false;
  }

  /** Internal tick — visibility-aware, inflight-guarded. Public `refresh`
   *  delegates to the same fetch logic minus the visibility check (manual
   *  Refresh button works even when the document is hidden — defensive
   *  for headless tests). */
  private async tick(): Promise<void> {
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;
    if (this.inflight) return;
    this.inflight = true;
    try {
      await this.refresh();
    } finally {
      this.inflight = false;
    }
  }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun run vitest run src/lib/stores/team-activity.svelte.test.ts` Expected:
17 tests pass (10 from Task 7 + 7 new).

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/team-activity.svelte.ts src/lib/stores/team-activity.svelte.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): poll loop + visibility-aware pause"
```

---

### Task 9: `githubBranchUrl` helper

**Files:**

- Create: `src/lib/github-url.ts`
- Create: `src/lib/github-url.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/github-url.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { githubBranchUrl } from './github-url';

describe('githubBranchUrl', () => {
  it('builds https URL from an https remote', () => {
    expect(githubBranchUrl('https://github.com/foo/bar', 'main')).toBe(
      'https://github.com/foo/bar/tree/main'
    );
  });

  it('converts ssh-style git@ remote to https URL', () => {
    expect(githubBranchUrl('git@github.com:foo/bar', 'feat/x')).toBe(
      'https://github.com/foo/bar/tree/feat%2Fx'
    );
  });

  it('returns null when remote URL is empty', () => {
    expect(githubBranchUrl('', 'main')).toBeNull();
  });

  it('returns null when branch is empty', () => {
    expect(githubBranchUrl('https://github.com/foo/bar', '')).toBeNull();
  });

  it('returns null for unknown URL scheme', () => {
    expect(githubBranchUrl('ftp://example.com/foo', 'main')).toBeNull();
    expect(githubBranchUrl('totally-not-a-url', 'main')).toBeNull();
  });

  it('returns null when ssh-style URL lacks colon separator', () => {
    expect(githubBranchUrl('git@github.com/foo/bar', 'main')).toBeNull();
  });

  it('URL-encodes the branch name to handle slashes and special chars', () => {
    expect(
      githubBranchUrl('https://github.com/foo/bar', 'feat/auth & fix')
    ).toBe('https://github.com/foo/bar/tree/feat%2Fauth%20%26%20fix');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `bun run vitest run src/lib/github-url.test.ts` Expected: failures —
`Cannot find module './github-url'`.

- [ ] **Step 3: Implement the helper**

Create `src/lib/github-url.ts`:

```typescript
/** Build a GitHub-shaped "view branch" URL from the canonical
 *  `repo_remote_url` and a branch name. Returns `null` when either input
 *  is empty or the remote URL doesn't match either supported scheme.
 *
 *  Two canonical shapes the Phase 3a-3 publisher writes are accepted:
 *  - `https://github.com/owner/repo` (https clone URL, post-canonicalise
 *    strips `.git` and lowercases host)
 *  - `git@github.com:owner/repo` (ssh clone URL, post-canonicalise same)
 *
 *  Self-hosted (GitLab / Bitbucket / Forgejo) remotes that match one of
 *  these shapes will get a URL that may 404; the link is best-effort.
 *  Returning the URL anyway is preferable to a hidden link — the user
 *  can see what was attempted and the worst case is a 404. */
export function githubBranchUrl(
  remoteUrl: string,
  branch: string
): string | null {
  if (!remoteUrl || !branch) return null;
  let httpsBase: string;
  if (remoteUrl.startsWith('git@')) {
    const match = remoteUrl.match(/^git@([^:]+):(.+)$/);
    if (!match) return null;
    httpsBase = `https://${match[1]}/${match[2]}`;
  } else if (remoteUrl.startsWith('https://')) {
    httpsBase = remoteUrl;
  } else {
    return null;
  }
  return `${httpsBase}/tree/${encodeURIComponent(branch)}`;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun run vitest run src/lib/github-url.test.ts` Expected: 7 tests pass.

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/github-url.ts src/lib/github-url.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): githubBranchUrl helper"
```

---

### Task 10: `TeamActivityPanel.svelte` component

**Files:**

- Create: `src/lib/components/sidebar/TeamActivityPanel.svelte`
- Create: `src/lib/components/sidebar/TeamActivityPanel.svelte.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/components/sidebar/TeamActivityPanel.svelte.test.ts`:

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import TeamActivityPanel from './TeamActivityPanel.svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';
import type { TeamActivityRow } from '$lib/types';

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_a',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Fix login',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: Date.now() - 5 * 60 * 1000,
    last_message_preview: '',
    branch_name: 'feat/login',
    diff_summary: '',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

describe('TeamActivityPanel', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.error = null;
    teamActivity.selectedWorkspaceId = null;
    localStorage.removeItem('ansambel-team-activity-collapsed');
  });

  it('renders disabled hint when status is disabled', () => {
    teamActivity.status = 'disabled';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/Configure Team Activity in Settings/i)).toBeTruthy();
  });

  it('renders machine_label hint when status is machine_label_empty', () => {
    teamActivity.status = 'machine_label_empty';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/Set your machine label/i)).toBeTruthy();
  });

  it('renders add-repo hint when status is no_overlap_repos', () => {
    teamActivity.status = 'no_overlap_repos';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/Add a repo to see team activity/i)).toBeTruthy();
  });

  it('renders "No team activity" copy when idle with zero rows', () => {
    teamActivity.status = 'idle';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/No team activity in your repos/i)).toBeTruthy();
  });

  it('groups rows by repo_display_name alphabetically', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set(
      'ws_z',
      row({ workspace_id: 'ws_z', repo_display_name: 'zeta' })
    );
    teamActivity.rows.set(
      'ws_a',
      row({ workspace_id: 'ws_a', repo_display_name: 'alpha' })
    );
    const { container } = render(TeamActivityPanel);
    const groups = container.querySelectorAll(
      '[data-testid="team-activity-repo-group"]'
    );
    expect(groups.length).toBe(2);
    expect(groups[0].getAttribute('data-repo')).toBe('alpha');
    expect(groups[1].getAttribute('data-repo')).toBe('zeta');
  });

  it('renders a status dot whose color reflects ansambel_status', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_r', row({ ansambel_status: 'running' }));
    const { container } = render(TeamActivityPanel);
    const dot = container.querySelector(
      '[data-testid="team-activity-status-dot"]'
    );
    expect(dot?.getAttribute('data-status')).toBe('running');
  });

  it('renders relative last_activity_at time', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set(
      'ws_t',
      row({ last_activity_at: Date.now() - 5 * 60 * 1000 })
    );
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/5m ago|just now/i)).toBeTruthy();
  });

  it('click on a row calls teamActivity.select with the workspace_id', async () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_c', row({ workspace_id: 'ws_c' }));
    const { container } = render(TeamActivityPanel);
    const button = container.querySelector(
      '[data-testid="team-activity-row"][data-workspace-id="ws_c"]'
    ) as HTMLButtonElement;
    await fireEvent.click(button);
    expect(teamActivity.selectedWorkspaceId).toBe('ws_c');
  });

  it('Refresh button calls teamActivity.refresh', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    const { getByLabelText } = render(TeamActivityPanel);
    const button = getByLabelText(/refresh team activity/i);
    await fireEvent.click(button);
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
  });

  it('renders an error banner with a Retry button when status is error', async () => {
    teamActivity.status = 'error';
    teamActivity.error = 'offline';
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    const { getByText, getByRole } = render(TeamActivityPanel);
    expect(getByText(/offline/i)).toBeTruthy();
    const retry = getByRole('button', { name: /retry/i });
    await fireEvent.click(retry);
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
  });

  it('persists collapse state to localStorage', async () => {
    const { getByLabelText } = render(TeamActivityPanel);
    const toggle = getByLabelText(
      /collapse team activity|expand team activity/i
    );
    await fireEvent.click(toggle);
    expect(localStorage.getItem('ansambel-team-activity-collapsed')).toBe(
      'true'
    );
    await fireEvent.click(toggle);
    expect(localStorage.getItem('ansambel-team-activity-collapsed')).toBe(
      'false'
    );
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
`bun run vitest run src/lib/components/sidebar/TeamActivityPanel.svelte.test.ts`
Expected: failures — `Cannot find module './TeamActivityPanel.svelte'`.

- [ ] **Step 3: Implement the component**

Create `src/lib/components/sidebar/TeamActivityPanel.svelte`:

```svelte
<script context="module" lang="ts">
  /** Status colour palette mirrors the WorkspaceView dot for visual
   *  continuity. Module-scoped so the function is hoist-stable and
   *  testable in isolation (not strictly needed yet but cheap). */
  export function statusColor(status: string): string {
    switch (status) {
      case 'running':
        return '#22c55e'; // green
      case 'waiting':
        return '#eab308'; // yellow
      case 'blocked':
        return '#ef4444'; // red
      case 'pr_ready':
        return '#a855f7'; // purple
      case 'done':
        return '#6b7280'; // grey
      default:
        return '#9ca3af'; // grey-light
    }
  }
</script>

<script lang="ts">
  import { teamActivity } from '$lib/stores/team-activity.svelte';
  import type { TeamActivityRow } from '$lib/types';

  const COLLAPSE_STORAGE_KEY = 'ansambel-team-activity-collapsed';

  function readCollapsed(): boolean {
    if (typeof localStorage === 'undefined') return false;
    return localStorage.getItem(COLLAPSE_STORAGE_KEY) === 'true';
  }

  let collapsed = $state(readCollapsed());

  function toggleCollapsed(): void {
    collapsed = !collapsed;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(COLLAPSE_STORAGE_KEY, String(collapsed));
    }
  }

  /** Group rows by repo_display_name, alphabetical. Returns an array of
   *  `[repo, rows[]]` pairs so the template can render stable
   *  iteration order without a sorted SvelteMap. */
  function groupByRepo(
    rows: Iterable<TeamActivityRow>
  ): Array<[string, TeamActivityRow[]]> {
    const groups = new Map<string, TeamActivityRow[]>();
    for (const r of rows) {
      const key = r.repo_display_name || '—';
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(r);
    }
    return Array.from(groups.entries()).sort(([a], [b]) => a.localeCompare(b));
  }

  const groups = $derived(groupByRepo(teamActivity.rows.values()));
  const isEmpty = $derived(teamActivity.rows.size === 0);

  function relativeTime(epochMs: number): string {
    if (!epochMs) return '';
    const diffMs = Date.now() - epochMs;
    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 60) return 'just now';
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }
</script>

<section
  class="border-t border-[var(--border)] mt-2 pt-2 text-xs"
  aria-label="Team Activity"
  data-testid="team-activity-panel"
>
  <header class="flex items-center justify-between px-3 py-1">
    <button
      type="button"
      class="flex items-center gap-1 text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
      onclick={toggleCollapsed}
      aria-label={collapsed ? 'expand team activity' : 'collapse team activity'}
    >
      <span aria-hidden="true">{collapsed ? '▶' : '▼'}</span>
      <span class="uppercase tracking-wide font-semibold">Team Activity</span>
    </button>
    <button
      type="button"
      class="text-[var(--text-dim)] hover:text-[var(--text-primary)]"
      onclick={() => teamActivity.refresh()}
      aria-label="refresh team activity"
    >
      ↻
    </button>
  </header>

  {#if !collapsed}
    {#if teamActivity.status === 'disabled'}
      <p class="px-3 py-2 text-[var(--text-muted)]">
        Configure Team Activity in Settings →
      </p>
    {:else if teamActivity.status === 'machine_label_empty'}
      <p class="px-3 py-2 text-[var(--text-muted)]">
        Set your machine label in Settings →
      </p>
    {:else if teamActivity.status === 'no_overlap_repos'}
      <p class="px-3 py-2 text-[var(--text-muted)]">
        Add a repo to see team activity.
      </p>
    {:else if teamActivity.status === 'error'}
      <div class="mx-3 my-2 p-2 rounded border border-red-500/40 bg-red-500/10">
        <p class="text-red-300">{teamActivity.error}</p>
        <button
          type="button"
          class="mt-1 text-xs underline text-red-300 hover:text-red-200"
          onclick={() => teamActivity.refresh()}
        >
          Retry
        </button>
      </div>
    {:else if isEmpty}
      <p class="px-3 py-2 text-[var(--text-muted)]">
        No team activity in your repos right now.
      </p>
    {:else}
      <ul class="space-y-2">
        {#each groups as [repo, rows] (repo)}
          <li data-testid="team-activity-repo-group" data-repo={repo}>
            <p class="px-3 text-[var(--text-dim)] uppercase tracking-wide">
              {repo}
            </p>
            <ul>
              {#each rows as row (row.workspace_id)}
                <li>
                  <button
                    type="button"
                    class="flex items-center gap-2 w-full px-3 py-1 hover:bg-[var(--bg-hover)] text-left"
                    onclick={() => teamActivity.select(row.workspace_id)}
                    data-testid="team-activity-row"
                    data-workspace-id={row.workspace_id}
                  >
                    <span
                      class="inline-block w-2 h-2 rounded-full"
                      data-testid="team-activity-status-dot"
                      data-status={row.ansambel_status || 'idle'}
                      style:background-color={statusColor(row.ansambel_status)}
                    ></span>
                    <span class="flex-1 truncate">
                      <span class="text-[var(--text-primary)]"
                        >{row.assignee_machine}</span
                      >
                      <span class="text-[var(--text-muted)]">
                        · {row.task_title}</span
                      >
                    </span>
                    <span class="text-[var(--text-dim)] shrink-0">
                      {relativeTime(row.last_activity_at)}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
`bun run vitest run src/lib/components/sidebar/TeamActivityPanel.svelte.test.ts`
Expected: 11 tests pass.

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/sidebar/TeamActivityPanel.svelte src/lib/components/sidebar/TeamActivityPanel.svelte.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): TeamActivityPanel component"
```

---

### Task 11: `TeamWorkspaceMirror.svelte` component

**Files:**

- Create: `src/lib/components/team/TeamWorkspaceMirror.svelte`
- Create: `src/lib/components/team/TeamWorkspaceMirror.svelte.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/components/team/TeamWorkspaceMirror.svelte.test.ts`:

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));

import TeamWorkspaceMirror from './TeamWorkspaceMirror.svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';
import type { TeamActivityRow } from '$lib/types';

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_a',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Refactor auth',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: Date.now() - 5 * 60 * 1000,
    last_message_preview: 'Working on token validation',
    branch_name: 'feat/auth',
    diff_summary: '+10 -3',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

describe('TeamWorkspaceMirror', () => {
  beforeEach(() => {
    teamActivity.rows.clear();
    teamActivity.selectedWorkspaceId = null;
  });

  it('renders task_title, assignee_machine, status in the header', () => {
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText('Refactor auth')).toBeTruthy();
    expect(getByText(/bob@laptop/i)).toBeTruthy();
    expect(getByText(/running/i)).toBeTruthy();
  });

  it('constructs a GitHub branch URL from an https remote', () => {
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const link = getByRole('link', { name: /open branch on github/i });
    expect(link.getAttribute('href')).toBe(
      'https://github.com/foo/bar/tree/feat%2Fauth'
    );
  });

  it('constructs a GitHub branch URL from a git@ ssh remote', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ repo_remote_url: 'git@github.com:foo/bar', branch_name: 'feat/x' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const link = getByRole('link', { name: /open branch on github/i });
    expect(link.getAttribute('href')).toBe(
      'https://github.com/foo/bar/tree/feat%2Fx'
    );
  });

  it('hides the branch link when the remote scheme is unknown', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ repo_remote_url: 'ftp://example/foo', branch_name: 'main' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { queryByRole } = render(TeamWorkspaceMirror);
    expect(queryByRole('link', { name: /open branch on github/i })).toBeNull();
  });

  it('renders "Not yet published" placeholder when diff_summary is empty', () => {
    teamActivity.rows.set('ws_a', row({ diff_summary: '' }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText(/Not yet published/i)).toBeTruthy();
  });

  it('renders the diff_summary text when present', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ diff_summary: '+45 -12 across 3 files' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText(/\+45 -12 across 3 files/)).toBeTruthy();
  });

  it('renders the Open PR button only when status is pr_ready AND pr_url is set', () => {
    teamActivity.rows.set(
      'ws_a',
      row({
        ansambel_status: 'pr_ready',
        pr_url: 'https://github.com/foo/bar/pull/9',
      })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const button = getByRole('link', { name: /open pr/i });
    expect(button.getAttribute('href')).toBe(
      'https://github.com/foo/bar/pull/9'
    );
  });

  it('hides the Open PR button when pr_url is empty', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ ansambel_status: 'pr_ready', pr_url: '' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { queryByRole } = render(TeamWorkspaceMirror);
    expect(queryByRole('link', { name: /open pr/i })).toBeNull();
  });

  it('back button clears selectedWorkspaceId', async () => {
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByLabelText } = render(TeamWorkspaceMirror);
    const back = getByLabelText(/back to workspace/i);
    await fireEvent.click(back);
    expect(teamActivity.selectedWorkspaceId).toBeNull();
  });

  it('renders the sanitized message preview verbatim', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ last_message_preview: 'token: [REDACTED] — checking next step' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText(/\[REDACTED\] — checking next step/i)).toBeTruthy();
  });

  it('handles workspaces with no branch_name gracefully', () => {
    teamActivity.rows.set('ws_a', row({ branch_name: '' }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { queryByRole, queryByText } = render(TeamWorkspaceMirror);
    // No branch link rendered; no badge with branch name.
    expect(queryByRole('link', { name: /open branch on github/i })).toBeNull();
    expect(queryByText(/feat\//i)).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
`bun run vitest run src/lib/components/team/TeamWorkspaceMirror.svelte.test.ts`
Expected: failures — `Cannot find module './TeamWorkspaceMirror.svelte'`.

- [ ] **Step 3: Implement the component**

Create `src/lib/components/team/TeamWorkspaceMirror.svelte`:

```svelte
<script lang="ts">
  import { teamActivity } from '$lib/stores/team-activity.svelte';
  import { githubBranchUrl } from '$lib/github-url';

  const row = $derived.by(() => {
    const id = teamActivity.selectedWorkspaceId;
    if (!id) return null;
    return teamActivity.rows.get(id) ?? null;
  });

  const branchUrl = $derived(
    row ? githubBranchUrl(row.repo_remote_url, row.branch_name) : null
  );

  function relativeTime(epochMs: number): string {
    if (!epochMs) return '';
    const diffMs = Date.now() - epochMs;
    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 60) return 'just now';
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }
</script>

{#if row}
  <article class="h-full overflow-auto p-6" data-testid="team-workspace-mirror">
    <header
      class="flex items-start justify-between gap-4 border-b border-[var(--border)] pb-4"
    >
      <div class="min-w-0">
        <h1 class="text-xl font-semibold text-[var(--text-primary)] truncate">
          {row.task_title}
        </h1>
        <p class="mt-1 text-sm text-[var(--text-secondary)]">
          {row.assignee_machine}
          <span class="mx-2 text-[var(--text-dim)]">·</span>
          <span data-testid="team-mirror-status">{row.ansambel_status}</span>
          <span class="mx-2 text-[var(--text-dim)]">·</span>
          <span class="text-[var(--text-dim)]"
            >{relativeTime(row.last_activity_at)}</span
          >
        </p>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        <button
          type="button"
          class="text-sm text-[var(--text-dim)] hover:text-[var(--text-primary)]"
          onclick={() => teamActivity.refresh()}
          aria-label="refresh team workspace"
        >
          ↻
        </button>
        <button
          type="button"
          class="text-sm rounded border border-[var(--border)] px-2 py-1 hover:bg-[var(--bg-hover)]"
          onclick={() => teamActivity.select(null)}
          aria-label="back to workspace"
        >
          ← Back
        </button>
      </div>
    </header>

    <section class="mt-6">
      <h2 class="text-sm uppercase tracking-wide text-[var(--text-dim)]">
        Code state
      </h2>
      <div class="mt-2 flex items-center gap-3 text-sm">
        {#if row.branch_name}
          <span
            class="rounded bg-[var(--bg-card)] px-2 py-0.5 font-mono text-[var(--text-secondary)]"
            >{row.branch_name}</span
          >
        {/if}
        {#if branchUrl}
          <a
            href={branchUrl}
            target="_blank"
            rel="noopener noreferrer"
            class="text-blue-400 hover:underline"
          >
            Open branch on GitHub
          </a>
        {/if}
      </div>
      <p class="mt-2 text-sm text-[var(--text-secondary)]">
        {#if row.diff_summary}
          {row.diff_summary}
        {:else}
          <span class="italic text-[var(--text-dim)]">
            Not yet published — Ansambel doesn't yet emit diff summaries; see
            Phase 3a-5/6 followups in the design spec.
          </span>
        {/if}
      </p>
    </section>

    <section class="mt-6">
      <h2 class="text-sm uppercase tracking-wide text-[var(--text-dim)]">
        Latest activity
      </h2>
      <pre
        class="mt-2 whitespace-pre-wrap rounded bg-[var(--bg-card)] p-3 text-sm text-[var(--text-primary)]">{row.last_message_preview ||
          '—'}</pre>
      {#if row.ansambel_status === 'pr_ready' && row.pr_url}
        <a
          href={row.pr_url}
          target="_blank"
          rel="noopener noreferrer"
          class="mt-3 inline-block rounded bg-purple-600 px-3 py-1.5 text-sm text-white hover:bg-purple-500"
        >
          Open PR
        </a>
      {/if}
    </section>
  </article>
{/if}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
`bun run vitest run src/lib/components/team/TeamWorkspaceMirror.svelte.test.ts`
Expected: 11 tests pass.

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/team/TeamWorkspaceMirror.svelte src/lib/components/team/TeamWorkspaceMirror.svelte.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): TeamWorkspaceMirror component"
```

---

### Task 12: Mount `TeamActivityPanel` in `Sidebar.svelte` + drive `teamActivity.start()`/`stop()`

**Files:**

- Modify: `src/lib/components/Sidebar.svelte`
- Modify: `src/lib/components/Sidebar.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `src/lib/components/Sidebar.test.ts`:

```typescript
describe('Sidebar — Team Activity panel mount', () => {
  beforeEach(() => {
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.selectedWorkspaceId = null;
  });

  it('renders the TeamActivityPanel below WORKSPACES', () => {
    const { getByTestId } = render(Sidebar);
    expect(getByTestId('team-activity-panel')).toBeTruthy();
  });

  it('calls teamActivity.start() on mount', () => {
    const startSpy = vi.spyOn(teamActivity, 'start');
    render(Sidebar);
    expect(startSpy).toHaveBeenCalled();
    startSpy.mockRestore();
  });

  it('calls teamActivity.stop() on destroy', () => {
    const stopSpy = vi.spyOn(teamActivity, 'stop');
    const { unmount } = render(Sidebar);
    unmount();
    expect(stopSpy).toHaveBeenCalled();
    stopSpy.mockRestore();
  });
});
```

Also add the imports at top of `Sidebar.test.ts` if not already present:

```typescript
import { teamActivity } from '$lib/stores/team-activity.svelte';
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
`bun run vitest run src/lib/components/Sidebar.test.ts -t "Team Activity panel mount"`
Expected: failures — testid not found / spy not called.

- [ ] **Step 3: Wire the panel into `Sidebar.svelte`**

In `src/lib/components/Sidebar.svelte`, add to the `<script>` block at the top:

```typescript
import { onDestroy, onMount } from 'svelte';
import TeamActivityPanel from './sidebar/TeamActivityPanel.svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';

onMount(() => {
  teamActivity.start();
});

onDestroy(() => {
  teamActivity.stop();
});
```

(Note: if `onMount` / `onDestroy` are already imported in this file from earlier
work, don't duplicate.)

At the bottom of the existing sidebar template, after the closing `</ul>` of the
WORKSPACES section (search for `<!-- Inline new workspace form -->` and find the
structural closing tags after the workspaces list), mount the panel:

```svelte
<TeamActivityPanel />
```

Place this just before the outermost wrapping element's closing tag, so the
panel renders below the workspaces list.

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun run vitest run src/lib/components/Sidebar.test.ts` Expected: all
Sidebar tests pass (existing + 3 new).

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/Sidebar.svelte src/lib/components/Sidebar.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): mount TeamActivityPanel + lifecycle in Sidebar"
```

---

### Task 13: TitleBar "Watching:" label + back button

**Files:**

- Modify: `src/lib/components/TitleBar.svelte`
- Modify: `src/lib/components/TitleBar.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `src/lib/components/TitleBar.test.ts`:

```typescript
describe('TitleBar — Team Activity mirror mode', () => {
  beforeEach(() => {
    teamActivity.rows.clear();
    teamActivity.selectedWorkspaceId = null;
  });

  it('replaces the Plan/Work toggle with a Watching label when a mirror is selected', () => {
    teamActivity.rows.set('ws_a', {
      workspace_id: 'ws_a',
      repo_remote_url: '',
      repo_display_name: 'bar',
      task_title: 'Fix it',
      assignee_machine: 'bob@laptop',
      ansambel_status: 'running',
      last_activity_at: 0,
      last_message_preview: '',
      branch_name: '',
      diff_summary: '',
      pr_url: '',
      private: false,
    });
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText, queryByRole } = render(TitleBar, {
      props: { mode: 'work', onModeChange: vi.fn() },
    });
    expect(getByText(/Watching:/i)).toBeTruthy();
    expect(getByText(/bob@laptop/i)).toBeTruthy();
    expect(queryByRole('button', { name: /^Plan$/ })).toBeNull();
  });

  it('back button on the Watching label clears selectedWorkspaceId', async () => {
    teamActivity.rows.set('ws_a', {
      workspace_id: 'ws_a',
      repo_remote_url: '',
      repo_display_name: 'bar',
      task_title: 'Fix it',
      assignee_machine: 'bob@laptop',
      ansambel_status: 'running',
      last_activity_at: 0,
      last_message_preview: '',
      branch_name: '',
      diff_summary: '',
      pr_url: '',
      private: false,
    });
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByLabelText } = render(TitleBar, {
      props: { mode: 'work', onModeChange: vi.fn() },
    });
    const back = getByLabelText(/back to workspace/i);
    await fireEvent.click(back);
    expect(teamActivity.selectedWorkspaceId).toBeNull();
  });

  it('renders the normal Plan/Work toggle when no mirror is selected', () => {
    const { getByText } = render(TitleBar, {
      props: { mode: 'work', onModeChange: vi.fn() },
    });
    expect(getByText('Plan')).toBeTruthy();
    expect(getByText('Work')).toBeTruthy();
  });
});
```

Add to the test file's imports (if not present):

```typescript
import { fireEvent } from '@testing-library/svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
`bun run vitest run src/lib/components/TitleBar.test.ts -t "Team Activity mirror"`
Expected: failures — `Watching:` text not present.

- [ ] **Step 3: Wire the conditional render**

In `src/lib/components/TitleBar.svelte`, at the top of the `<script>` block,
add:

```typescript
import { teamActivity } from '$lib/stores/team-activity.svelte';

const watching = $derived.by(() => {
  const id = teamActivity.selectedWorkspaceId;
  if (!id) return null;
  return teamActivity.rows.get(id) ?? null;
});
```

Replace the existing `{#if mode !== undefined && onModeChange !== undefined}`
block (around line 106) with:

```svelte
{#if watching}
  <div class="flex items-center gap-2 text-sm">
    <button
      type="button"
      class="flex items-center justify-center w-7 h-7 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
      onclick={() => teamActivity.select(null)}
      aria-label="back to workspace"
    >
      ←
    </button>
    <span class="text-[var(--text-secondary)]">
      Watching:
      <span class="ml-1 text-[var(--text-primary)] font-medium"
        >{watching.assignee_machine}</span
      >
      <span class="ml-1 text-[var(--text-dim)]">@ {watching.task_title}</span>
    </span>
  </div>
{:else if mode !== undefined && onModeChange !== undefined}
  <!-- existing Plan/Work toggle block unchanged -->
  <div ...>
    <!-- existing Plan/Work buttons -->
  </div>
{/if}
```

When making the edit, preserve the existing inner Plan/Work button markup
verbatim — just wrap it in the `{:else if ...}` branch.

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun run vitest run src/lib/components/TitleBar.test.ts` Expected: all
TitleBar tests pass (existing + 3 new).

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/TitleBar.svelte src/lib/components/TitleBar.test.ts
git commit -m "feat(phase-3a-4-team-activity-sidebar): TitleBar Watching: label + back button"
```

---

### Task 14: App.svelte — route to `TeamWorkspaceMirror` when `selectedWorkspaceId` set

**Files:**

- Modify: `src/App.svelte`

- [ ] **Step 1: Verify existing tests are not broken**

Run: `bun run vitest run src/App.test.ts` (if this test file exists) and
`bun run check` after the edit. There's no dedicated app routing test — the E2E
test in Task 16 covers it.

- [ ] **Step 2: Wire the route**

In `src/App.svelte`, find the `<main>` block that has the existing
`{#if modeStore.mode === 'plan'} ... {:else if selectedWorkspace} ... {/if}`
(around line 181). At the top of the `<script>` block, add:

```typescript
import TeamWorkspaceMirror from '$lib/components/team/TeamWorkspaceMirror.svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';
```

Replace the existing conditional block:

```svelte
{#if modeStore.mode === 'plan'}
  {#if selectedRepo}
    <KanbanBoard ... />
  {:else}
    <div ...>Add a repo to start managing tasks.</div>
  {/if}
{:else if selectedWorkspace}
  <WorkspaceView workspace={selectedWorkspace} {highlightedFile} />
{:else}
  <div ...>Select or create a workspace</div>
{/if}
```

With:

```svelte
{#if teamActivity.selectedWorkspaceId}
  <TeamWorkspaceMirror />
{:else if modeStore.mode === 'plan'}
  {#if selectedRepo}
    <KanbanBoard ... />
  {:else}
    <div ...>Add a repo to start managing tasks.</div>
  {/if}
{:else if selectedWorkspace}
  <WorkspaceView workspace={selectedWorkspace} {highlightedFile} />
{:else}
  <div ...>Select or create a workspace</div>
{/if}
```

(Preserve the inner blocks of the existing conditional exactly — only add the
new `{#if teamActivity.selectedWorkspaceId}` branch at the top.)

- [ ] **Step 3: Run check**

Run: `bun run check` Expected: `0 ERRORS 0 WARNINGS`.

Run full vitest sweep:

Run: `bun run vitest run` Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/App.svelte
git commit -m "feat(phase-3a-4-team-activity-sidebar): App routes to TeamWorkspaceMirror when selectedWorkspaceId set"
```

---

### Task 15: Env-gated E2E test

**Files:**

- Create: `tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts`

- [ ] **Step 1: Write the E2E spec**

Create `tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { installTauriShim } from '../helpers/tauri-shim';
import type { FetchResult, TeamActivityRow } from '../../../src/lib/types';

const FIXTURE_GATE = process.env.ANSAMBEL_LARK_FIXTURE === '1';
test.skip(!FIXTURE_GATE, 'set ANSAMBEL_LARK_FIXTURE=1 to run');

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_remote_a',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Refactor auth',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: Date.now() - 5 * 60 * 1000,
    last_message_preview: 'doing the thing',
    branch_name: 'feat/auth',
    diff_summary: '',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

test.describe('Phase 3a-4: Team Activity sidebar + mirror view', () => {
  test('sidebar shows team rows for overlapping repos', async ({ page }) => {
    await installTauriShim(page, {
      fetch_team_activity_rows: (): FetchResult => ({
        kind: 'rows',
        rows: [row()],
      }),
    });
    await page.goto('/');
    await expect(page.getByTestId('team-activity-panel')).toBeVisible();
    await expect(
      page.locator(
        '[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]'
      )
    ).toBeVisible();
  });

  test('clicking a row opens the mirror view with GitHub branch link', async ({
    page,
  }) => {
    await installTauriShim(page, {
      fetch_team_activity_rows: (): FetchResult => ({
        kind: 'rows',
        rows: [row()],
      }),
    });
    await page.goto('/');
    await page
      .locator(
        '[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]'
      )
      .click();
    await expect(page.getByTestId('team-workspace-mirror')).toBeVisible();
    const link = page.getByRole('link', { name: /open branch on github/i });
    await expect(link).toHaveAttribute(
      'href',
      'https://github.com/foo/bar/tree/feat%2Fauth'
    );
  });

  test('back button returns to the prior main view', async ({ page }) => {
    await installTauriShim(page, {
      fetch_team_activity_rows: (): FetchResult => ({
        kind: 'rows',
        rows: [row()],
      }),
    });
    await page.goto('/');
    await page
      .locator(
        '[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]'
      )
      .click();
    await expect(page.getByTestId('team-workspace-mirror')).toBeVisible();
    await page.getByRole('button', { name: /back to workspace/i }).click();
    await expect(page.getByTestId('team-workspace-mirror')).not.toBeVisible();
  });

  test('mirror auto-closes when row disappears from polled response', async ({
    page,
  }) => {
    let serveRows = true;
    await installTauriShim(page, {
      fetch_team_activity_rows: (): FetchResult =>
        serveRows
          ? { kind: 'rows', rows: [row()] }
          : { kind: 'rows', rows: [] },
    });
    await page.goto('/');
    await page
      .locator(
        '[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]'
      )
      .click();
    await expect(page.getByTestId('team-workspace-mirror')).toBeVisible();
    serveRows = false;
    // Wait for next poll (10s) — visibility may make it earlier in test env.
    await page.waitForTimeout(11_000);
    await expect(page.getByTestId('team-workspace-mirror')).not.toBeVisible();
  });

  test('panel renders disabled state when config is absent', async ({
    page,
  }) => {
    await installTauriShim(page, {
      fetch_team_activity_rows: (): FetchResult => ({ kind: 'disabled' }),
    });
    await page.goto('/');
    await expect(
      page.getByText(/Configure Team Activity in Settings/i)
    ).toBeVisible();
  });
});
```

- [ ] **Step 2: Run the E2E suite under the fixture gate**

Run:
`ANSAMBEL_LARK_FIXTURE=1 bun run test:e2e -- tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts`
Expected: 5 tests pass.

If the project's E2E runner uses a different command, find it in `package.json`
(look for `test:e2e` script). The Phase 3a-3 publisher E2E
(`tests/e2e/phase-3a-3-publisher/publisher-roundtrip.spec.ts`) is the closest
precedent — match its runner command.

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts
git commit -m "test(phase-3a-4-team-activity-sidebar): env-gated E2E"
```

---

### Task 16: Coverage check + journal entry

**Files:**

- Create: `journal/2026-05-19-phase-3a-4-team-activity-sidebar.md`

- [ ] **Step 1: Run the coverage gate**

Run: `bun run vitest run --coverage` Expected: per-file thresholds met — 95%
lines/statements/functions, 93% branches on changed files. If any new file falls
below threshold, add a targeted test that exercises the missing line/branch. The
most common gap is a fallback branch in a `switch` or a fall-through `else` —
read the coverage report (`coverage/index.html`) and add the missing case.

Run: `cd src-tauri && cargo test --lib` Expected: all Rust tests pass.

Run: `cd src-tauri && cargo clippy --lib --all-targets -- -D warnings` Expected:
no warnings.

- [ ] **Step 2: Write the journal entry**

Create `journal/2026-05-19-phase-3a-4-team-activity-sidebar.md`:

```markdown
# Journal — 2026-05-19 — Phase 3a-4 team activity sidebar + watch view

## What shipped

Phase 3a-4 reads the dedicated `ansambel_team_activity` Bitable (the table Phase
3a-3 publishes to) and surfaces two UI surfaces:

- **Sidebar panel** below WORKSPACES — collapsible list of active team
  workspaces for the user's local repos, grouped by repo, with status dot +
  assignee + task title + relative last-activity time.
- **Mirror view** — replaces the main content when a row is clicked. Read-only:
  status / assignee / branch + GitHub branch link / last message preview / Open
  PR button when `ansambel_status == pr_ready` AND `pr_url` is set.

The backend exposes one new Tauri command `fetch_team_activity_rows` that builds
a `FilterSpec` internally from `AppState.repos` +
`team_activity_config.machine_label`, runs `git remote get-url origin` once per
repo (cached), calls `bitable_search_records`, and returns a tagged
`FetchResult` enum. Frontend polls every 10s with
`document.visibilityState`-aware pause.

## Backend

- `commands/team_activity.rs` (extended): added `TeamActivityRow` +
  `FetchResult` enum + `parse_record_to_row` parser + `read_remote_url_cached` +
  `fetch_team_activity_rows_inner` (pure- Rust testable core) +
  `fetch_team_activity_rows` Tauri command. The reader's canonical-URL cache is
  process-wide static (`OnceLock<Arc<Mutex<HashMap<RepoId, String>>>>`) and
  distinct from the publisher's enricher cache so read and write paths don't
  entangle.
- Filter construction inverts the original 3a-4 sketch: rather than the frontend
  building the FilterSpec, the backend does. The canonical `repo_remote_url` is
  computed by Rust (publisher enricher precedent) and stays out of the
  frontend's mental model. The IPC contract is argument-free:
  `api.teamActivity.fetchRows()`.

## Frontend

- `src/lib/types.ts`: added `TeamActivityRow` + `FetchResult` matching the Rust
  shapes via `serde(tag = "kind", rename_all = "snake_case")`.
- `src/lib/ipc.ts`: extended `teamActivity` namespace with `fetchRows()`
  wrapper.
- `src/lib/stores/team-activity.svelte.ts` (NEW): `TeamActivityStore` owns the
  polling loop. `start()` fires an immediate fetch + `setInterval(10_000)` +
  `visibilitychange` listener (immediate fetch on tab focus). `stop()` clears
  both. `inflight` guard prevents overlapping ticks. Reconcile auto-closes the
  mirror view when the selected row disappears server-side.
- `src/lib/github-url.ts` (NEW): pure helper that converts the publisher's
  canonical remote URL (`https://...` or `git@...`) into a `/tree/<branch>` URL,
  returning `null` for unsupported schemes so the link can be hidden instead of
  dead.
- `src/lib/components/sidebar/TeamActivityPanel.svelte` (NEW): collapsible
  section mounted in `Sidebar.svelte` below the workspaces list. Groups rows by
  repo, renders status dot + assignee + task title + relative time.
  Empty/disabled/error states cover every `FetchResult` variant. Collapse state
  persists to localStorage.
- `src/lib/components/team/TeamWorkspaceMirror.svelte` (NEW): the watch view.
  Header (task title + assignee + status + relative time + back button), Code
  state section (branch badge + GitHub link + diff summary or "Not yet
  published" placeholder), Latest activity section (sanitised message preview in
  `<pre>` + Open PR button when applicable).
- `src/lib/components/TitleBar.svelte` (extended): when
  `teamActivity.selectedWorkspaceId !== null`, replaces the Plan/Work toggle
  with "Watching: {assignee} @ {task}" + a back button.
- `src/App.svelte` (extended): routes to `TeamWorkspaceMirror` when
  `selectedWorkspaceId` is set, otherwise falls back to the existing Plan/Work
  conditional.
- `src/lib/components/Sidebar.svelte` (extended): mounts `TeamActivityPanel` and
  drives `teamActivity.start()` / `stop()` through `onMount` / `onDestroy`.

## Architectural decisions

- **Backend-side filter construction.** The canonical `repo_remote_url` cache
  lives in Rust. Surfacing it to the frontend just so JS could rebuild the
  filter would be either extra IPC round-trips or a serialised field that trails
  the cache. Backend builds the filter, frontend calls `fetchRows()` with no
  args.
- **`assignee_machine isNotEmpty` instead of `private isNot true`.** The
  publisher's privacy escape (3a-3) clears `assignee_machine` along with the
  other sensitive columns when a workspace goes private. An empty
  `assignee_machine` is therefore the canonical "should not appear in sidebar"
  signal, and the text-field `isNotEmpty` operator is unambiguously documented
  in Lark's filter API (whereas the boolean-checkbox filter semantics are not).
- **10s polling + visibility pause.** Lark Bitable rate limit is 200 req/min per
  app. The publisher alone can use up to 200 req/min in the worst case. 10s
  ticks at 6 req/min per engineer give the publisher headroom, and
  `document.visibilityState` pause cuts inactive engineers to zero.
- **Mirror view = main-content replacement, not modal.** Keeps consistent
  navigation (TitleBar shows context, back button returns to the prior mode) and
  gives the watch view space to breathe.
- **GitHub link as escape hatch for missing `diff_summary` / `pr_url`.** Both
  columns are deferred to Phase 3a-5/6 (no commit/push/PR-create handler exists
  yet), but the canonical remote + branch is enough to construct a
  `/tree/<branch>` URL. Self-hosted git remotes that don't match GitHub's shape
  get a hidden link rather than a dead one.

## Followups deferred

- `diff_summary` and `pr_url` column population — wired in 3a-3 but no emitter
  exists. Phase 3a-5/6 or 3a-8 is the natural place to add a
  commit/push/PR-create surface that emits these events.
- Notifications when teammate status flips to `blocked` or `pr_ready` — Phase
  3a-6 (Lark IM ping).
- Per-table membership UI — single-table assumption holds for now; Phase 3a-7 if
  a team needs multiple Bitable tables.
- Handoff bundles — Phase 3a-8.
- Provider abstraction for non-GitHub remotes (GitLab / Bitbucket / Forgejo).
  The `githubBranchUrl` helper is GitHub-shaped today; refactor when a
  non-GitHub user needs it.

## Tests

- **Rust**: +N unit tests covering parser, remote-URL cache, fetch_inner
  FetchResult variants, and Lark integration (wiremock).
- **TS**: +N vitest cases covering store FetchResult mapping, reconciliation,
  poll lifecycle, panel rendering states, mirror view branch URL construction +
  back button + auto-close.
- **E2E**: 5 env-gated cases (`ANSAMBEL_LARK_FIXTURE=1`) covering the full
  sidebar → click → mirror → back round-trip plus the
  auto-close-on-row-disappear path and the disabled-config render.

(Substitute N with the actual counts from `coverage` after the final sweep.)

## Aftermath

The publisher is now bi-directional: 3a-3 writes the row, 3a-4 reads it. The
"who is working on what" awareness loop closes for teams whose members all run
Ansambel against the same `team_activity_config.json`. Single-table assumption
holds; multi-table membership UI is Phase 3a-7.
```

Replace the `+N` placeholders with the actual counts from
`bun run vitest run --coverage` output.

- [ ] **Step 3: Commit**

```bash
git add journal/2026-05-19-phase-3a-4-team-activity-sidebar.md
git commit -m "docs(journal): 2026-05-19 — Phase 3a-4 team activity sidebar + watch view"
```

- [ ] **Step 4: Run the full pre-push gate locally before opening PR**

Run:
`bun run check && cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo test --lib && cd .. && bun run vitest run`
Expected: all green.

---

## Self-review

**Spec coverage** — every spec section has a task:

| Spec section                                      | Task(s)                                                                   |
| ------------------------------------------------- | ------------------------------------------------------------------------- |
| Architecture diagram                              | 1 (types), 3-4 (Rust core + command), 7-8 (store), 10-11 (panels)         |
| Configuration (`team_activity_config.json` reuse) | 3 (fetch_inner reads it), 4 (loader path)                                 |
| Filter construction (backend-side)                | 3 (fetch_inner)                                                           |
| FetchResult enum + status mapping                 | 1, 5 (types), 7 (store status mapping)                                    |
| Polling lifecycle                                 | 8                                                                         |
| Reconciliation                                    | 7                                                                         |
| Backend command shape                             | 3, 4                                                                      |
| Frontend store API                                | 7, 8                                                                      |
| TeamActivityPanel                                 | 10                                                                        |
| TeamWorkspaceMirror                               | 11                                                                        |
| TitleBar Watching label                           | 13                                                                        |
| App.svelte routing                                | 14                                                                        |
| GitHub URL construction                           | 9                                                                         |
| Status colour palette                             | 10 (statusColor function)                                                 |
| Error handling matrix                             | 7 (store status mapping), 10 (panel renders)                              |
| Lifecycle hooks                                   | 8 (store), 12 (sidebar mount/destroy)                                     |
| Test plan                                         | every implementation task has its TDD red/green cycle; Task 15 covers E2E |

**No placeholder steps** — every step has actual code or an exact command.

**Type consistency** — `TeamActivityRow` defined in Task 1 (Rust) and Task 5
(TS) use the same field names; `FetchResult` `kind` discriminator matches Rust's
`#[serde(tag = "kind", rename_all = "snake_case")]`. `fetch_team_activity_rows`
signature matches the IPC wrapper in Task 6.

---
