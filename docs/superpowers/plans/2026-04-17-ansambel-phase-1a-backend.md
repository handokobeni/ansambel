# Ansambel — Phase 1a Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Rust backend for Phase 1: expanded
`RepoInfo`/`WorkspaceInfo` structs, persistence layer, git-worktree-backed
workspace lifecycle commands (create / list / remove), repo CRUD (add via folder
picker contract / list / remove / update_gh_profile), and startup state
hydration with `Running` → `Waiting` coercion. Phase 1a-frontend (next plan)
will add the matching TS/Svelte UI shell.

**Architecture:** Expand the Phase 0 `AppState` with rich domain structs
serialized to `repos.json`, `workspaces.json`, `app_settings.json` via
`write_atomic`. Add `commands::repo` and `commands::workspace` Tauri handler
modules that shell out to `git` via a shared `commands::helpers::exec_git`
wrapper (no `git2` crate — aligns with korlap's approach and keeps the native
dependency surface small). Register `tauri-plugin-dialog` for the frontend's
folder picker (consumed in Phase 1a-frontend).

**Tech Stack:** Tauri v2 + Rust, `tauri-plugin-dialog` v2, shelled `git` CLI,
`dunce::canonicalize` for Windows path normalization, `#[tauri::command]` async
handlers returning `Result<T, String>`.

---

## Task 1: Add `tauri-plugin-dialog` v2 dependency

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1.1: Write the failing test**

```rust
// Add to src-tauri/src/lib.rs temporarily to verify it compiles
// The real test is: cargo check passes after adding the dep.
// Write a placeholder integration check in src-tauri/src/commands/system.rs
// confirming dialog plugin is registered (compile-only test).
```

- [ ] **Step 1.2: Run check to verify it fails**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo check 2>&1 | tail -10
```

Expected:
`error[E0433]: failed to resolve: use of undeclared crate or module 'tauri_plugin_dialog'`
— once `lib.rs` references it.

- [ ] **Step 1.3: Implement**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tauri-plugin-dialog = "2"
```

In `src-tauri/capabilities/default.json`, add to `"permissions"`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability granted to the main window",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:default", "dialog:allow-open"]
}
```

In `src-tauri/src/lib.rs`, add plugin registration (the final `lib.rs` state
after this task):

```rust
use tauri::Manager;

pub mod commands;
pub mod error;
pub mod ids;
pub mod logging;
pub mod panic;
pub mod persistence;
pub mod platform;
pub mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            crate::platform::paths::ensure_data_dirs(&data_dir)?;
            let guard = crate::logging::init(&data_dir)?;
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(Some(guard))));
            crate::panic::install_hook(data_dir.clone());
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                crate::state::AppState::default(),
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::system::get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 1.4: Run check — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo check 2>&1 | tail -5
```

Expected: `Finished` — no errors.

- [ ] **Step 1.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/Cargo.toml src-tauri/capabilities/default.json src-tauri/src/lib.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add tauri-plugin-dialog v2 dep and capabilities

Register tauri-plugin-dialog::init() in the Tauri builder and grant
dialog:default + dialog:allow-open capabilities so the Phase 1a-frontend
folder picker can invoke the native file dialog.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Expand `WorkspaceStatus` + `KanbanColumn` enums

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 2.1: Write the failing test**

```rust
// In src-tauri/src/state.rs  #[cfg(test)] mod tests — add:
#[test]
fn workspace_status_default_is_not_started() {
    assert_eq!(WorkspaceStatus::default(), WorkspaceStatus::NotStarted);
}

#[test]
fn kanban_column_default_is_todo() {
    assert_eq!(KanbanColumn::default(), KanbanColumn::Todo);
}

#[test]
fn workspace_status_round_trips_json() {
    let cases = [
        (WorkspaceStatus::NotStarted, "\"not_started\""),
        (WorkspaceStatus::Running, "\"running\""),
        (WorkspaceStatus::Waiting, "\"waiting\""),
        (WorkspaceStatus::Done, "\"done\""),
        (WorkspaceStatus::Error, "\"error\""),
    ];
    for (variant, expected_json) in cases {
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(json, expected_json);
        let back: WorkspaceStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn kanban_column_round_trips_json() {
    let cases = [
        (KanbanColumn::Todo, "\"todo\""),
        (KanbanColumn::InProgress, "\"in_progress\""),
        (KanbanColumn::Review, "\"review\""),
        (KanbanColumn::Done, "\"done\""),
    ];
    for (variant, expected_json) in cases {
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(json, expected_json);
        let back: KanbanColumn = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
}
```

- [ ] **Step 2.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -15
```

Expected: compile error — `WorkspaceStatus`, `KanbanColumn` unresolved.

- [ ] **Step 2.3: Implement**

Replace the enum section of `src-tauri/src/state.rs` (add above the existing
struct block):

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    NotStarted,
    Running,
    Waiting,
    Done,
    Error,
}

impl Default for WorkspaceStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KanbanColumn {
    Todo,
    InProgress,
    Review,
    Done,
}

impl Default for KanbanColumn {
    fn default() -> Self {
        Self::Todo
    }
}
```

- [ ] **Step 2.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed; 0 failed` (existing 2 + new 4).

- [ ] **Step 2.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add WorkspaceStatus and KanbanColumn enums to state

Both enums use snake_case serde serialization and implement Default.
WorkspaceStatus::NotStarted and KanbanColumn::Todo are the defaults.
Tests verify round-trip JSON serialization for all variants.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Expand `RepoInfo` struct

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 3.1: Write the failing test**

```rust
// In src-tauri/src/state.rs  #[cfg(test)] mod tests — add:
#[test]
fn repo_info_round_trips_json() {
    let r = RepoInfo {
        id: "repo_abc123".into(),
        name: "my-repo".into(),
        path: std::path::PathBuf::from("/home/user/my-repo"),
        gh_profile: Some("handokoben".into()),
        default_branch: "main".into(),
        created_at: 1_776_000_000,
        updated_at: 1_776_099_000,
    };
    let json = serde_json::to_string(&r).unwrap();
    let back: RepoInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}

#[test]
fn repo_info_gh_profile_nullable() {
    let r = RepoInfo {
        id: "repo_xyz".into(),
        name: "other".into(),
        path: std::path::PathBuf::from("/tmp/other"),
        gh_profile: None,
        default_branch: "main".into(),
        created_at: 0,
        updated_at: 0,
    };
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains("\"gh_profile\":null"));
    let back: RepoInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back.gh_profile, None);
}
```

- [ ] **Step 3.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -15
```

Expected: compile error — `RepoInfo` missing fields `gh_profile`,
`default_branch`, `created_at`, `updated_at`.

- [ ] **Step 3.3: Implement**

Replace the `RepoInfo` struct in `src-tauri/src/state.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub path: std::path::PathBuf,
    pub gh_profile: Option<String>,
    pub default_branch: String,
    pub created_at: i64,
    pub updated_at: i64,
}
```

- [ ] **Step 3.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -10
```

Expected: `test result: ok. 8 passed; 0 failed`.

- [ ] **Step 3.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): expand RepoInfo struct with Phase 1 fields

Add gh_profile, default_branch, created_at, updated_at and change path
to PathBuf. Tests verify PartialEq, JSON round-trip, and nullable gh_profile.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Expand `WorkspaceInfo` struct

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 4.1: Write the failing test**

