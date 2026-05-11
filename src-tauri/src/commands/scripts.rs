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

// ── Task 5: script_run ───────────────────────────────────────────────
//
// Spawns the configured script via PTY rooted at the workspace's
// worktree. Output streams over a Tauri Channel<TerminalChunk>: bytes
// for stdout chunks, then a final Exited{code}. Frontend feeds these
// into the same xterm.js buffer the interactive shell writes to.
//
// One PTY per script run (ephemeral). The interactive workspace
// terminal (terminal_spawn) is a separate session and is not affected.

use crate::platform::pty;
use crate::state::TerminalChunk;
use portable_pty::CommandBuilder;
use tauri::ipc::Channel;

pub fn script_run_inner<F>(
    workspace_id: &str,
    script_id: &str,
    state: Arc<Mutex<AppState>>,
    emit: F,
) -> crate::error::Result<()>
where
    F: FnMut(TerminalChunk) + Send + 'static,
{
    // Resolve workspace + worktree dir + script command. Fail fast on
    // any missing piece so the frontend gets a clear error per cause.
    let (worktree, command) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        let repo = st
            .repos
            .get(&ws.repo_id)
            .ok_or_else(|| AppError::NotFound(format!("repo '{}' for workspace", ws.repo_id)))?;
        let script = repo
            .scripts
            .iter()
            .find(|s| s.id == script_id)
            .ok_or_else(|| AppError::NotFound(format!("script '{script_id}'")))?;
        (ws.worktree_dir.clone(), script.command.clone())
    };

    if !worktree.exists() {
        return Err(AppError::PathNotFound(worktree));
    }

    // Wrap the user's command in `sh -c` / `cmd /c` so they can chain
    // pipes and redirects naturally. Same shape Phase 1's agent path
    // uses. Inherit the parent env (PATH, HOME, etc.) so the script
    // can resolve binaries — portable-pty's CommandBuilder starts with
    // an empty env otherwise.
    let mut cmd = if cfg!(windows) {
        let mut c = CommandBuilder::new("cmd.exe");
        c.args(["/C", &command]);
        c.cwd(&worktree);
        c
    } else {
        let mut c = CommandBuilder::new("sh");
        c.args(["-c", &command]);
        c.cwd(&worktree);
        c
    };
    for (k, v) in std::env::vars() {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");

    let session = pty::spawn(cmd)?;
    run_pty_with_emit(Box::new(session), emit)
}

/// Test-friendly variant: take a pre-built PTY and run the reader +
/// watchdog threads against it. Production code path spawns a real
/// `PortablePty`; tests inject `MockPty`.
pub fn run_pty_with_emit<F>(pty: Box<dyn pty::Pty + Send>, mut emit: F) -> crate::error::Result<()>
where
    F: FnMut(TerminalChunk) + Send + 'static,
{
    let reader = pty.reader()?;
    let session: Arc<Mutex<Box<dyn pty::Pty + Send>>> = Arc::new(Mutex::new(pty));

    // Watchdog: poll `try_wait` and force-close the master PTY when the
    // child has exited. Without this the Windows ConPTY reader stays
    // blocked indefinitely after a clean child exit because EOF is not
    // always delivered. `child.kill()` alone does not close the master
    // on Windows — only dropping the master (`close_master()`) propagates
    // EOF to the reader so the loop below terminates and the Exited
    // chunk fires.
    let session_for_watchdog = Arc::clone(&session);
    let watchdog_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watchdog_done_clone = Arc::clone(&watchdog_done);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if watchdog_done_clone.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let exited = session_for_watchdog
            .lock()
            .ok()
            .and_then(|mut s| s.try_wait().ok().flatten())
            .is_some();
        if exited {
            if let Ok(mut s) = session_for_watchdog.lock() {
                let _ = s.kill();
                s.close_master();
            }
            return;
        }
    });

    // Spawn a thread that reads PTY stdout and forwards each chunk
    // through the emit closure. EOF triggers the Exited chunk with the
    // process's exit code.
    let session_for_reader = Arc::clone(&session);
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut reader, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    emit(TerminalChunk::Bytes {
                        bytes: buf[..n].to_vec(),
                    });
                }
                Err(_) => break,
            }
        }
        watchdog_done.store(true, std::sync::atomic::Ordering::SeqCst);
        let code = session_for_reader
            .lock()
            .ok()
            .and_then(|mut s| s.try_wait().ok().flatten())
            .and_then(|status| i32::try_from(status.exit_code()).ok());
        emit(TerminalChunk::Exited { code });
    });

    Ok(())
}

