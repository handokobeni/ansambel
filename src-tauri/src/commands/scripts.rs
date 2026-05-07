// Phase 2b · Task 1 — RepoScript list / set commands.
//
// Read-side surface for the script runner. Returns the persisted scripts
// for a given repo (empty when none configured). The set side is wired
// for the future settings UI in Phase 8 — Phase 2b ships read-only,
// but the command exists now so the frontend type surface stays stable.
//
// `script_run` (Task 5) lives in this module too; it spawns the actual
// script via PTY and streams output through the workspace terminal
// broadcaster.

use crate::error::{AppError, Result};
use crate::persistence::repos::save_repos;
use crate::state::{AppState, RepoScript};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

pub fn script_list_inner(repo_id: &str, state: Arc<Mutex<AppState>>) -> Result<Vec<RepoScript>> {
    let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let repo = st
        .repos
        .get(repo_id)
        .ok_or_else(|| AppError::NotFound(format!("repo '{repo_id}'")))?;
    Ok(repo.scripts.clone())
}

pub fn script_set_inner(
    repo_id: &str,
    scripts: Vec<RepoScript>,
    data_dir: &std::path::Path,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    // Validate before mutating: every script needs a non-empty id, name,
    // command. Duplicate ids inside the same set are rejected so the
    // frontend's "by id" lookup stays unambiguous.
    let mut seen = std::collections::HashSet::new();
    for s in &scripts {
        if s.id.trim().is_empty() {
            return Err(AppError::InvalidState("script id is empty".into()));
        }
        if s.name.trim().is_empty() {
            return Err(AppError::InvalidState(format!(
                "script '{}' has empty name",
                s.id
            )));
        }
        if s.command.trim().is_empty() {
            return Err(AppError::InvalidState(format!(
                "script '{}' has empty command",
                s.id
            )));
        }
        if !seen.insert(s.id.clone()) {
            return Err(AppError::InvalidState(format!(
                "duplicate script id '{}'",
                s.id
            )));
        }
    }

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let repo = st
        .repos
        .get_mut(repo_id)
        .ok_or_else(|| AppError::NotFound(format!("repo '{repo_id}'")))?;
    repo.scripts = scripts;
    save_repos(data_dir, &st.repos)?;
    Ok(())
}