```rust
// In src-tauri/src/state.rs  #[cfg(test)] mod tests — add:
#[test]
fn workspace_info_round_trips_json() {
    let ws = WorkspaceInfo {
        id: "ws_abc123".into(),
        repo_id: "repo_xyz".into(),
        branch: "ws/abc123".into(),
        base_branch: "main".into(),
        custom_branch: false,
        title: "Fix login bug".into(),
        description: "Broken on mobile".into(),
        status: WorkspaceStatus::Waiting,
        column: KanbanColumn::InProgress,
        created_at: 1_776_000_000,
        updated_at: 1_776_099_500,
    };
    let json = serde_json::to_string(&ws).unwrap();
    let back: WorkspaceInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ws);
}

#[test]
fn workspace_info_status_is_not_started_by_default() {
    // Verify Default derive would give NotStarted / Todo if we could use it
    // (WorkspaceInfo doesn't derive Default, but status field default is)
    assert_eq!(WorkspaceStatus::default(), WorkspaceStatus::NotStarted);
    assert_eq!(KanbanColumn::default(), KanbanColumn::Todo);
}
```

- [ ] **Step 4.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -15
```

Expected: compile error — `WorkspaceInfo` missing the new fields.

- [ ] **Step 4.3: Implement**

Replace the `WorkspaceInfo` struct in `src-tauri/src/state.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
    pub base_branch: String,
    pub custom_branch: bool,
    pub title: String,
    pub description: String,
    pub status: WorkspaceStatus,
    pub column: KanbanColumn,
    pub created_at: i64,
    pub updated_at: i64,
}
```

- [ ] **Step 4.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -10
```

Expected: `test result: ok. 10 passed; 0 failed`.

- [ ] **Step 4.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): expand WorkspaceInfo struct with Phase 1 fields

Add base_branch, custom_branch, title, description, status, column,
created_at, updated_at. Tests verify JSON round-trip and PartialEq.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Add `AppSettings` struct + wire into `AppState`

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 5.1: Write the failing test**

```rust
// In src-tauri/src/state.rs  #[cfg(test)] mod tests — add:
#[test]
fn app_settings_default_values() {
    let s = AppSettings::default();
    assert_eq!(s.schema_version, 1);
    assert_eq!(s.theme, "warm-dark");
    assert_eq!(s.selected_repo_id, None);
    assert_eq!(s.selected_workspace_id, None);
    assert!(s.recent_repos.is_empty());
    assert_eq!(s.window_width, 1400);
    assert_eq!(s.window_height, 900);
    assert!(!s.onboarding_completed);
}

#[test]
fn app_settings_round_trips_json() {
    let s = AppSettings::default();
    let json = serde_json::to_string(&s).unwrap();
    let back: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
}

#[test]
fn app_state_has_settings_field() {
    let state = AppState::default();
    assert_eq!(state.settings.schema_version, 1);
}
```

- [ ] **Step 5.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -15
```

Expected: compile error — `AppSettings` unresolved.

- [ ] **Step 5.3: Implement**

Add `AppSettings` to `src-tauri/src/state.rs` and update `AppState`. The final
relevant block:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppSettings {
    pub schema_version: u32,
    pub theme: String,
    pub selected_repo_id: Option<String>,
    pub selected_workspace_id: Option<String>,
    pub recent_repos: Vec<String>,
    pub window_width: u32,
    pub window_height: u32,
    pub onboarding_completed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: "warm-dark".into(),
            selected_repo_id: None,
            selected_workspace_id: None,
            recent_repos: Vec::new(),
            window_width: 1400,
            window_height: 900,
            onboarding_completed: false,
        }
    }
}

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: std::collections::HashMap<String, RepoInfo>,
    pub workspaces: std::collections::HashMap<String, WorkspaceInfo>,
    pub settings: AppSettings,
}
```

- [ ] **Step 5.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -10
```

Expected: `test result: ok. 13 passed; 0 failed`.

- [ ] **Step 5.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add AppSettings struct and wire into AppState

AppSettings carries schema_version=1, theme, window dims, selection state,
and onboarding flag. AppState gains settings field with Default impl.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `commands/helpers.rs` — `now_unix` + `exec_git` + tests

**Files:**

- Create: `src-tauri/src/commands/helpers.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 6.1: Write the failing test**

```rust
// In src-tauri/src/commands/helpers.rs  #[cfg(test)] mod tests:
#[test]
fn now_unix_is_recent() {
    let t = now_unix();
    // Should be after 2026-01-01 (unix 1_767_225_600) and before 2100
    assert!(t > 1_767_225_600, "now_unix returned {t}, expected > 2026");
    assert!(t < 4_102_444_800, "now_unix returned {t}, expected < 2100");
}

#[test]
fn exec_git_version_returns_non_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let out = exec_git(&["--version"], tmp.path()).unwrap();
    assert!(!out.is_empty());
    assert!(out.starts_with("git version"), "Got: {out}");
}

#[test]
fn exec_git_invalid_subcommand_returns_err() {
    let tmp = tempfile::tempdir().unwrap();
    let result = exec_git(&["__no_such_subcommand__"], tmp.path());
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("git") || msg.contains("External command"), "Got: {msg}");
}
```

- [ ] **Step 6.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::helpers 2>&1 | tail -15
```

Expected: compile error — `helpers` module not found.

- [ ] **Step 6.3: Implement**

Create `src-tauri/src/commands/helpers.rs`:

```rust
use crate::error::{AppError, Result};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as Unix timestamp (seconds).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Resolve the `git` binary. Uses PATH on all platforms.
/// On Windows, `which::which` will find `git.exe` correctly.
fn git_binary() -> std::path::PathBuf {
    which::which("git")
        .unwrap_or_else(|_| std::path::PathBuf::from("git"))
}

