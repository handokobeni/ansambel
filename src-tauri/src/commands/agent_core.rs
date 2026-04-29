use crate::commands::agent_stream::parse_line;
use crate::commands::helpers::now_unix;
use crate::error::AppResult;
use crate::platform::pty::{spawn as pty_spawn, PtySession};
use crate::state::{AgentEvent, AgentHandle, AppState, WorkspaceStatus};
use portable_pty::CommandBuilder;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

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
        "--input-format",
        "stream-json",
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
            if writeln!(writer, "{line}").is_err() || writer.flush().is_err() {
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

/// Inner reader loop extracted for testability — does not require a Tauri Channel.
/// Reads NDJSON lines from `reader`, parses each into `AgentEvent`s, and calls
/// `send_event` for every event. On EOF, resets workspace status to `Waiting`
/// and removes the agent handle from state.
pub fn process_reader_events<F>(
    reader: Box<dyn std::io::Read + Send>,
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    send_event: &F,
) where
    F: Fn(AgentEvent),
{
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
                                if let Some(handle) = s.agents.get_mut(workspace_id) {
                                    handle.session_id = Some(session_id.clone());
                                }
                            }
                        }
                        send_event(ev);
                    }
                }
                Err(e) => {
                    send_event(AgentEvent::Error {
                        message: format!("parse: {e}"),
                    });
                }
            },
            Err(e) => {
                send_event(AgentEvent::Error {
                    message: format!("read: {e}"),
                });
                break;
            }
        }
    }
    // EOF cleanup — reset workspace status and drop the agent handle.
    if let Ok(mut s) = state.lock() {
        if let Some(ws) = s.workspaces.get_mut(workspace_id) {
            ws.status = WorkspaceStatus::Waiting;
        }
        s.agents.remove(workspace_id);
    }
}

pub fn send_message_inner(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    use crate::error::AppError;
    let s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let handle = s
        .agents
        .get(workspace_id)
        .ok_or_else(|| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("no agent for workspace {workspace_id}"),
        })?;
    let envelope = serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{ "type": "text", "text": text }],
        },
    })
    .to_string();
    handle
        .stdin_tx
        .send(envelope)
        .map_err(|e| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("stdin closed: {e}"),
        })?;
    Ok(())
}

