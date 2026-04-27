use crate::commands::agent_stream::parse_line;
use crate::commands::helpers::now_unix;
use crate::error::AppResult;
use crate::platform::pty::{spawn as pty_spawn, PtySession};
use crate::state::{AgentEvent, AgentHandle, AgentStatus, AppState, WorkspaceStatus};
use portable_pty::CommandBuilder;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::Manager;
use tokio::sync::mpsc;

#[tauri::command]
pub async fn spawn_agent(
    workspace_id: String,
    on_event: Channel<AgentEvent>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let claude_path = state
        .lock()
        .map_err(|e| format!("state lock poisoned: {e}"))?
        .settings
        .claude_binary_override
        .clone();
    let session = spawn_agent_inner(state.inner().clone(), &data_dir, &workspace_id, claude_path)
        .map_err(|e| e.to_string())?;
    spawn_reader_thread(session, on_event, state.inner().clone(), workspace_id);
    Ok(())
}

pub fn spawn_agent_inner(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    claude_path: Option<PathBuf>,
) -> AppResult<PtySession> {
    use crate::error::AppError;

    let (worktree_dir, repo_id) = {
        let s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = s
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace {workspace_id} not found")))?;
        (ws.worktree_dir.clone(), ws.repo_id.clone())
    }; // lock dropped here

    {
        let s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        if s.agents.contains_key(workspace_id) {
            return Err(AppError::Command {
                cmd: "spawn_agent".into(),
                msg: format!("agent already running for {workspace_id}"),
            });
        }
    } // lock dropped here

    let claude = claude_path
        .or_else(|| crate::platform::binary::claude_binary(None))
        .ok_or_else(|| AppError::Command {
            cmd: "spawn_agent".into(),
            msg: "claude binary not found".into(),
        })?;

    let mut cmd = CommandBuilder::new(&claude);
    cmd.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode",
        "bypassPermissions",
        "--disallowedTools",
        "EnterWorktree,ExitWorktree",
    ]);
    cmd.cwd(&worktree_dir);

    let prefix = build_system_prompt_prefix(data_dir, &repo_id);
    if !prefix.is_empty() {
        cmd.args(["--append-system-prompt", &prefix]);
    }

    let session = pty_spawn(cmd)?;

    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
    let mut writer = session.writer()?;
    std::thread::spawn(move || {
        use std::io::Write;
        while let Some(line) = stdin_rx.blocking_recv() {
            if writeln!(writer, "{line}").is_err() {
                break;
            }
        }
    });

    {
        let mut s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        // Double-check after spawn to close the race window.
        if s.agents.contains_key(workspace_id) {
            return Err(AppError::Command {
                cmd: "spawn_agent".into(),
                msg: format!("agent already running for {workspace_id}"),
            });
        }
        if let Some(ws) = s.workspaces.get_mut(workspace_id) {
            ws.status = WorkspaceStatus::Running;
        }
        s.agents.insert(
            workspace_id.into(),
            AgentHandle {
                workspace_id: workspace_id.into(),
                stdin_tx,
                session_id: None,
            },
        );
    } // lock dropped here

    Ok(session)
}

pub fn build_system_prompt_prefix(data_dir: &Path, repo_id: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let ctx_dir = data_dir.join("contexts").join(repo_id);
    for fname in ["context.md", "hot.md"] {
        let p = ctx_dir.join(fname);
        if let Ok(content) = std::fs::read_to_string(&p) {
            parts.push(content);
        }
    }
    parts.join("\n\n---\n\n")
}