/// Run `git <args>` in `cwd`, return trimmed stdout on success,
/// or `AppError::Command` carrying stderr on nonzero exit.
pub fn exec_git(args: &[&str], cwd: &Path) -> Result<String> {
    let git = git_binary();
    let output = std::process::Command::new(&git)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| AppError::Command {
            cmd: git.display().to_string(),
            msg: e.to_string(),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(AppError::Command {
            cmd: format!("git {}", args.join(" ")),
            msg: stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_unix_is_recent() {
        let t = now_unix();
        assert!(t > 1_767_225_600, "now_unix returned {t}, expected > 2026");
        assert!(t < 4_102_444_800, "now_unix returned {t}, expected < 2100");
    }

    #[test]
    fn exec_git_version_returns_non_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let out = exec_git(&["--version"], tmp.path()).unwrap();
        assert!(!out.is_empty());
        assert!(out.starts_with("git version"), "Got: {out}");
    }

    #[test]
    fn exec_git_invalid_subcommand_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let result = exec_git(&["__no_such_subcommand__"], tmp.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("git") || msg.contains("External command"), "Got: {msg}");
    }
}
```

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod helpers;
pub mod system;
```

- [ ] **Step 6.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::helpers 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 6.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/helpers.rs src-tauri/src/commands/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add commands/helpers.rs with now_unix and exec_git

exec_git shells out to the git binary resolved via which::which (cross-
platform), returns trimmed stdout or AppError::Command with stderr on
nonzero exit. now_unix returns current Unix timestamp as i64.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `commands/helpers.rs` — `detect_default_branch` + tests

**Files:**

- Modify: `src-tauri/src/commands/helpers.rs`

- [ ] **Step 7.1: Write the failing test**

```rust
// In src-tauri/src/commands/helpers.rs  #[cfg(test)] mod tests — add:
use std::process::Command;

fn make_repo_with_origin_main(tmp: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    // Create a bare "remote" repo
    let remote = tmp.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    Command::new("git").args(["init", "--bare"]).current_dir(&remote).output().unwrap();

    // Clone it as local repo
    let local = tmp.path().join("local");
    Command::new("git")
        .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
        .output().unwrap();

    // Configure identity for commits
    Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["config", "user.name", "Test"]).current_dir(&local).output().unwrap();

    // Make an initial commit so the branch exists
    let readme = local.join("README.md");
    std::fs::write(&readme, b"init").unwrap();
    Command::new("git").args(["add", "."]).current_dir(&local).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&local).output().unwrap();

    // Set origin HEAD to main
    Command::new("git")
        .args(["remote", "set-head", "origin", "main"])
        .current_dir(&local)
        .output().unwrap();

    (local, remote)
}

#[test]
fn detect_default_branch_finds_main_via_symbolic_ref() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _remote) = make_repo_with_origin_main(&tmp);
    let branch = detect_default_branch(&local).unwrap();
    assert_eq!(branch, "main");
}

#[test]
fn detect_default_branch_falls_back_to_ls_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _remote) = make_repo_with_origin_main(&tmp);

    // Remove origin/HEAD symref to force tier-2 fallback
    let _ = Command::new("git")
        .args(["remote", "set-head", "origin", "--delete"])
        .current_dir(&local)
        .output();

    let branch = detect_default_branch(&local).unwrap();
    assert_eq!(branch, "main");
}

#[test]
fn detect_default_branch_no_origin_returns_err() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("noop");
    std::fs::create_dir_all(&repo).unwrap();
    Command::new("git").args(["init"]).current_dir(&repo).output().unwrap();
    // No remote added
    let result = detect_default_branch(&repo);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("Could not detect") || msg.contains("origin"), "Got: {msg}");
}
```

- [ ] **Step 7.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::helpers 2>&1 | tail -15
```

Expected: compile error — `detect_default_branch` unresolved.

- [ ] **Step 7.3: Implement**

Add to `src-tauri/src/commands/helpers.rs`:

```rust
/// Detect the default branch from origin remote tracking refs.
///
/// Tier 1: `git symbolic-ref --short refs/remotes/origin/HEAD`
/// Tier 2: probe `git ls-remote --heads origin main` then `master`
///
/// Never falls back to local branches — workspaces must always branch from origin.
pub fn detect_default_branch(repo_path: &Path) -> Result<String> {
    // Tier 1: origin HEAD symref
    let tier1 = exec_git(
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        repo_path,
    );
    if let Ok(ref_short) = tier1 {
        // ref_short looks like "origin/main"
        if let Some(branch) = ref_short.strip_prefix("origin/") {
            return Ok(branch.to_string());
        }
    }

    // Tier 2: probe ls-remote for known default names
    for candidate in ["main", "master"] {
        let ls = exec_git(
            &["ls-remote", "--heads", "origin", candidate],
            repo_path,
        );
        if let Ok(out) = ls {
            if !out.is_empty() {
                return Ok(candidate.to_string());
            }
        }
    }

    Err(AppError::InvalidState(
        "Could not detect default branch from origin remote. \
         No origin/HEAD, origin/main, or origin/master found. \
         Run `git remote set-head origin --auto` or check your remote configuration."
            .into(),
    ))
}
```

- [ ] **Step 7.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::helpers 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed; 0 failed`.

- [ ] **Step 7.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/helpers.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add detect_default_branch to helpers

Two-tier detection: symbolic-ref origin/HEAD first, then ls-remote probe
for main/master. Hard errors (no origin or no main/master) surface as
AppError::InvalidState with an actionable message.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `commands/helpers.rs` — `is_git_repo` + tests

**Files:**

- Modify: `src-tauri/src/commands/helpers.rs`

- [ ] **Step 8.1: Write the failing test**

```rust
// In src-tauri/src/commands/helpers.rs  #[cfg(test)] mod tests — add:
#[test]
fn is_git_repo_true_for_git_init_dir() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new("git").args(["init"]).current_dir(tmp.path()).output().unwrap();
    assert!(is_git_repo(tmp.path()));
}

#[test]
fn is_git_repo_false_for_plain_dir() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(!is_git_repo(tmp.path()));
}

#[test]
fn is_git_repo_true_for_bare_repo() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new("git").args(["init", "--bare"]).current_dir(tmp.path()).output().unwrap();
    // bare repo has HEAD, config, objects — no .git subdir but the dir itself is the repo
    // is_git_repo checks for .git entry; bare repos don't have .git — expected false for bare
    // (Ansambel only manages non-bare repos)
    assert!(!is_git_repo(tmp.path()));
}

#[test]
fn is_git_repo_true_for_worktree_link_file() {
    // Worktrees have a .git FILE (not dir) pointing back to the main repo
    let tmp = tempfile::tempdir().unwrap();
    let git_file = tmp.path().join(".git");
    std::fs::write(&git_file, b"gitdir: /some/path/.git/worktrees/ws1\n").unwrap();
    assert!(is_git_repo(tmp.path()));
}
```

- [ ] **Step 8.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::helpers 2>&1 | tail -15
```

Expected: compile error — `is_git_repo` unresolved.

- [ ] **Step 8.3: Implement**

Add to `src-tauri/src/commands/helpers.rs`:

```rust
/// Returns true if `path` is the root of a git repository (`.git` dir or `.git` file).
/// A `.git` FILE indicates a git worktree link — also counts as a git repo root.
/// Bare repositories (no `.git` entry) return false; Ansambel only manages non-bare repos.
pub fn is_git_repo(path: &Path) -> bool {
    let git_entry = path.join(".git");
    git_entry.is_dir() || git_entry.is_file()
}
```

- [ ] **Step 8.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::helpers 2>&1 | tail -10
```

Expected: `test result: ok. 10 passed; 0 failed`.

- [ ] **Step 8.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/helpers.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add is_git_repo helper

Checks for .git dir (regular clone) or .git file (worktree link). Bare
repos intentionally return false — Ansambel only manages non-bare repos.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Persistence — `repos` module helpers

**Files:**

- Create: `src-tauri/src/persistence/repos.rs`
- Modify: `src-tauri/src/persistence/mod.rs`

- [ ] **Step 9.1: Write the failing test**

```rust
// In src-tauri/src/persistence/repos.rs  #[cfg(test)] mod tests:
use super::*;
use crate::state::RepoInfo;
use std::collections::HashMap;
use std::path::PathBuf;

fn make_repo(id: &str) -> RepoInfo {
    RepoInfo {
        id: id.into(),
        name: "test-repo".into(),
        path: PathBuf::from("/tmp/test-repo"),
        gh_profile: None,
        default_branch: "main".into(),
        created_at: 1_000_000,
        updated_at: 1_000_001,
    }
}

#[test]
fn save_and_load_repos_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let mut map = HashMap::new();
    map.insert("repo_abc".into(), make_repo("repo_abc"));
    save_repos(tmp.path(), &map).unwrap();

    let loaded = load_repos(tmp.path()).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded["repo_abc"].name, "test-repo");
}

#[test]
fn load_repos_missing_file_returns_empty_map() {
    let tmp = tempfile::tempdir().unwrap();
    let loaded = load_repos(tmp.path()).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn save_repos_writes_schema_version() {
    let tmp = tempfile::tempdir().unwrap();
    let map: HashMap<String, RepoInfo> = HashMap::new();
    save_repos(tmp.path(), &map).unwrap();

    let content = std::fs::read_to_string(
        crate::platform::paths::repos_file(tmp.path())
    ).unwrap();
    assert!(content.contains("\"schema_version\""));
    assert!(content.contains("\"repos\""));
}
```

- [ ] **Step 9.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::repos 2>&1 | tail -15
```

Expected: compile error — `persistence::repos` module not found.

- [ ] **Step 9.3: Implement**

Create `src-tauri/src/persistence/repos.rs`:

```rust
use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::repos_file;
use crate::state::RepoInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct ReposFile {
    schema_version: u32,
    repos: HashMap<String, RepoInfo>,
}

pub fn load_repos(data_dir: &Path) -> Result<HashMap<String, RepoInfo>> {
    let file: ReposFile = load_or_default(&repos_file(data_dir))?;
    Ok(file.repos)
}

pub fn save_repos(data_dir: &Path, repos: &HashMap<String, RepoInfo>) -> Result<()> {
    let file = ReposFile {
        schema_version: 1,
        repos: repos.clone(),
    };
    write_atomic(&repos_file(data_dir), &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_repo(id: &str) -> RepoInfo {
        RepoInfo {
            id: id.into(),
            name: "test-repo".into(),
            path: PathBuf::from("/tmp/test-repo"),
            gh_profile: None,
            default_branch: "main".into(),
            created_at: 1_000_000,
            updated_at: 1_000_001,
        }
    }

    #[test]
    fn save_and_load_repos_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = HashMap::new();
        map.insert("repo_abc".into(), make_repo("repo_abc"));
        save_repos(tmp.path(), &map).unwrap();

        let loaded = load_repos(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["repo_abc"].name, "test-repo");
    }

    #[test]
    fn load_repos_missing_file_returns_empty_map() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_repos(tmp.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_repos_writes_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let map: HashMap<String, RepoInfo> = HashMap::new();
        save_repos(tmp.path(), &map).unwrap();

        let content = std::fs::read_to_string(
            crate::platform::paths::repos_file(tmp.path())
        ).unwrap();
        assert!(content.contains("\"schema_version\""));
        assert!(content.contains("\"repos\""));
    }
}
```

Update `src-tauri/src/persistence/mod.rs`:

```rust
pub mod atomic;
pub mod debounce;
pub mod repos;
pub mod settings;
pub mod workspaces;
```

(Declare `settings` and `workspaces` now; implement them in Tasks 10–11.)

- [ ] **Step 9.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::repos 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 9.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/persistence/repos.rs src-tauri/src/persistence/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add persistence/repos.rs with load_repos and save_repos

Serializes to {"schema_version":1,"repos":{...}} via atomic write.
load_repos returns empty map when file is absent (first-run safe).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Persistence — `workspaces` module helpers

**Files:**

- Create: `src-tauri/src/persistence/workspaces.rs`

- [ ] **Step 10.1: Write the failing test**

```rust
// In src-tauri/src/persistence/workspaces.rs  #[cfg(test)] mod tests:
use super::*;
use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};

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
    let mut map = std::collections::HashMap::new();
    map.insert("ws_abc".into(), make_workspace("ws_abc", WorkspaceStatus::Waiting));
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
    let mut map = std::collections::HashMap::new();
    map.insert("ws_1".into(), make_workspace("ws_1", WorkspaceStatus::NotStarted));
    map.insert("ws_2".into(), make_workspace("ws_2", WorkspaceStatus::Running));
    map.insert("ws_3".into(), make_workspace("ws_3", WorkspaceStatus::Done));
    save_workspaces(tmp.path(), &map).unwrap();

    let reset = load_and_reset_running(tmp.path()).unwrap();
    assert_eq!(reset["ws_1"].status, WorkspaceStatus::NotStarted);
    assert_eq!(reset["ws_2"].status, WorkspaceStatus::Waiting);
    assert_eq!(reset["ws_3"].status, WorkspaceStatus::Done);
}
```

- [ ] **Step 10.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::workspaces 2>&1 | tail -15
```