pub fn stop_agent_inner(state: Arc<Mutex<AppState>>, workspace_id: &str) -> AppResult<()> {
    use crate::error::AppError;
    let mut s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    // Remove handle — drops stdin_tx sender, which closes the PTY writer thread.
    s.agents.remove(workspace_id);
    if let Some(ws) = s.workspaces.get_mut(workspace_id) {
        ws.status = WorkspaceStatus::Waiting;
    }
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
        let parsed: serde_json::Value = serde_json::from_str(&received).expect("valid NDJSON");
        assert_eq!(parsed["type"], "user");
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["content"][0]["type"], "text");
        assert_eq!(parsed["message"]["content"][0]["text"], "Hello!");
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

    #[test]
    fn stop_agent_inner_no_handle_returns_ok_silently() {
        let state = make_state();
        // Calling stop on a workspace with no agent is a no-op (idempotent).
        let result = stop_agent_inner(state, "ws_no_agent");
        assert!(result.is_ok());
    }

    #[test]
    fn stop_agent_inner_drops_stdin_tx() {
        use tokio::sync::mpsc;
        let state = make_state();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_stop_a".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_stop_a".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        stop_agent_inner(state.clone(), "ws_stop_a").unwrap();
        // After stop, the sender side is dropped, so try_recv returns Disconnected.
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn stop_agent_inner_removes_handle_from_map() {
        use tokio::sync::mpsc;
        let state = make_state();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_stop_b".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_stop_b".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        stop_agent_inner(state.clone(), "ws_stop_b").unwrap();
        assert!(!state.lock().unwrap().agents.contains_key("ws_stop_b"));
    }

    #[test]
    fn stop_agent_inner_marks_workspace_waiting() {
        use crate::state::WorkspaceStatus;
        use tokio::sync::mpsc;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_stop_c");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_stop_c", "repo_c", worktree);
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_stop_c")
            .unwrap()
            .status = WorkspaceStatus::Running;
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_stop_c".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_stop_c".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        stop_agent_inner(state.clone(), "ws_stop_c").unwrap();
        let s = state.lock().unwrap();
        assert_eq!(
            s.workspaces.get("ws_stop_c").unwrap().status,
            WorkspaceStatus::Waiting
        );
    }

    // ── process_reader_events tests ───────────────────────────────────────────

    #[test]
    fn process_reader_events_emits_error_on_bad_json() {
        use std::io::Cursor;
        let state = make_state();
        let reader: Box<dyn std::io::Read + Send> =
            Box::new(Cursor::new(b"not json {{{\n".to_vec()));
        let events =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<crate::state::AgentEvent>::new()));
        let events_clone = events.clone();
        process_reader_events(reader, state, "ws_reader_b", &|ev| {
            events_clone.lock().unwrap().push(ev);
        });
        let evs = events.lock().unwrap();
        assert_eq!(evs.len(), 1);
        assert!(matches!(&evs[0], crate::state::AgentEvent::Error { .. }));
    }

    #[test]
    fn process_reader_events_sets_workspace_waiting_on_eof() {
        use crate::state::WorkspaceStatus;
        use std::io::Cursor;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_reader_c");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_reader_c", "repo_c", worktree);
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_reader_c")
            .unwrap()
            .status = WorkspaceStatus::Running;
        // Empty reader = immediate EOF
        let reader: Box<dyn std::io::Read + Send> = Box::new(Cursor::new(b"".to_vec()));
        process_reader_events(reader, state.clone(), "ws_reader_c", &|_| {});
        assert_eq!(
            state
                .lock()
                .unwrap()
                .workspaces
                .get("ws_reader_c")
                .unwrap()
                .status,
            WorkspaceStatus::Waiting
        );
    }

    #[test]
    fn process_reader_events_removes_agent_handle_on_eof() {
        use std::io::Cursor;
        let state = make_state();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_reader_eof".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_reader_eof".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        let reader: Box<dyn std::io::Read + Send> = Box::new(Cursor::new(b"".to_vec()));
        process_reader_events(reader, state.clone(), "ws_reader_eof", &|_| {});
        assert!(!state.lock().unwrap().agents.contains_key("ws_reader_eof"));
    }

    #[test]
    fn process_reader_events_handles_init_sets_session_id() {
        use std::io::Cursor;
        let state = make_state();
        let line = "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"ses_test\",\"model\":\"claude-sonnet-4-6\",\"tools\":[],\"cwd\":\"/tmp\"}\n";
        let reader: Box<dyn std::io::Read + Send> = Box::new(Cursor::new(line.as_bytes().to_vec()));
        let events =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<crate::state::AgentEvent>::new()));
        let events_clone = events.clone();
        // Insert a dummy agent handle so session_id can be set.
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_reader_a".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_reader_a".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        process_reader_events(reader, state.clone(), "ws_reader_a", &|ev| {
            events_clone.lock().unwrap().push(ev);
        });
        let evs = events.lock().unwrap();
        assert!(!evs.is_empty());
        assert!(matches!(&evs[0], crate::state::AgentEvent::Init { .. }));
        // Handle removed on EOF.
        assert!(!state.lock().unwrap().agents.contains_key("ws_reader_a"));
    }

    #[test]
    fn process_reader_events_processes_multiple_lines() {
        use std::io::Cursor;
        let state = make_state();
        let ndjson = concat!(
            "{\"type\":\"assistant\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Hello\"}]}}\n",
            "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false}\n",
        );
        let reader: Box<dyn std::io::Read + Send> =
            Box::new(Cursor::new(ndjson.as_bytes().to_vec()));
        let events =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<crate::state::AgentEvent>::new()));
        let events_clone = events.clone();
        process_reader_events(reader, state, "ws_reader_d", &|ev| {
            events_clone.lock().unwrap().push(ev);
        });
        let evs = events.lock().unwrap();
        assert_eq!(evs.len(), 2);
        assert!(matches!(&evs[0], crate::state::AgentEvent::Message { .. }));
        assert!(matches!(&evs[1], crate::state::AgentEvent::Status { .. }));
    }

    #[test]
    fn process_reader_events_skips_empty_lines() {
        use std::io::Cursor;
        let state = make_state();
        // Empty line (whitespace-only) should produce zero events.
        let reader: Box<dyn std::io::Read + Send> = Box::new(Cursor::new(b"\n\n".to_vec()));
        let events =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<crate::state::AgentEvent>::new()));
        let events_clone = events.clone();
        process_reader_events(reader, state, "ws_reader_e", &|ev| {
            events_clone.lock().unwrap().push(ev);
        });
        let evs = events.lock().unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn spawn_agent_claude_binary_not_found_returns_err() {
        // Pass a path that doesn't exist so the binary-not-found branch is hit.
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_nobin");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_nobin", "repo_nobin", worktree);
        // Use a path that definitely doesn't exist.
        let bad_path = std::path::PathBuf::from("/tmp/definitely-does-not-exist-binary-xyz");
        let result = spawn_agent_inner(state, tmp.path(), "ws_nobin", Some(bad_path));
        // Should fail because the PTY can't spawn a non-existent binary.
        assert!(result.is_err());
    }

    #[test]
    fn build_system_prompt_prefix_includes_only_present_files() {
        let tmp = make_data_dir();
        let ctx_dir = tmp.path().join("contexts/repo_partial");
        std::fs::create_dir_all(&ctx_dir).unwrap();
        // Only write context.md; hot.md is absent.
        std::fs::write(ctx_dir.join("context.md"), "only context").unwrap();
        let prefix = build_system_prompt_prefix(tmp.path(), "repo_partial");
        assert!(prefix.contains("only context"));
        // No separator when only one file.
        assert!(!prefix.contains("---"));
    }

    #[test]
    fn send_message_inner_stdin_closed_returns_err() {
        // Drop the receiver so the send fails with "stdin closed".
        let state = make_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        drop(rx); // close receiver side
        state.lock().unwrap().agents.insert(
            "ws_closed".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_closed".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        let result = send_message_inner(state, "ws_closed", "hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("stdin closed"));
    }

    #[test]
    fn process_reader_events_emits_error_on_read_failure() {
        use std::io;

        /// A `Read` impl that always returns an IO error on the first call.
        struct FailingReader;
        impl io::Read for FailingReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "forced read error",
                ))
            }
        }

        let state = make_state();
        let reader: Box<dyn std::io::Read + Send> = Box::new(FailingReader);
        let events =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<crate::state::AgentEvent>::new()));
        let events_clone = events.clone();
        process_reader_events(reader, state, "ws_read_err", &|ev| {
            events_clone.lock().unwrap().push(ev);
        });
        let evs = events.lock().unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            crate::state::AgentEvent::Error { message } => {
                assert!(message.contains("read:"), "got: {message}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn spawn_agent_inner_appends_system_prompt_when_context_present() {
        use crate::state::WorkspaceStatus;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_ctx");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_ctx", "repo_ctx", worktree);
        // Write a context file so build_system_prompt_prefix returns non-empty.
        let ctx_dir = tmp.path().join("contexts/repo_ctx");
        std::fs::create_dir_all(&ctx_dir).unwrap();
        std::fs::write(ctx_dir.join("context.md"), "# context").unwrap();
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        let result = spawn_agent_inner(state.clone(), tmp.path(), "ws_ctx", Some(echo_path));
        assert!(result.is_ok(), "got err: {:?}", result.err());
        assert_eq!(
            state
                .lock()
                .unwrap()
                .workspaces
                .get("ws_ctx")
                .unwrap()
                .status,
            WorkspaceStatus::Running
        );
    }

    #[test]
    fn spawn_agent_inner_double_check_race_returns_err() {
        // Pre-insert the handle so the first TOCTOU check (lines 31-34) fires.
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_race");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_race", "repo_race", worktree);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_race".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_race".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        let result = spawn_agent_inner(state, tmp.path(), "ws_race", Some(echo_path));
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("already running"), "got: {err_msg}");
    }
}