fn spawn_reader_thread(
    mut session: PtySession,
    on_event: Channel<AgentEvent>,
    state: Arc<Mutex<AppState>>,
    workspace_id: String,
) {
    let _ = on_event.send(AgentEvent::Status {
        status: AgentStatus::Running,
    });
    std::thread::spawn(move || {
        let reader = match session.reader() {
            Ok(r) => r,
            Err(e) => {
                let _ = on_event.send(AgentEvent::Error {
                    message: format!("reader: {e}"),
                });
                return;
            }
        };
        let mut br = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match br.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => match parse_line(&line) {
                    Ok(events) => {
                        for ev in events {
                            if let AgentEvent::Init { session_id, .. } = &ev {
                                if let Ok(mut s) = state.lock() {
                                    if let Some(handle) = s.agents.get_mut(&workspace_id) {
                                        handle.session_id = Some(session_id.clone());
                                    }
                                }
                            }
                            let _ = on_event.send(ev);
                        }
                    }
                    Err(e) => {
                        let _ = on_event.send(AgentEvent::Error {
                            message: format!("parse: {e}"),
                        });
                    }
                },
                Err(e) => {
                    let _ = on_event.send(AgentEvent::Error {
                        message: format!("read: {e}"),
                    });
                    break;
                }
            }
        }
        let _ = session.try_wait();
        if let Ok(mut s) = state.lock() {
            if let Some(ws) = s.workspaces.get_mut(&workspace_id) {
                ws.status = WorkspaceStatus::Waiting;
            }
            s.agents.remove(&workspace_id);
        }
        let _ = on_event.send(AgentEvent::Status {
            status: AgentStatus::Stopped,
        });
    });
}

#[tauri::command]
pub async fn send_message(
    workspace_id: String,
    text: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    send_message_inner_with_persist(state.inner().clone(), &data_dir, &workspace_id, &text)
        .map_err(|e| e.to_string())
}

pub fn send_message_inner(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    use crate::error::AppError;
    let s = state.lock().unwrap();
    let handle = s
        .agents
        .get(workspace_id)
        .ok_or_else(|| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("no agent for workspace {workspace_id}"),
        })?;
    handle
        .stdin_tx
        .send(text.to_string())
        .map_err(|e| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("stdin closed: {e}"),
        })?;
    Ok(())
}