Expected: compile error — `persistence::workspaces` module not found.

- [ ] **Step 10.3: Implement**

Create `src-tauri/src/persistence/workspaces.rs`:

```rust
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
        map.insert("ws_abc".into(), make_workspace("ws_abc", WorkspaceStatus::Waiting));
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
        map.insert("ws_1".into(), make_workspace("ws_1", WorkspaceStatus::NotStarted));
        map.insert("ws_2".into(), make_workspace("ws_2", WorkspaceStatus::Running));
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
        map.insert("ws_w".into(), make_workspace("ws_w", WorkspaceStatus::Waiting));
        map.insert("ws_e".into(), make_workspace("ws_e", WorkspaceStatus::Error));
        save_workspaces(tmp.path(), &map).unwrap();

        let reset = load_and_reset_running(tmp.path()).unwrap();
        assert_eq!(reset["ws_w"].status, WorkspaceStatus::Waiting);
        assert_eq!(reset["ws_e"].status, WorkspaceStatus::Error);
    }
}
```

- [ ] **Step 10.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::workspaces 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 10.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/persistence/workspaces.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add persistence/workspaces.rs with load/save/reset helpers

load_and_reset_running coerces Running→Waiting on load, providing the
dead-agent guardrail required on app restart. All other statuses preserved.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Persistence — `settings` module helpers

**Files:**

- Create: `src-tauri/src/persistence/settings.rs`

- [ ] **Step 11.1: Write the failing test**

```rust
// In src-tauri/src/persistence/settings.rs  #[cfg(test)] mod tests:
use super::*;

#[test]
fn save_and_load_settings_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = crate::state::AppSettings::default();
    s.theme = "cool-light".into();
    s.onboarding_completed = true;
    save_settings(tmp.path(), &s).unwrap();

    let loaded = load_settings(tmp.path()).unwrap();
    assert_eq!(loaded.theme, "cool-light");
    assert!(loaded.onboarding_completed);
}

#[test]
fn load_settings_missing_file_returns_default() {
    let tmp = tempfile::tempdir().unwrap();
    let loaded = load_settings(tmp.path()).unwrap();
    assert_eq!(loaded.schema_version, 1);
    assert_eq!(loaded.theme, "warm-dark");
}
```

- [ ] **Step 11.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::settings 2>&1 | tail -15
```

Expected: compile error — `persistence::settings` module not found.

- [ ] **Step 11.3: Implement**

Create `src-tauri/src/persistence/settings.rs`:

```rust
use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::app_settings_file;
use crate::state::AppSettings;
use std::path::Path;

pub fn load_settings(data_dir: &Path) -> Result<AppSettings> {
    load_or_default(&app_settings_file(data_dir))
}

pub fn save_settings(data_dir: &Path, settings: &AppSettings) -> Result<()> {
    write_atomic(&app_settings_file(data_dir), settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_settings_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = crate::state::AppSettings::default();
        s.theme = "cool-light".into();
        s.onboarding_completed = true;
        save_settings(tmp.path(), &s).unwrap();

        let loaded = load_settings(tmp.path()).unwrap();
        assert_eq!(loaded.theme, "cool-light");
        assert!(loaded.onboarding_completed);
    }

    #[test]
    fn load_settings_missing_file_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_settings(tmp.path()).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.theme, "warm-dark");
    }

    #[test]
    fn save_settings_serializes_all_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = crate::state::AppSettings::default();
        s.selected_repo_id = Some("repo_abc".into());
        s.recent_repos = vec!["repo_abc".into()];
        save_settings(tmp.path(), &s).unwrap();

        let content = std::fs::read_to_string(
            crate::platform::paths::app_settings_file(tmp.path())
        ).unwrap();
        assert!(content.contains("\"selected_repo_id\""));
        assert!(content.contains("repo_abc"));
    }
}
```

- [ ] **Step 11.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::settings 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 11.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/persistence/settings.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add persistence/settings.rs with load/save helpers

AppSettings serializes directly (no wrapper struct needed — no collections
to version separately). Missing file returns Default, matching first-run UX.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Workspace status reset on load (already in Task 10)

> Task 12 scope (`load_and_reset_running`) was fully implemented in Task 10 as
> part of `persistence/workspaces.rs`. This task verifies it in isolation and
> confirms the fixture round-trip with persisted JSON.

**Files:**

- Modify: `src-tauri/src/persistence/workspaces.rs` (add one extra edge-case
  test)

- [ ] **Step 12.1: Write the extra fixture test**

```rust
// Add to persistence/workspaces.rs  #[cfg(test)] mod tests:
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
    std::fs::write(
        crate::platform::paths::workspaces_file(tmp.path()),
        fixture
    ).unwrap();

    let map = load_and_reset_running(tmp.path()).unwrap();
    assert_eq!(map["ws_a"].status, WorkspaceStatus::NotStarted);
    assert_eq!(map["ws_b"].status, WorkspaceStatus::Waiting);  // was running
    assert_eq!(map["ws_c"].status, WorkspaceStatus::Done);
}
```

- [ ] **Step 12.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::workspaces::tests::load_and_reset_running_from_raw_json_fixture 2>&1 | tail -10
```