#[tauri::command]
pub async fn script_run(
    workspace_id: String,
    script_id: String,
    channel: Channel<TerminalChunk>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let inner_state = state.inner().clone();
    let emit = move |chunk: TerminalChunk| {
        let _ = channel.send(chunk);
    };
    script_run_inner(&workspace_id, &script_id, inner_state, emit).map_err(|e| {
        tracing::error!(error = %e, "script_run failed");
        e.to_string()
    })
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

    // ── script_run ──────────────────────────────────────────────────

    use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
    use std::time::Duration;

    fn make_state_with_repo_and_workspace(
        repo_id: &str,
        workspace_id: &str,
        worktree: &std::path::Path,
        scripts: Vec<RepoScript>,
    ) -> Arc<Mutex<AppState>> {
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
                scripts,
            },
        );
        state.workspaces.insert(
            workspace_id.into(),
            WorkspaceInfo {
                id: workspace_id.into(),
                repo_id: repo_id.into(),
                branch: "ansambel/t".into(),
                base_branch: "main".into(),
                custom_branch: false,
                title: "T".into(),
                description: String::new(),
                status: WorkspaceStatus::Waiting,
                column: KanbanColumn::InProgress,
                created_at: 0,
                updated_at: 0,
                worktree_dir: worktree.to_path_buf(),
            },
        );
        Arc::new(Mutex::new(state))
    }

    fn make_worktree() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        (tmp, wt)
    }

    fn collect_chunks_until_exit(
        rx: std::sync::mpsc::Receiver<TerminalChunk>,
        deadline: Duration,
    ) -> Vec<TerminalChunk> {
        let start = std::time::Instant::now();
        let mut out = Vec::new();
        while start.elapsed() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
                let is_exit = matches!(chunk, TerminalChunk::Exited { .. });
                out.push(chunk);
                if is_exit {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn run_pty_with_emit_streams_output_and_emits_exit_chunk() {
        // MockPty-driven: tests the reader + watchdog + emit pipeline
        // without spawning a real shell. Deterministic on all OS.
        use crate::platform::pty::MockPty;

        let (mock, handle) = MockPty::new(42);
        let (tx, rx) = std::sync::mpsc::channel::<TerminalChunk>();
        let emit = move |chunk: TerminalChunk| {
            let _ = tx.send(chunk);
        };
        run_pty_with_emit(Box::new(mock), emit).unwrap();

        // Push output then signal exit. Watchdog picks it up within
        // ~100ms and closes the master so the reader EOFs.
        handle.push_stdout(b"hello\r\n");
        handle.set_exited(0);

        let chunks = collect_chunks_until_exit(rx, Duration::from_secs(3));
        assert!(
            chunks.len() >= 2,
            "expected ≥2 chunks (bytes + exit), got {chunks:?}"
        );
        assert!(matches!(chunks.last(), Some(TerminalChunk::Exited { .. })));
        let combined: Vec<u8> = chunks
            .iter()
            .filter_map(|c| match c {
                TerminalChunk::Bytes { bytes } => Some(bytes.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let text = String::from_utf8_lossy(&combined);
        assert!(text.contains("hello"), "expected 'hello' in: {text:?}");
    }

    #[test]
    fn script_run_inner_resolves_workspace_and_script_then_spawns() {
        // Real-shell integration test — kept as the one happy-path
        // check that build_shell_command + portable-pty work end-to-end
        // on this host. State-machine assertions live in the MockPty
        // test above.
        let (_tmp, wt) = make_worktree();
        let scripts = vec![RepoScript {
            id: "sc_echo".into(),
            name: "Echo".into(),
            command: "echo hello".into(),
        }];
        let state = make_state_with_repo_and_workspace("repo_a", "ws_a", &wt, scripts);

        let (tx, rx) = std::sync::mpsc::channel::<TerminalChunk>();
        let emit = move |chunk: TerminalChunk| {
            let _ = tx.send(chunk);
        };
        script_run_inner("ws_a", "sc_echo", state, emit).unwrap();

        let chunks = collect_chunks_until_exit(rx, Duration::from_secs(5));
        assert!(
            chunks
                .iter()
                .any(|c| matches!(c, TerminalChunk::Exited { .. })),
            "expected an Exited chunk eventually, got: {chunks:?}"
        );
    }

    #[test]
    fn script_run_inner_returns_error_for_unknown_workspace() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = script_run_inner("ws_missing", "sc_x", state, |_| {}).unwrap_err();
        assert!(err.to_string().contains("ws_missing"));
    }

    #[test]
    fn script_run_inner_returns_error_for_unknown_script_id() {
        let (_tmp, wt) = make_worktree();
        let state = make_state_with_repo_and_workspace("repo_a", "ws_a", &wt, vec![]);
        let err = script_run_inner("ws_a", "sc_ghost", state, |_| {}).unwrap_err();
        assert!(err.to_string().contains("sc_ghost"));
    }

    #[test]
    fn script_run_inner_returns_error_when_worktree_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("nope");
        let scripts = vec![RepoScript {
            id: "sc_x".into(),
            name: "X".into(),
            command: "echo hi".into(),
        }];
        let state = make_state_with_repo_and_workspace("repo_a", "ws_a", &wt, scripts);
        let err = script_run_inner("ws_a", "sc_x", state, |_| {}).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("path") || msg.contains("not found"));
    }
}