pub fn send_message_inner_with_persist(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    use crate::ids::message_id;
    use crate::persistence::messages::{load_messages, save_messages};
    use crate::state::{Message, MessageRole};

    send_message_inner(state, workspace_id, text)?;

    let mut current = load_messages(data_dir, workspace_id).unwrap_or_default();
    current.push(Message {
        id: message_id(),
        workspace_id: workspace_id.into(),
        role: MessageRole::User,
        text: text.into(),
        is_partial: false,
        tool_use: None,
        tool_result: None,
        created_at: now_unix(),
    });
    save_messages(data_dir, workspace_id, &current)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::default()))
    }

    fn make_data_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        crate::platform::paths::ensure_data_dirs(tmp.path()).unwrap();
        tmp
    }

    fn write_workspace(
        state: &Arc<Mutex<AppState>>,
        ws_id: &str,
        repo_id: &str,
        worktree: PathBuf,
    ) {
        use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
        let mut s = state.lock().unwrap();
        s.workspaces.insert(
            ws_id.into(),
            WorkspaceInfo {
                id: ws_id.into(),
                repo_id: repo_id.into(),
                title: "T".into(),
                description: String::new(),
                branch: "feat/x".into(),
                base_branch: "main".into(),
                custom_branch: false,
                status: WorkspaceStatus::NotStarted,
                column: KanbanColumn::InProgress,
                created_at: 0,
                updated_at: 0,
                worktree_dir: worktree,
            },
        );
    }

    #[test]
    fn spawn_agent_unknown_workspace_returns_err() {
        let state = make_state();
        let tmp = make_data_dir();
        let result = spawn_agent_inner(state.clone(), tmp.path(), "ws_missing", None);
        assert!(result.is_err());
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("not found") || msg.contains("missing"),
            "got: {msg}"
        );
    }

    #[test]
    fn spawn_agent_already_running_returns_err() {
        use tokio::sync::mpsc;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_a");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_a", "repo_a", worktree);
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_a".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_a".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        let result = spawn_agent_inner(state, tmp.path(), "ws_a", None);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("already running"));
    }

    #[test]
    fn spawn_agent_uses_explicit_claude_path_when_provided() {
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_b");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_b", "repo_b", worktree);
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        let result = spawn_agent_inner(state, tmp.path(), "ws_b", Some(echo_path));
        assert!(result.is_ok(), "got err: {:?}", result.err());
    }

    #[test]
    fn spawn_agent_marks_workspace_running_in_state() {
        use crate::state::WorkspaceStatus;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_c");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_c", "repo_c", worktree);
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        spawn_agent_inner(state.clone(), tmp.path(), "ws_c", Some(echo_path)).unwrap();
        let s = state.lock().unwrap();
        let ws = s.workspaces.get("ws_c").unwrap();
        assert_eq!(ws.status, WorkspaceStatus::Running);
    }

    #[test]
    fn spawn_agent_inserts_handle_into_agents_map() {
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_d");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_d", "repo_d", worktree);
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        spawn_agent_inner(state.clone(), tmp.path(), "ws_d", Some(echo_path)).unwrap();
        let s = state.lock().unwrap();
        assert!(s.agents.contains_key("ws_d"));
    }

    #[test]
    fn build_system_prompt_prefix_includes_context_when_present() {
        let tmp = make_data_dir();
        let ctx_dir = tmp.path().join("contexts/repo_x");
        std::fs::create_dir_all(&ctx_dir).unwrap();
        std::fs::write(ctx_dir.join("context.md"), "## Repo conventions\nUse Rust.").unwrap();
        std::fs::write(ctx_dir.join("hot.md"), "Recent: bug in login.").unwrap();
        let prefix = build_system_prompt_prefix(tmp.path(), "repo_x");
        assert!(prefix.contains("Repo conventions"));
        assert!(prefix.contains("Use Rust."));
        assert!(prefix.contains("Recent: bug in login."));
    }

    #[test]
    fn build_system_prompt_prefix_returns_empty_when_files_missing() {
        let tmp = make_data_dir();
        let prefix = build_system_prompt_prefix(tmp.path(), "repo_y");
        assert!(prefix.is_empty());
    }

    #[test]
    fn send_message_inner_sends_to_stdin_channel() {
        use tokio::sync::mpsc;
        let state = make_state();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_send_a".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_send_a".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        send_message_inner(state, "ws_send_a", "Hello!").unwrap();
        let received = rx.try_recv().unwrap();
        assert_eq!(received, "Hello!");
    }

    #[test]
    fn send_message_inner_no_agent_returns_err() {
        let state = make_state();
        let result = send_message_inner(state, "ws_none", "hi");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no agent"));
    }

    #[test]
    fn send_message_inner_appends_message_to_disk() {
        use crate::persistence::messages::load_messages;
        let tmp = make_data_dir();
        let state = make_state();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_send_b".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_send_b".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        send_message_inner_with_persist(state, tmp.path(), "ws_send_b", "Persist me").unwrap();
        let on_disk = load_messages(tmp.path(), "ws_send_b").unwrap();
        assert_eq!(on_disk.len(), 1);
        assert_eq!(on_disk[0].text, "Persist me");
        assert_eq!(on_disk[0].role, crate::state::MessageRole::User);
    }

    #[test]
    fn send_message_inner_persist_handles_existing_messages() {
        use crate::persistence::messages::{load_messages, save_messages};
        use crate::state::{Message, MessageRole};
        let tmp = make_data_dir();
        let state = make_state();
        save_messages(
            tmp.path(),
            "ws_send_c",
            &[Message {
                id: "msg_old".into(),
                workspace_id: "ws_send_c".into(),
                role: MessageRole::Assistant,
                text: "previous".into(),
                is_partial: false,
                tool_use: None,
                tool_result: None,
                created_at: 0,
            }],
        )
        .unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_send_c".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_send_c".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        send_message_inner_with_persist(state, tmp.path(), "ws_send_c", "next").unwrap();
        let on_disk = load_messages(tmp.path(), "ws_send_c").unwrap();
        assert_eq!(on_disk.len(), 2);
        assert_eq!(on_disk[0].text, "previous");
        assert_eq!(on_disk[1].text, "next");
    }
}