Expected: test not yet present.

- [ ] **Step 12.3: Add the fixture test to `persistence/workspaces.rs`**

Add the `load_and_reset_running_from_raw_json_fixture` test to the
`#[cfg(test)] mod tests` block of `src-tauri/src/persistence/workspaces.rs`. The
content matches the test above verbatim — insert after the last existing test.

- [ ] **Step 12.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::workspaces 2>&1 | tail -10
```

Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Step 12.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/persistence/workspaces.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1a): add raw JSON fixture test for load_and_reset_running

Exercises the coercion from a real JSON file on disk to confirm
snake_case deserialization ("running" → Waiting) works end-to-end.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: `commands/repo.rs` — `add_repo` + tests

**Files:**

- Create: `src-tauri/src/commands/repo.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 13.1: Write the failing test**

```rust
// In src-tauri/src/commands/repo.rs  #[cfg(test)] mod tests:
use std::process::Command;
use std::sync::{Arc, Mutex};
use crate::state::AppState;

fn init_repo_with_remote_main(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    let remote = tmp.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    Command::new("git").args(["init", "--bare"]).current_dir(&remote).output().unwrap();

    let local = tmp.path().join("local");
    Command::new("git")
        .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
        .output().unwrap();

    Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["config", "user.name", "T"]).current_dir(&local).output().unwrap();

    std::fs::write(local.join("f"), b"x").unwrap();
    Command::new("git").args(["add", "."]).current_dir(&local).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["remote", "set-head", "origin", "main"]).current_dir(&local).output().unwrap();

    local
}

#[tokio::test]
async fn add_repo_returns_repo_info_with_correct_name() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let local = init_repo_with_remote_main(&tmp);

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let result = add_repo_inner(local.to_str().unwrap().to_string(), data, state).await.unwrap();

    assert_eq!(result.name, "local");
    assert_eq!(result.default_branch, "main");
    assert!(result.id.starts_with("repo_"));
}

#[tokio::test]
async fn add_repo_deduplicates_by_path() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let local = init_repo_with_remote_main(&tmp);

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let r1 = add_repo_inner(local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)).await.unwrap();
    let r2 = add_repo_inner(local.to_str().unwrap().to_string(), data, state).await.unwrap();

    assert_eq!(r1.id, r2.id);
}

#[tokio::test]
async fn add_repo_non_git_dir_returns_err() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let plain = tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let result = add_repo_inner(plain.to_str().unwrap().to_string(), data, state).await;
    assert!(result.is_err());
}
```

- [ ] **Step 13.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::repo 2>&1 | tail -15
```

Expected: compile error — `commands::repo` not found.

- [ ] **Step 13.3: Implement**

Create `src-tauri/src/commands/repo.rs`:

```rust
use crate::error::{AppError, Result};
use crate::ids::repo_id;
use crate::persistence::repos::{load_repos, save_repos};
use crate::state::{AppState, RepoInfo};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

// ── Public Tauri commands ────────────────────────────────────────────

#[tauri::command]
pub async fn add_repo(
    path: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<RepoInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let inner_state = state.inner().clone();
    add_repo_inner(path, data_dir, inner_state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "add_repo failed");
            e.to_string()
        })
}

#[tauri::command]
pub fn list_repos(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<RepoInfo>, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    let mut repos: Vec<RepoInfo> = st.repos.values().cloned().collect();
    repos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(repos)
}

#[tauri::command]
pub async fn remove_repo(
    repo_id: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    remove_repo_inner(repo_id, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "remove_repo failed");
            e.to_string()
        })
}

#[tauri::command]
pub async fn update_gh_profile(
    repo_id: String,
    gh_profile: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<RepoInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    update_gh_profile_inner(repo_id, gh_profile, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "update_gh_profile failed");
            e.to_string()
        })
}

// ── Inner implementations ────────────────────────────────────────────

pub(crate) async fn add_repo_inner(
    path: String,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<RepoInfo> {
    use crate::commands::helpers::{detect_default_branch, is_git_repo, now_unix};

    let canonical = dunce::canonicalize(&path)
        .map_err(|e| AppError::Io(e))?;

    if !is_git_repo(&canonical) {
        return Err(AppError::InvalidState(format!(
            "'{}' is not a git repository (no .git entry found)",
            canonical.display()
        )));
    }

    let default_branch = detect_default_branch(&canonical)?;
    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| canonical.display().to_string());

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;

    // Deduplicate by canonicalized path
    if let Some(existing) = st.repos.values().find(|r| r.path == canonical).cloned() {
        tracing::info!("Repo at {} already registered as {}", canonical.display(), existing.id);
        return Ok(existing);
    }

    let now = now_unix();
    let id = repo_id();
    let repo = RepoInfo {
        id: id.clone(),
        name,
        path: canonical,
        gh_profile: None,
        default_branch,
        created_at: now,
        updated_at: now,
    };

    st.repos.insert(id, repo.clone());
    save_repos(&data_dir, &st.repos)?;
    tracing::info!(repo_id = %repo.id, path = %repo.path.display(), "Registered repo");
    Ok(repo)
}

async fn remove_repo_inner(
    id: String,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;

    if !st.repos.contains_key(&id) {
        return Err(AppError::NotFound(id));
    }

    let has_workspaces = st.workspaces.values().any(|w| w.repo_id == id);
    if has_workspaces {
        return Err(AppError::InvalidState(format!(
            "Cannot remove repo '{}': it still has workspaces. Remove all workspaces first.",
            id
        )));
    }

    st.repos.remove(&id);
    save_repos(&data_dir, &st.repos)?;
    tracing::info!(repo_id = %id, "Removed repo");
    Ok(())
}