#[tauri::command]
pub async fn script_list(
    repo_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<RepoScript>, String> {
    script_list_inner(&repo_id, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn script_set(
    repo_id: String,
    scripts: Vec<RepoScript>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir: PathBuf = app.path().app_data_dir().map_err(|e| e.to_string())?;
    script_set_inner(&repo_id, scripts, &data_dir, state.inner().clone()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RepoInfo;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn make_state_with_repo(repo_id: &str) -> Arc<Mutex<AppState>> {
        let mut state = AppState::default();
        state.repos.insert(
            repo_id.into(),
            RepoInfo {
                id: repo_id.into(),
                name: "test".into(),
                path: PathBuf::from("/tmp/test"),
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );
        Arc::new(Mutex::new(state))
    }

    fn data_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        tmp
    }

    // ── script_list ─────────────────────────────────────────────────

    #[test]
    fn script_list_inner_returns_empty_for_repo_with_no_scripts() {
        let state = make_state_with_repo("repo_a");
        let scripts = script_list_inner("repo_a", state).unwrap();
        assert!(scripts.is_empty());
    }

    #[test]
    fn script_list_inner_returns_persisted_scripts_in_order() {
        let state = make_state_with_repo("repo_a");
        {
            let mut st = state.lock().unwrap();
            let repo = st.repos.get_mut("repo_a").unwrap();
            repo.scripts = vec![
                RepoScript {
                    id: "sc_1".into(),
                    name: "Run tests".into(),
                    command: "bun test".into(),
                },
                RepoScript {
                    id: "sc_2".into(),
                    name: "Lint".into(),
                    command: "bun run lint".into(),
                },
            ];
        }
        let scripts = script_list_inner("repo_a", state).unwrap();
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].id, "sc_1");
        assert_eq!(scripts[1].name, "Lint");
    }

    #[test]
    fn script_list_inner_returns_error_for_unknown_repo() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = script_list_inner("repo_missing", state).unwrap_err();
        assert!(err.to_string().contains("repo_missing"));
    }

    // ── script_set ──────────────────────────────────────────────────

    #[test]
    fn script_set_inner_replaces_existing_list_atomically() {
        let state = make_state_with_repo("repo_a");
        let data = data_dir();

        let initial = vec![RepoScript {
            id: "sc_1".into(),
            name: "Old".into(),
            command: "echo old".into(),
        }];
        script_set_inner("repo_a", initial, data.path(), Arc::clone(&state)).unwrap();
        assert_eq!(state.lock().unwrap().repos["repo_a"].scripts.len(), 1);

        let replacement = vec![
            RepoScript {
                id: "sc_2".into(),
                name: "New".into(),
                command: "echo new".into(),
            },
            RepoScript {
                id: "sc_3".into(),
                name: "Build".into(),
                command: "cargo build".into(),
            },
        ];
        script_set_inner("repo_a", replacement, data.path(), Arc::clone(&state)).unwrap();
        let scripts = &state.lock().unwrap().repos["repo_a"].scripts;
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].id, "sc_2");
        // The previous `sc_1` is gone — replaces, not appends.
        assert!(scripts.iter().all(|s| s.id != "sc_1"));
    }

    #[test]
    fn script_set_inner_persists_to_repos_json() {
        let state = make_state_with_repo("repo_a");
        let data = data_dir();
        let scripts = vec![RepoScript {
            id: "sc_1".into(),
            name: "Tests".into(),
            command: "bun test".into(),
        }];
        script_set_inner("repo_a", scripts, data.path(), Arc::clone(&state)).unwrap();

        // Re-load from disk via the persistence layer to prove the write
        // wasn't only in-memory.
        let reloaded = crate::persistence::repos::load_repos(data.path()).unwrap();
        assert_eq!(reloaded["repo_a"].scripts.len(), 1);
        assert_eq!(reloaded["repo_a"].scripts[0].name, "Tests");
    }

    #[test]
    fn script_set_inner_rejects_empty_id() {
        let state = make_state_with_repo("repo_a");
        let data = data_dir();
        let scripts = vec![RepoScript {
            id: String::new(),
            name: "x".into(),
            command: "y".into(),
        }];
        let err = script_set_inner("repo_a", scripts, data.path(), state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("script id"));
    }

    #[test]
    fn script_set_inner_rejects_empty_name() {
        let state = make_state_with_repo("repo_a");
        let data = data_dir();
        let scripts = vec![RepoScript {
            id: "sc_1".into(),
            name: "  ".into(),
            command: "y".into(),
        }];
        let err = script_set_inner("repo_a", scripts, data.path(), state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("name"));
    }

    #[test]
    fn script_set_inner_rejects_empty_command() {
        let state = make_state_with_repo("repo_a");
        let data = data_dir();
        let scripts = vec![RepoScript {
            id: "sc_1".into(),
            name: "x".into(),
            command: "   ".into(),
        }];
        let err = script_set_inner("repo_a", scripts, data.path(), state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("command"));
    }

    #[test]
    fn script_set_inner_rejects_duplicate_ids() {
        let state = make_state_with_repo("repo_a");
        let data = data_dir();
        let scripts = vec![
            RepoScript {
                id: "sc_dup".into(),
                name: "A".into(),
                command: "echo a".into(),
            },
            RepoScript {
                id: "sc_dup".into(),
                name: "B".into(),
                command: "echo b".into(),
            },
        ];
        let err = script_set_inner("repo_a", scripts, data.path(), state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("duplicate"));
    }

    #[test]
    fn script_set_inner_returns_error_for_unknown_repo() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let data = data_dir();
        let err = script_set_inner("repo_missing", vec![], data.path(), state).unwrap_err();
        assert!(err.to_string().contains("repo_missing"));
    }
}