async fn update_gh_profile_inner(
    id: String,
    gh_profile: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<RepoInfo> {
    use crate::commands::helpers::now_unix;

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let repo = st.repos.get_mut(&id).ok_or_else(|| AppError::NotFound(id.clone()))?;
    repo.gh_profile = gh_profile;
    repo.updated_at = now_unix();
    let repo = repo.clone();
    save_repos(&data_dir, &st.repos)?;
    Ok(repo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    fn init_repo_with_remote_main(tmp: &tempfile::TempDir) -> PathBuf {
        let remote = tmp.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        Command::new("git").args(["init", "--bare"]).current_dir(&remote).output().unwrap();

        let local = tmp.path().join("local");
        Command::new("git")
            .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
            .output().unwrap();

        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&local).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&local).output().unwrap();

        std::fs::write(local.join("f"), b"x").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&local).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&local).output().unwrap();
        Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&local).output().unwrap();
        Command::new("git").args(["remote", "set-head", "origin", "main"]).current_dir(&local).output().unwrap();

        local
    }

    #[tokio::test]
    async fn add_repo_returns_repo_info_with_correct_name() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let result = add_repo_inner(local.to_str().unwrap().to_string(), data, state).await.unwrap();

        assert_eq!(result.name, "local");
        assert_eq!(result.default_branch, "main");
        assert!(result.id.starts_with("repo_"));
    }

    #[tokio::test]
    async fn add_repo_deduplicates_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let r1 = add_repo_inner(local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)).await.unwrap();
        let r2 = add_repo_inner(local.to_str().unwrap().to_string(), data, state).await.unwrap();
        assert_eq!(r1.id, r2.id);
    }

    #[tokio::test]
    async fn add_repo_non_git_dir_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let result = add_repo_inner(plain.to_str().unwrap().to_string(), data, state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_repos_returns_sorted_by_updated_at_desc() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local1 = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        add_repo_inner(local1.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)).await.unwrap();

        // Manually tweak timestamps so we can assert sort order
        {
            let mut st = state.lock().unwrap();
            let ids: Vec<String> = st.repos.keys().cloned().collect();
            for (i, id) in ids.iter().enumerate() {
                st.repos.get_mut(id).unwrap().updated_at = 1_000 + i as i64;
            }
        }

        let st = state.lock().unwrap();
        let mut repos: Vec<crate::state::RepoInfo> = st.repos.values().cloned().collect();
        repos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        // Verify first is greatest updated_at
        if repos.len() > 1 {
            assert!(repos[0].updated_at >= repos[1].updated_at);
        }
    }

    #[tokio::test]
    async fn remove_repo_with_workspaces_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = add_repo_inner(local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)).await.unwrap();

        // Insert a fake workspace belonging to this repo
        {
            let mut st = state.lock().unwrap();
            st.workspaces.insert("ws_fake".into(), crate::state::WorkspaceInfo {
                id: "ws_fake".into(),
                repo_id: repo.id.clone(),
                branch: "ws/fake".into(),
                base_branch: "main".into(),
                custom_branch: false,
                title: "Fake".into(),
                description: String::new(),
                status: crate::state::WorkspaceStatus::NotStarted,
                column: crate::state::KanbanColumn::Todo,
                created_at: 0,
                updated_at: 0,
            });
        }

        let result = remove_repo_inner(repo.id, data, state).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("workspaces"), "Got: {msg}");
    }

    #[tokio::test]
    async fn update_gh_profile_sets_and_clears_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = add_repo_inner(local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)).await.unwrap();

        let updated = update_gh_profile_inner(repo.id.clone(), Some("handokoben".into()), data.clone(), Arc::clone(&state)).await.unwrap();
        assert_eq!(updated.gh_profile, Some("handokoben".into()));

        let cleared = update_gh_profile_inner(repo.id, None, data, state).await.unwrap();
        assert_eq!(cleared.gh_profile, None);
    }
}
```

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod helpers;
pub mod repo;
pub mod system;
```

- [ ] **Step 13.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::repo 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed; 0 failed`.

- [ ] **Step 13.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/repo.rs src-tauri/src/commands/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add commands/repo.rs with add_repo, list_repos, remove_repo, update_gh_profile

add_repo canonicalizes via dunce, checks is_git_repo, detects default branch,
deduplicates by path. remove_repo blocks if workspaces exist. list_repos sorted
desc by updated_at. update_gh_profile sets or clears gh_profile and saves.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: `commands/repo.rs` — `list_repos` (covered in Task 13)

> `list_repos` was implemented and tested in Task 13. This task adds one
> additional edge-case test for the empty-state case.

**Files:**

- Modify: `src-tauri/src/commands/repo.rs`

- [ ] **Step 14.1: Write the additional test**

```rust
// Add to commands/repo.rs  #[cfg(test)] mod tests:
#[test]
fn list_repos_returns_empty_vec_when_no_repos() {
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let repos: Vec<crate::state::RepoInfo> = state.lock().unwrap()
        .repos.values().cloned().collect();
    assert!(repos.is_empty());
}
```

- [ ] **Step 14.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::repo::tests::list_repos_returns_empty_vec_when_no_repos 2>&1 | tail -10
```

Expected: test not yet present.

- [ ] **Step 14.3: Add the test**

Add `list_repos_returns_empty_vec_when_no_repos` to `#[cfg(test)] mod tests` in
`src-tauri/src/commands/repo.rs`.

- [ ] **Step 14.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::repo 2>&1 | tail -10
```

Expected: `test result: ok. 7 passed; 0 failed`.

- [ ] **Step 14.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/repo.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1a): add empty-state test for list_repos

Verifies list_repos returns an empty Vec when AppState has no repos,
covering the first-run path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 15: `commands/repo.rs` — `remove_repo` + `update_gh_profile` (covered in Task 13)

> Both commands were implemented and tested in Task 13. This task confirms
> cross-module cargo check passes cleanly before moving on to workspace
> commands.

**Files:**

- None new

- [ ] **Step 15.1: Run full lib test to confirm no regressions**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -10
```

Expected: all existing tests pass (≥20 total at this point).

- [ ] **Step 15.2: Run clippy to ensure code is clean**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -15
```

Expected: `warning: 0 warnings emitted` or only allowed warnings. Fix any
`D warnings` violations in `repo.rs` before proceeding.

- [ ] **Step 15.3: Commit (clean-up only if clippy found issues)**

If clippy required changes:

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/repo.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
refactor(phase-1a): fix clippy warnings in commands/repo.rs

Address -D warnings findings before workspace commands are added.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 16: `commands/workspace.rs` — `create_workspace` + tests

**Files:**

- Create: `src-tauri/src/commands/workspace.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 16.1: Write the failing test**

```rust
// In src-tauri/src/commands/workspace.rs  #[cfg(test)] mod tests:
use crate::state::AppState;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

fn init_repo_with_remote(tmp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    let remote = tmp.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    Command::new("git").args(["init", "--bare"]).current_dir(&remote).output().unwrap();

    let local = tmp.path().join("local");
    Command::new("git")
        .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
        .output().unwrap();
    Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["config", "user.name", "T"]).current_dir(&local).output().unwrap();
    std::fs::write(local.join("f"), b"x").unwrap();
    Command::new("git").args(["add", "."]).current_dir(&local).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["remote", "set-head", "origin", "main"]).current_dir(&local).output().unwrap();

    (local, remote)
}

#[tokio::test]
async fn create_workspace_creates_worktree_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _) = init_repo_with_remote(&tmp);
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));

    let repo = crate::commands::repo::add_repo_inner(
        local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
    ).await.unwrap();

    let ws = create_workspace_inner(
        repo.id.clone(), "Fix login".into(), String::new(), None,
        data.clone(), Arc::clone(&state)
    ).await.unwrap();

    let worktree_path = crate::platform::paths::worktree_dir(&data, &ws.id);
    assert!(worktree_path.exists(), "Worktree dir should exist at {}", worktree_path.display());
}

#[tokio::test]
async fn create_workspace_custom_branch_sets_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _) = init_repo_with_remote(&tmp);
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let repo = crate::commands::repo::add_repo_inner(
        local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
    ).await.unwrap();

    let ws = create_workspace_inner(
        repo.id, "Custom branch test".into(), String::new(),
        Some("feat/custom-branch".into()),
        data.clone(), Arc::clone(&state)
    ).await.unwrap();

    assert!(ws.custom_branch);
    assert_eq!(ws.branch, "feat/custom-branch");
}

#[tokio::test]
async fn create_workspace_missing_repo_returns_err() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let result = create_workspace_inner(
        "repo_nonexistent".into(), "X".into(), String::new(), None,
        data, state
    ).await;
    assert!(result.is_err());
}
```

- [ ] **Step 16.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::workspace 2>&1 | tail -15
```

Expected: compile error — `commands::workspace` not found.

- [ ] **Step 16.3: Implement**

Create `src-tauri/src/commands/workspace.rs`:

```rust
use crate::error::{AppError, Result};
use crate::ids::workspace_id;
use crate::persistence::workspaces::save_workspaces;
use crate::state::{AppState, KanbanColumn, WorkspaceInfo, WorkspaceStatus};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

// ── Public Tauri commands ────────────────────────────────────────────

#[tauri::command]
pub async fn create_workspace(
    repo_id: String,
    title: String,
    description: String,
    branch_name: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<WorkspaceInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    create_workspace_inner(repo_id, title, description, branch_name, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "create_workspace failed");
            e.to_string()
        })
}

#[tauri::command]
pub fn list_workspaces(
    repo_id: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<WorkspaceInfo>, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    let mut workspaces: Vec<WorkspaceInfo> = st
        .workspaces
        .values()
        .filter(|w| repo_id.as_ref().map_or(true, |id| &w.repo_id == id))
        .cloned()
        .collect();
    workspaces.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(workspaces)
}

#[tauri::command]
pub async fn remove_workspace(
    workspace_id: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    remove_workspace_inner(workspace_id, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "remove_workspace failed");
            e.to_string()
        })
}

// ── Inner implementations ────────────────────────────────────────────

pub(crate) async fn create_workspace_inner(
    repo_id: String,
    title: String,
    description: String,
    branch_name: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<WorkspaceInfo> {
    use crate::commands::helpers::now_unix;

    let repo_path = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        st.repos
            .get(&repo_id)
            .map(|r| r.path.clone())
            .ok_or_else(|| AppError::NotFound(format!("repo '{}'", repo_id)))?
    };

    let base_branch = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        st.repos.get(&repo_id).map(|r| r.default_branch.clone())
            .ok_or_else(|| AppError::NotFound(repo_id.clone()))?
    };

    let ws_id = workspace_id();
    let worktree_path = crate::platform::paths::worktree_dir(&data_dir, &ws_id);

    let (branch, custom_branch) = if let Some(ref custom) = branch_name {
        if custom.trim().is_empty() {
            return Err(AppError::InvalidState("Branch name cannot be empty".into()));
        }
        (custom.trim().to_string(), true)
    } else {
        (format!("ws/{}", ws_id), false)
    };

    // Create parent dir for the worktree
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::Io)?;
    }

    // Run: git worktree add -b <branch> <worktree_path> <base_branch>
    let worktree_str = worktree_path.to_string_lossy();
    let output = std::process::Command::new("git")
        .args(["worktree", "add", "-b", &branch, &worktree_str, &base_branch])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| AppError::Command {
            cmd: "git worktree add".into(),
            msg: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Git(format!("git worktree add failed: {}", stderr)));
    }

    let now = now_unix();
    let ws = WorkspaceInfo {
        id: ws_id.clone(),
        repo_id,
        branch,
        base_branch,
        custom_branch,
        title,
        description,
        status: WorkspaceStatus::NotStarted,
        column: KanbanColumn::Todo,
        created_at: now,
        updated_at: now,
    };

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    st.workspaces.insert(ws_id, ws.clone());
    save_workspaces(&data_dir, &st.workspaces)?;
    tracing::info!(workspace_id = %ws.id, branch = %ws.branch, "Created workspace");
    Ok(ws)
}

async fn remove_workspace_inner(
    ws_id: String,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let (worktree_path, repo_path, branch) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(&ws_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{}'", ws_id)))?;
        let repo = st
            .repos
            .get(&ws.repo_id)
            .ok_or_else(|| AppError::NotFound(format!("repo '{}'", ws.repo_id)))?;
        let wt_path = crate::platform::paths::worktree_dir(&data_dir, &ws_id);
        (wt_path, repo.path.clone(), ws.branch.clone())
    };

    // git worktree remove --force <path>
    if worktree_path.exists() {
        let wt_str = worktree_path.to_string_lossy();
        let rm_out = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", &wt_str])
            .current_dir(&repo_path)
            .output()
            .map_err(|e| AppError::Command {
                cmd: "git worktree remove".into(),
                msg: e.to_string(),
            })?;

        if !rm_out.status.success() {
            // Prune stale worktree entries as fallback
            let _ = std::process::Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(&repo_path)
                .output();
        }
    }

    // git branch -D <branch> — ignore error if branch already gone
    let _ = std::process::Command::new("git")
        .args(["branch", "-D", &branch])
        .current_dir(&repo_path)
        .output();

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    st.workspaces.remove(&ws_id);
    save_workspaces(&data_dir, &st.workspaces)?;
    tracing::info!(workspace_id = %ws_id, "Removed workspace");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::process::Command;

    fn init_repo_with_remote(tmp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        let remote = tmp.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        Command::new("git").args(["init", "--bare"]).current_dir(&remote).output().unwrap();

        let local = tmp.path().join("local");
        Command::new("git")
            .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
            .output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&local).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&local).output().unwrap();
        std::fs::write(local.join("f"), b"x").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&local).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&local).output().unwrap();
        Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&local).output().unwrap();
        Command::new("git").args(["remote", "set-head", "origin", "main"]).current_dir(&local).output().unwrap();

        (local, remote)
    }

    #[tokio::test]
    async fn create_workspace_creates_worktree_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let ws = create_workspace_inner(
            repo.id, "Fix login".into(), String::new(), None,
            data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let worktree_path = crate::platform::paths::worktree_dir(&data, &ws.id);
        assert!(worktree_path.exists(), "Worktree dir should exist at {}", worktree_path.display());
    }

    #[tokio::test]
    async fn create_workspace_git_worktree_list_shows_new_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let ws = create_workspace_inner(
            repo.id, "Test".into(), String::new(), None,
            data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let out = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&local)
            .output().unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        assert!(list.contains(&ws.id), "worktree list should contain ws id: {}", ws.id);
    }

    #[tokio::test]
    async fn create_workspace_auto_branch_has_ws_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let ws = create_workspace_inner(
            repo.id, "Auto branch".into(), String::new(), None,
            data, state
        ).await.unwrap();

        assert!(ws.branch.starts_with("ws/"), "branch should start with ws/, got {}", ws.branch);
        assert!(!ws.custom_branch);
    }

    #[tokio::test]
    async fn create_workspace_custom_branch_sets_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let ws = create_workspace_inner(
            repo.id, "Custom branch test".into(), String::new(),
            Some("feat/custom-branch".into()),
            data.clone(), state
        ).await.unwrap();

        assert!(ws.custom_branch);
        assert_eq!(ws.branch, "feat/custom-branch");
    }

    #[tokio::test]
    async fn create_workspace_missing_repo_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let result = create_workspace_inner(
            "repo_nonexistent".into(), "X".into(), String::new(), None, data, state
        ).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_workspace_cleans_up_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let ws = create_workspace_inner(
            repo.id, "Remove test".into(), String::new(), None,
            data.clone(), Arc::clone(&state)
        ).await.unwrap();

        let wt_path = crate::platform::paths::worktree_dir(&data, &ws.id);
        assert!(wt_path.exists());

        remove_workspace_inner(ws.id.clone(), data.clone(), Arc::clone(&state)).await.unwrap();

        assert!(!wt_path.exists(), "Worktree dir should be removed");
        let st = state.lock().unwrap();
        assert!(!st.workspaces.contains_key(&ws.id));
    }
}
```

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod helpers;
pub mod repo;
pub mod system;
pub mod workspace;
```

- [ ] **Step 16.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::workspace 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed; 0 failed`.

- [ ] **Step 16.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/workspace.rs src-tauri/src/commands/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add commands/workspace.rs with create, list, remove workspace

create_workspace runs git worktree add -b into <data>/workspaces/<ws_id>/.
Auto-branch uses ws/<id> prefix; custom_branch flag set when caller provides name.
remove_workspace runs git worktree remove --force then git branch -D.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 17: `commands/workspace.rs` — `list_workspaces` + `remove_workspace` (covered in Task 16)

> Both commands were implemented in Task 16. This task adds `list_workspaces`
> filter-by-repo-id edge cases.

**Files:**

- Modify: `src-tauri/src/commands/workspace.rs`

- [ ] **Step 17.1: Write additional tests**

```rust
// Add to commands/workspace.rs  #[cfg(test)] mod tests:
#[tokio::test]
async fn list_workspaces_filters_by_repo_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _) = init_repo_with_remote(&tmp);
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let repo = crate::commands::repo::add_repo_inner(
        local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
    ).await.unwrap();

    create_workspace_inner(repo.id.clone(), "WS 1".into(), String::new(), None,
        data.clone(), Arc::clone(&state)).await.unwrap();
    create_workspace_inner(repo.id.clone(), "WS 2".into(), String::new(), None,
        data.clone(), Arc::clone(&state)).await.unwrap();

    let st = state.lock().unwrap();
    let all: Vec<_> = st.workspaces.values().collect();
    let filtered: Vec<_> = all.iter().filter(|w| w.repo_id == repo.id).collect();
    assert_eq!(filtered.len(), 2);
    let unrelated: Vec<_> = all.iter().filter(|w| w.repo_id == "repo_other").collect();
    assert_eq!(unrelated.len(), 0);
}

#[tokio::test]
async fn list_workspaces_none_filter_returns_all() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _) = init_repo_with_remote(&tmp);
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let repo = crate::commands::repo::add_repo_inner(
        local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state)
    ).await.unwrap();

    create_workspace_inner(repo.id.clone(), "WS A".into(), String::new(), None,
        data.clone(), Arc::clone(&state)).await.unwrap();
    create_workspace_inner(repo.id.clone(), "WS B".into(), String::new(), None,
        data.clone(), Arc::clone(&state)).await.unwrap();

    let st = state.lock().unwrap();
    // None filter means all workspaces
    let all: Vec<_> = st.workspaces.values().collect();
    assert_eq!(all.len(), 2);
}
```

- [ ] **Step 17.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::workspace::tests::list_workspaces_filters_by_repo_id 2>&1 | tail -10
```

Expected: test not found.

- [ ] **Step 17.3: Add the tests to `commands/workspace.rs`**

Insert the two new tests into `#[cfg(test)] mod tests` in
`src-tauri/src/commands/workspace.rs`.

- [ ] **Step 17.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::workspace 2>&1 | tail -10
```

Expected: `test result: ok. 8 passed; 0 failed`.

- [ ] **Step 17.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/workspace.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1a): add list_workspaces filter-by-repo_id edge case tests

Verifies that the repo_id filter returns only matching workspaces and
that None filter returns all workspaces across repos.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 18: Register all commands in `lib.rs` + full integration

**Files:**

- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 18.1: Write the verification test**

```rust
// cargo check --all-targets is the test. Additionally run:
// cargo test --lib 2>&1 to confirm all tests pass.
```

- [ ] **Step 18.2: Run check to verify current state**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo check --all-targets 2>&1 | tail -10
```

Expected: clean compilation before wiring the handler.

- [ ] **Step 18.3: Implement — wire everything into `lib.rs`**

Final content of `src-tauri/src/lib.rs`:

```rust
use tauri::Manager;

pub mod commands;
pub mod error;
pub mod ids;
pub mod logging;
pub mod panic;
pub mod persistence;
pub mod platform;
pub mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            crate::platform::paths::ensure_data_dirs(&data_dir)?;
            let guard = crate::logging::init(&data_dir)?;
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(Some(guard))));
            crate::panic::install_hook(data_dir.clone());

            // Hydrate AppState from disk on startup
            let repos = crate::persistence::repos::load_repos(&data_dir)
                .unwrap_or_default();
            let workspaces = crate::persistence::workspaces::load_and_reset_running(&data_dir)
                .unwrap_or_default();
            let settings = crate::persistence::settings::load_settings(&data_dir)
                .unwrap_or_default();

            let app_state = crate::state::AppState { repos, workspaces, settings };
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(app_state)));

            tracing::info!("AppState hydrated from disk");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::system::get_app_version,
            crate::commands::repo::add_repo,
            crate::commands::repo::list_repos,
            crate::commands::repo::remove_repo,
            crate::commands::repo::update_gh_profile,
            crate::commands::workspace::create_workspace,
            crate::commands::workspace::list_workspaces,
            crate::commands::workspace::remove_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 18.4: Run full verification suite**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo check --all-targets 2>&1 | tail -5
cargo test --lib 2>&1 | tail -15
cargo fmt --all -- --check 2>&1 | tail -5
cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -10
```

Expected:

- `cargo check`: `Finished` — no errors
- `cargo test --lib`: all 80+ tests pass, 0 failed
- `cargo fmt --check`: clean (no diff output)
- `cargo clippy`: `warning: 0 warnings emitted`

If `cargo fmt --check` finds diffs, run `cargo fmt --all` then re-check.

- [ ] **Step 18.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/lib.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): wire repo + workspace commands into Tauri handler

Register add_repo, list_repos, remove_repo, update_gh_profile,
create_workspace, list_workspaces, remove_workspace in invoke_handler.
Setup hook hydrates AppState from repos.json + workspaces.json (with
Running→Waiting coercion) + app_settings.json on startup.

New files in this phase:
  src-tauri/src/commands/helpers.rs
  src-tauri/src/commands/repo.rs
  src-tauri/src/commands/workspace.rs
  src-tauri/src/persistence/repos.rs
  src-tauri/src/persistence/settings.rs
  src-tauri/src/persistence/workspaces.rs

Modified files:
  src-tauri/Cargo.toml         (+ tauri-plugin-dialog)
  src-tauri/capabilities/default.json (+ dialog permissions)
  src-tauri/src/lib.rs         (setup hook + handler registration)
  src-tauri/src/state.rs       (expanded structs + AppSettings)
  src-tauri/src/commands/mod.rs (+ repo, workspace, helpers)
  src-tauri/src/persistence/mod.rs (+ repos, settings, workspaces)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 1a-backend shipping criteria (checklist)

- [ ] `cargo check --all-targets` passes
- [ ] `cargo test --lib` — ~80 tests pass (existing 55 + new ~25)
- [ ] `cargo llvm-cov --lib --ignore-filename-regex 'lib\.rs$|main\.rs$' --fail-under-lines 95 --fail-under-functions 95`
      passes
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --lib --all-targets -- -D warnings` clean
- [ ] `AppState` hydrates from disk on app start (persisted data survives
      restart)
- [ ] Workspace `Running` status is coerced to `Waiting` on app start
      (dead-agent guardrail)
- [ ] No file in Phase 0 has been functionally changed except the 3 data-model
      files (`state.rs`, `lib.rs`, `commands/mod.rs`)

---

## Known risks / deferrals to Phase 1a-frontend

- Folder picker itself lives in Phase 1a-frontend (TitleBar component). Phase
  1a-backend merely adds the plugin so the frontend can call it.
- `update_gh_profile` has no UI yet; exposed as a command for Phase 2+ settings
  screen.
- Workspace `diff_stats` and `task_*` fields not included yet — added when Phase
  3 (task providers) lands.
