use crate::commands::agent_stream::StreamParser;
use crate::commands::helpers::now_unix;
use crate::error::{AppError, AppResult};
use crate::state::{AgentEvent, AgentHandle, AppState, WorkspaceStatus};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Owns the Claude agent child process and its stdout pipe.
///
/// Claude's `--print --input-format stream-json` mode refuses TTY stdin and
/// requires plain pipes (the CLI prints
/// `Error: Input must be provided either through stdin or as a prompt argument`
/// when run under a PTY). We spawn via `std::process::Command` with
/// `Stdio::piped()` so the CLI accepts NDJSON on stdin.
pub struct AgentProcess {
    child: Child,
    stdout: Option<ChildStdout>,
}

impl AgentProcess {
    pub fn reader(&mut self) -> AppResult<Box<dyn std::io::Read + Send>> {
        let stdout = self.stdout.take().ok_or_else(|| AppError::Command {
            cmd: "spawn_agent".into(),
            msg: "stdout already taken".into(),
        })?;
        Ok(Box::new(stdout))
    }

    pub fn try_wait(&mut self) -> AppResult<()> {
        let _ = self.child.try_wait();
        Ok(())
    }
}

pub fn spawn_agent_inner(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    claude_path: Option<PathBuf>,
) -> AppResult<AgentProcess> {
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

    let mut cmd = Command::new(&claude);
    cmd.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        // Surface content_block_delta events so the chat UI can render
        // assistant text token-by-token instead of waiting for the whole
        // turn. The trailing non-partial assistant line still arrives and
        // overwrites the last partial via the message-id upsert.
        "--include-partial-messages",
        "--permission-mode",
        "bypassPermissions",
        "--disallowedTools",
        "EnterWorktree,ExitWorktree",
    ]);
    cmd.current_dir(&worktree_dir);

    let prefix = build_system_prompt_prefix(data_dir, &repo_id);
    if !prefix.is_empty() {
        cmd.args(["--append-system-prompt", &prefix]);
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| AppError::Command {
        cmd: "spawn_agent".into(),
        msg: format!("spawn claude: {e}"),
    })?;

    let stdin_pipe = child.stdin.take().ok_or_else(|| AppError::Command {
        cmd: "spawn_agent".into(),
        msg: "child stdin not piped".into(),
    })?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    // Construct the event broadcaster up-front so the stderr-pump thread
    // (spawned below) can forward CLI stderr lines as Error events. Buffer
    // of 256 absorbs partial-message bursts; slow consumers drop oldest
    // with `Lagged`, which is acceptable for a UI that re-renders on the
    // next message.
    let (event_tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(256);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Forward stderr to both `tracing::warn` (logs) and the broadcaster
    // (chat error banner). When Claude's CLI complains about auth, quota,
    // or network errors, the user sees the actual reason instead of just
    // "Stopped".
    if let Some(stderr) = stderr_pipe {
        let stderr_workspace_id = workspace_id.to_string();
        let stderr_tx = event_tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                tracing::warn!(
                    workspace_id = %stderr_workspace_id,
                    line = %line,
                    "agent stderr"
                );
                let _ = stderr_tx.send(stderr_line_to_event(&line));
            }
        });
    }

    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
    let mut writer = stdin_pipe;
    let writer_workspace_id = workspace_id.to_string();
    std::thread::spawn(move || {
        use std::io::Write;
        while let Some(line) = stdin_rx.blocking_recv() {
            tracing::debug!(workspace_id = %writer_workspace_id, line = %line, "agent writer: stdin");
            if writeln!(writer, "{line}").is_err() || writer.flush().is_err() {
                tracing::warn!(workspace_id = %writer_workspace_id, "agent writer: stdin write failed");
                break;
            }
        }
        tracing::info!(workspace_id = %writer_workspace_id, "agent writer: stdin channel closed");
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
                event_tx,
                cancel,
            },
        );
    } // lock dropped here

    Ok(AgentProcess {
        child,
        stdout: stdout_pipe,
    })
}

/// Maps a single line of agent CLI stderr to a chat-visible Error event.
/// The `CLI:` prefix tells users the message originated from the agent
/// process, not from Ansambel itself — useful for distinguishing auth
/// errors from app bugs.
pub fn stderr_line_to_event(line: &str) -> AgentEvent {
    AgentEvent::Error {
        message: format!("CLI: {}", line.trim_end()),
    }
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
/// `send_event` for every event. On EOF (or cancel), resets workspace status
/// to `Waiting` and removes the agent handle from state.
///
/// `cancel` is checked between reads. `stop_agent` flips it to `true` before
/// dropping the handle so the loop exits even if the child stdout doesn't
/// EOF promptly (defense-in-depth — closing stdin usually forces EOF).
pub fn process_reader_events_with_cancel<F>(
    reader: Box<dyn std::io::Read + Send>,
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    send_event: &F,
) where
    F: Fn(AgentEvent),
{
    use std::sync::atomic::Ordering;
    let mut br = BufReader::new(reader);
    let mut line = String::new();
    let mut parser = StreamParser::new();
    while !cancel.load(Ordering::Relaxed) {
        line.clear();
        match br.read_line(&mut line) {
            Ok(0) => {
                tracing::info!(workspace_id, "agent reader: EOF");
                break;
            }
            Ok(_) => {
                tracing::debug!(workspace_id, line = %line.trim_end(), "agent reader: line");
                match parser.parse_line(&line) {
                    Ok(events) => {
                        for ev in events {
                            tracing::debug!(workspace_id, event = ?ev, "agent reader: event");
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
                        tracing::warn!(
                            workspace_id,
                            error = %e,
                            line = %line.trim_end(),
                            "agent reader: parse failed"
                        );
                        send_event(AgentEvent::Error {
                            message: format!("parse: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(workspace_id, error = %e, "agent reader: read failed");
                send_event(AgentEvent::Error {
                    message: format!("read: {e}"),
                });
                break;
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        tracing::info!(workspace_id, "agent reader: cancelled");
    }
    // EOF / cancel cleanup — reset workspace status and drop the agent handle.
    // Use Arc::ptr_eq on the cancel token to verify the handle in state is
    // still ours: after a user-initiated Stop+respawn, a fresh handle (with a
    // different cancel Arc) may already occupy the slot, and blindly removing
    // would orphan the new agent and break the user's next send.
    if let Ok(mut s) = state.lock() {
        let still_ours = s
            .agents
            .get(workspace_id)
            .map(|h| std::sync::Arc::ptr_eq(&h.cancel, &cancel))
            .unwrap_or(false);
        if still_ours {
            if let Some(ws) = s.workspaces.get_mut(workspace_id) {
                ws.status = WorkspaceStatus::Waiting;
            }
            s.agents.remove(workspace_id);
        }
    }
}

/// Backwards-compatible thin wrapper for tests and callers that don't need
/// to drive cancellation themselves. Adopts the existing handle's cancel
/// Arc as the ownership token so EOF cleanup still removes the handle —
/// without that adoption, the ptr_eq check in
/// `process_reader_events_with_cancel` would always fail on a freshly-made
/// "never-fire" Arc and leave the handle stranded in state.
pub fn process_reader_events<F>(
    reader: Box<dyn std::io::Read + Send>,
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    send_event: &F,
) where
    F: Fn(AgentEvent),
{
    let cancel = state
        .lock()
        .ok()
        .and_then(|s| s.agents.get(workspace_id).map(|h| h.cancel.clone()))
        .unwrap_or_else(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    process_reader_events_with_cancel(reader, state, workspace_id, cancel, send_event)
}

pub fn send_message_inner(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    send_message_inner_with_attachments(state, workspace_id, text, &[])
}

/// Send a user message to the agent with optional attached image files.
/// Each Attachment must already exist on disk at its `path`; this function
/// reads + base64-encodes the bytes inline and pushes them as `image`
/// content blocks BEFORE the trailing text block, matching how the
/// Anthropic API wants multimodal turns ordered.
pub fn send_message_inner_with_attachments(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    text: &str,
    attachments: &[crate::state::Attachment],
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
    // session_id is required by claude's stream-json input parser. The
    // CLI's authoritative session_id arrives in the init event and is
    // stored on AgentHandle; before that point we fall back to the
    // workspace_id (any string is accepted for a fresh session).
    let session_id = handle
        .session_id
        .clone()
        .unwrap_or_else(|| workspace_id.to_string());

    let mut content: Vec<serde_json::Value> = Vec::with_capacity(attachments.len() + 1);
    for att in attachments {
        let bytes = std::fs::read(&att.path).map_err(|e| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("read attachment {}: {e}", att.path),
        })?;
        let encoded = base64_encode(&bytes);
        content.push(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": att.media_type,
                "data": encoded,
            }
        }));
    }
    // Text always comes last — ordering matters for Claude's multimodal
    // attention, the prompt should reference the images preceding it.
    content.push(serde_json::json!({ "type": "text", "text": text }));

    let envelope = serde_json::json!({
        "type": "user",
        "session_id": session_id,
        "message": {
            "role": "user",
            "content": content,
        },
        "parent_tool_use_id": serde_json::Value::Null,
    })
    .to_string();
    handle
        .stdin_tx
        .send(envelope)
        .map_err(|e| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("stdin closed: {e}"),
        })?;
    // Mark the turn as in-flight. The CLI streams content blocks back over
    // many milliseconds before the `result` line arrives — Status::Running
    // here lets the UI's TurnStatusBar appear immediately on send rather
    // than only once the first assistant token shows up. The matching
    // Waiting transition is emitted by the parser on `result`.
    let _ = handle.event_tx.send(crate::state::AgentEvent::Status {
        status: crate::state::AgentStatus::Running,
    });
    Ok(())
}

/// Minimal base64 encoder for image bytes — keeps us off the `base64`
/// crate which would be a fresh dependency for this single use case.
/// Standard alphabet, padded.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let b = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        out.push(ALPHA[((b >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 6) & 0x3F) as usize] as char);
        out.push(ALPHA[(b & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let b = (rem[0] as u32) << 16;
            out.push(ALPHA[((b >> 18) & 0x3F) as usize] as char);
            out.push(ALPHA[((b >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let b = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(ALPHA[((b >> 18) & 0x3F) as usize] as char);
            out.push(ALPHA[((b >> 12) & 0x3F) as usize] as char);
            out.push(ALPHA[((b >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Converts a streaming `AgentEvent` to a persistable `Message`, returning
/// `None` for events that should not be saved (init, status, error, partial
/// message chunks). Tool events are persisted as separate `Tool`-role
/// messages so the on-disk shape mirrors what the frontend store renders.
pub fn event_to_persisted_message(
    event: &AgentEvent,
    workspace_id: &str,
) -> Option<crate::state::Message> {
    use crate::state::{Message, MessageRole};
    let now = now_unix();
    match event {
        AgentEvent::Message {
            id,
            role,
            text,
            is_partial,
        } => {
            if *is_partial {
                return None;
            }
            Some(Message {
                id: id.clone(),
                workspace_id: workspace_id.into(),
                role: role.clone(),
                text: text.clone(),
                is_partial: false,
                tool_use: None,
                tool_result: None,
                created_at: now,
                attachments: Vec::new(),
            })
        }
        AgentEvent::ToolUse {
            message_id,
            tool_use,
        } => Some(Message {
            id: format!("{message_id}/tool_use/{}", tool_use.id),
            workspace_id: workspace_id.into(),
            role: MessageRole::Tool,
            text: String::new(),
            is_partial: false,
            tool_use: Some(tool_use.clone()),
            tool_result: None,
            created_at: now,
            attachments: Vec::new(),
        }),
        AgentEvent::ToolResult {
            message_id,
            tool_result,
        } => Some(Message {
            id: format!("{message_id}/tool_result/{}", tool_result.tool_use_id),
            workspace_id: workspace_id.into(),
            role: MessageRole::Tool,
            text: String::new(),
            is_partial: false,
            tool_use: None,
            tool_result: Some(tool_result.clone()),
            created_at: now,
            attachments: Vec::new(),
        }),
        AgentEvent::Init { .. }
        | AgentEvent::Status { .. }
        | AgentEvent::Error { .. }
        | AgentEvent::Compact { .. }
        | AgentEvent::Thinking { .. }
        | AgentEvent::Usage { .. } => None,
    }
}

pub fn stop_agent_inner(state: Arc<Mutex<AppState>>, workspace_id: &str) -> AppResult<()> {
    use crate::error::AppError;
    let mut s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    // Flip the cancel token *before* removing the handle so the reader
    // thread observes it on its next read_line check and exits cleanly,
    // even if the child stdout doesn't EOF promptly.
    if let Some(handle) = s.agents.get(workspace_id) {
        handle
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // Remove handle — drops stdin_tx sender, which closes the child's
    // stdin and (usually) forces EOF on stdout.
    s.agents.remove(workspace_id);
    if let Some(ws) = s.workspaces.get_mut(workspace_id) {
        ws.status = WorkspaceStatus::Waiting;
    }
    Ok(())
}

/// Subscribes to a running agent's event broadcaster. Returns an error if no
/// agent is registered for the workspace. Called when the user navigates back
/// to a workspace that's still running and we need a fresh receiver to pump
/// events to a new Tauri Channel — the original Channel handler is GC'd on
/// component unmount, so without re-subscribing the UI would stall.
pub fn reattach_agent_inner(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
) -> AppResult<tokio::sync::broadcast::Receiver<AgentEvent>> {
    let s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let handle = s
        .agents
        .get(workspace_id)
        .ok_or_else(|| AppError::Command {
            cmd: "reattach_agent".into(),
            msg: format!("no agent for workspace {workspace_id}"),
        })?;
    Ok(handle.event_tx.subscribe())
}

pub fn send_message_inner_with_persist(
    state: Arc<Mutex<AppState>>,
    message_writer: &crate::persistence::message_writer::MessageWriter,
    data_dir: &Path,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    send_message_inner_with_persist_and_attachments(
        state,
        message_writer,
        data_dir,
        workspace_id,
        text,
        &[],
    )
}

/// Like `send_message_inner_with_persist` but also handles file attachments:
/// each `AttachmentInput` (a path the user picked + media_type) is copied
/// into `<data_dir>/attachments/<ws>/<msg>/` so the chat survives the user
/// later moving or deleting the source file, then base64-encoded into the
/// CLI envelope and stamped onto the persisted user `Message`.
pub fn send_message_inner_with_persist_and_attachments(
    state: Arc<Mutex<AppState>>,
    message_writer: &crate::persistence::message_writer::MessageWriter,
    data_dir: &Path,
    workspace_id: &str,
    text: &str,
    attachments: &[crate::commands::agent::AttachmentInput],
) -> AppResult<()> {
    use crate::error::AppError;
    use crate::ids::message_id;
    use crate::state::{Attachment, AttachmentKind, Message, MessageRole};

    let user_id = message_id();

    // Copy each attachment into a dedicated directory for this message so
    // the persisted `Message` references stable paths under the app data
    // dir rather than the user's downloads folder.
    let mut copied: Vec<Attachment> = Vec::with_capacity(attachments.len());
    if !attachments.is_empty() {
        let dest_dir = data_dir
            .join("attachments")
            .join(workspace_id)
            .join(&user_id);
        std::fs::create_dir_all(&dest_dir).map_err(|e| AppError::Command {
            cmd: "send_message".into(),
            msg: format!("create attachments dir {}: {e}", dest_dir.display()),
        })?;
        for input in attachments {
            if !input.media_type.starts_with("image/") {
                return Err(AppError::Command {
                    cmd: "send_message".into(),
                    msg: format!("unsupported media_type {:?}", input.media_type),
                });
            }
            let source = std::path::Path::new(&input.source_path);
            let basename = source
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .or_else(|| input.filename.clone())
                .unwrap_or_else(|| format!("attachment-{}", copied.len()));
            let dest = dest_dir.join(&basename);
            std::fs::copy(source, &dest).map_err(|e| AppError::Command {
                cmd: "send_message".into(),
                msg: format!("copy attachment {}: {e}", input.source_path),
            })?;
            copied.push(Attachment {
                kind: AttachmentKind::Image,
                media_type: input.media_type.clone(),
                path: dest.to_string_lossy().into_owned(),
                filename: input.filename.clone().or(Some(basename)),
            });
        }
    }

    // Send to the CLI using the *copied* paths so a successful send proves
    // the files are in their final home.
    send_message_inner_with_attachments(state, workspace_id, text, &copied)?;

    let user_msg = Message {
        id: user_id,
        workspace_id: workspace_id.into(),
        role: MessageRole::User,
        text: text.into(),
        is_partial: false,
        tool_use: None,
        tool_result: None,
        created_at: now_unix(),
        attachments: copied,
    };
    message_writer.queue(data_dir, workspace_id, user_msg)
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        send_message_inner(state, "ws_send_a", "Hello!").unwrap();
        let received = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&received).expect("valid NDJSON");
        assert_eq!(parsed["type"], "user");
        // session_id is required by the claude CLI; falls back to workspace_id
        // until the init event populates AgentHandle.session_id.
        assert_eq!(parsed["session_id"], "ws_send_a");
        assert_eq!(parsed["parent_tool_use_id"], serde_json::Value::Null);
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["content"][0]["type"], "text");
        assert_eq!(parsed["message"]["content"][0]["text"], "Hello!");
    }

    #[test]
    fn send_message_inner_uses_captured_session_id_after_init() {
        use tokio::sync::mpsc;
        let state = make_state();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_session".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_session".into(),
                stdin_tx: tx,
                session_id: Some("ses_authoritative".into()),
                event_tx: tokio::sync::broadcast::channel::<AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        send_message_inner(state, "ws_session", "hi").unwrap();
        let received = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();
        assert_eq!(parsed["session_id"], "ses_authoritative");
    }

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"hello world"), "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn send_message_with_attachments_builds_image_blocks_before_text() {
        use tokio::sync::mpsc;
        let tmp = make_data_dir();
        // Tiny "image" — content doesn't have to be valid PNG; the encoder
        // just round-trips bytes.
        let img_path = tmp.path().join("smol.png");
        std::fs::write(&img_path, b"\x89PNG\x0d\x0a\x1a\x0a fake bytes").unwrap();

        let state = make_state();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_att".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_att".into(),
                stdin_tx: tx,
                session_id: Some("ses_x".into()),
                event_tx: tokio::sync::broadcast::channel::<AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );

        let attachments = [crate::state::Attachment {
            kind: crate::state::AttachmentKind::Image,
            media_type: "image/png".into(),
            path: img_path.to_string_lossy().into_owned(),
            filename: Some("smol.png".into()),
        }];
        send_message_inner_with_attachments(state, "ws_att", "look", &attachments).unwrap();

        let received = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&received).expect("valid NDJSON");
        let content = parsed["message"]["content"].as_array().unwrap();
        // Image must come BEFORE the text block.
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert!(!content[0]["source"]["data"].as_str().unwrap().is_empty());
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "look");
    }

    #[test]
    fn send_message_with_attachments_returns_err_when_file_missing() {
        use tokio::sync::mpsc;
        let state = make_state();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_missing_att".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_missing_att".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let bogus = [crate::state::Attachment {
            kind: crate::state::AttachmentKind::Image,
            media_type: "image/png".into(),
            path: "/nonexistent/foo.png".into(),
            filename: None,
        }];
        let result = send_message_inner_with_attachments(state, "ws_missing_att", "x", &bogus);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read attachment"));
    }

    #[tokio::test]
    async fn send_message_with_persist_copies_attachment_into_data_dir() {
        use crate::persistence::message_writer::MessageWriter;
        use crate::persistence::messages::load_messages;
        let tmp = make_data_dir();
        // Source file lives outside the app data dir to mirror the real
        // case where the user picks something from ~/Downloads.
        let outside = TempDir::new().unwrap();
        let src = outside.path().join("design.png");
        std::fs::write(&src, b"\x89PNGsmol").unwrap();

        let state = make_state();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_copy".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_copy".into(),
                stdin_tx: tx,
                session_id: Some("ses_y".into()),
                event_tx: tokio::sync::broadcast::channel::<AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let writer = MessageWriter::new(std::time::Duration::from_millis(20));
        let inputs = [crate::commands::agent::AttachmentInput {
            source_path: src.to_string_lossy().into_owned(),
            media_type: "image/png".into(),
            filename: Some("design.png".into()),
        }];
        send_message_inner_with_persist_and_attachments(
            state,
            &writer,
            tmp.path(),
            "ws_copy",
            "see this",
            &inputs,
        )
        .unwrap();
        writer.flush_all().await;

        // Source file untouched.
        assert!(src.exists(), "source file must not be moved");

        // Persisted Message references a path inside the data dir, and the
        // file actually lives there now.
        let on_disk = load_messages(tmp.path(), "ws_copy").unwrap();
        assert_eq!(on_disk.len(), 1);
        assert_eq!(on_disk[0].attachments.len(), 1);
        let att = &on_disk[0].attachments[0];
        assert_eq!(att.media_type, "image/png");
        assert!(
            att.path.starts_with(tmp.path().to_string_lossy().as_ref()),
            "attachment path {} should live under data dir {}",
            att.path,
            tmp.path().display()
        );
        assert!(std::path::Path::new(&att.path).exists());
    }

    #[tokio::test]
    async fn send_message_with_persist_rejects_non_image_media_types() {
        use tokio::sync::mpsc;
        let tmp = make_data_dir();
        let outside = TempDir::new().unwrap();
        let src = outside.path().join("note.txt");
        std::fs::write(&src, b"hello").unwrap();

        let state = make_state();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_reject".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_reject".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let writer = crate::persistence::message_writer::MessageWriter::new(
            std::time::Duration::from_millis(20),
        );
        let inputs = [crate::commands::agent::AttachmentInput {
            source_path: src.to_string_lossy().into_owned(),
            media_type: "text/plain".into(),
            filename: None,
        }];
        let result = send_message_inner_with_persist_and_attachments(
            state,
            &writer,
            tmp.path(),
            "ws_reject",
            "x",
            &inputs,
        );
        assert!(result.is_err(), "non-image media_type must be rejected");
    }

    #[test]
    fn send_message_inner_no_agent_returns_err() {
        let state = make_state();
        let result = send_message_inner(state, "ws_none", "hi");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no agent"));
    }

    #[tokio::test]
    async fn send_message_inner_appends_message_to_disk() {
        use crate::persistence::message_writer::MessageWriter;
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let writer = MessageWriter::new(std::time::Duration::from_millis(50));
        send_message_inner_with_persist(state, &writer, tmp.path(), "ws_send_b", "Persist me")
            .unwrap();
        writer.flush_all().await;
        let on_disk = load_messages(tmp.path(), "ws_send_b").unwrap();
        assert_eq!(on_disk.len(), 1);
        assert_eq!(on_disk[0].text, "Persist me");
        assert_eq!(on_disk[0].role, crate::state::MessageRole::User);
    }

    #[tokio::test]
    async fn send_message_inner_persist_handles_existing_messages() {
        use crate::persistence::message_writer::MessageWriter;
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
                attachments: Vec::new(),
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let writer = MessageWriter::new(std::time::Duration::from_millis(50));
        send_message_inner_with_persist(state, &writer, tmp.path(), "ws_send_c", "next").unwrap();
        writer.flush_all().await;
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        // Insert an agent handle so the EOF cleanup recognises this run as
        // its own and flips the workspace status (the post-Stop+respawn
        // race protection now requires Arc::ptr_eq on the cancel token).
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_reader_c".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_reader_c".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(8).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        // The assistant line produces a Message; the "result" line now
        // emits a trailing Status::Waiting so the live turn indicator can
        // close out cleanly when the turn finishes.
        assert_eq!(evs.len(), 2);
        assert!(matches!(&evs[0], crate::state::AgentEvent::Message { .. }));
        match &evs[1] {
            crate::state::AgentEvent::Status { status } => {
                assert_eq!(*status, crate::state::AgentStatus::Waiting);
            }
            other => panic!("expected Status::Waiting trailer, got {other:?}"),
        }
    }

    #[test]
    fn process_reader_events_streams_partial_then_final_assistant_message() {
        use std::io::Cursor;
        let state = make_state();
        // Realistic --include-partial-messages stream: message_start →
        // two text deltas → message_stop → final assistant message.
        let ndjson = concat!(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_s","role":"assistant","content":[]}}}"#,
            "\n",
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}}"#,
            "\n",
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}}"#,
            "\n",
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"id":"msg_s","role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#,
            "\n",
        );
        let reader: Box<dyn std::io::Read + Send> =
            Box::new(Cursor::new(ndjson.as_bytes().to_vec()));
        let events =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<crate::state::AgentEvent>::new()));
        let events_clone = events.clone();
        process_reader_events(reader, state, "ws_stream", &|ev| {
            events_clone.lock().unwrap().push(ev);
        });
        let evs = events.lock().unwrap();
        // Two partials + one final = three Message events with id "msg_s".
        assert_eq!(evs.len(), 3);
        let texts: Vec<(&str, bool)> = evs
            .iter()
            .map(|e| match e {
                crate::state::AgentEvent::Message {
                    text, is_partial, ..
                } => (text.as_str(), *is_partial),
                _ => panic!("expected Message events only"),
            })
            .collect();
        assert_eq!(texts[0], ("Hel", true));
        assert_eq!(texts[1], ("Hello", true));
        assert_eq!(texts[2], ("Hello", false));
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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

    // ── event_to_persisted_message tests ──────────────────────────────────────
    #[test]
    fn event_to_persisted_message_converts_assistant_text() {
        use crate::state::MessageRole;
        let ev = AgentEvent::Message {
            id: "msg_1".into(),
            role: MessageRole::Assistant,
            text: "Hello!".into(),
            is_partial: false,
        };
        let msg = event_to_persisted_message(&ev, "ws_a").expect("should persist");
        assert_eq!(msg.id, "msg_1");
        assert_eq!(msg.workspace_id, "ws_a");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.text, "Hello!");
        assert!(!msg.is_partial);
    }

    #[test]
    fn event_to_persisted_message_skips_partial_messages() {
        use crate::state::MessageRole;
        let ev = AgentEvent::Message {
            id: "msg_2".into(),
            role: MessageRole::Assistant,
            text: "streaming...".into(),
            is_partial: true,
        };
        assert!(event_to_persisted_message(&ev, "ws_a").is_none());
    }

    #[test]
    fn event_to_persisted_message_converts_tool_use() {
        use crate::state::{MessageRole, ToolUse};
        let tu = ToolUse {
            id: "toolu_1".into(),
            name: "Read".into(),
            input: serde_json::json!({"path": "/etc/hosts"}),
        };
        let ev = AgentEvent::ToolUse {
            message_id: "msg_3".into(),
            tool_use: tu.clone(),
        };
        let msg = event_to_persisted_message(&ev, "ws_b").expect("should persist");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_use, Some(tu));
        assert!(msg.tool_result.is_none());
    }

    #[test]
    fn event_to_persisted_message_converts_tool_result() {
        use crate::state::{MessageRole, ToolResult};
        let tr = ToolResult {
            tool_use_id: "toolu_1".into(),
            content: "127.0.0.1 localhost".into(),
            is_error: false,
        };
        let ev = AgentEvent::ToolResult {
            message_id: "msg_4".into(),
            tool_result: tr.clone(),
        };
        let msg = event_to_persisted_message(&ev, "ws_c").expect("should persist");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_result, Some(tr));
        assert!(msg.tool_use.is_none());
    }

    #[test]
    fn event_to_persisted_message_skips_init_status_error() {
        let init = AgentEvent::Init {
            session_id: "ses".into(),
            model: "claude-sonnet-4-6".into(),
        };
        let status = AgentEvent::Status {
            status: crate::state::AgentStatus::Running,
        };
        let err = AgentEvent::Error {
            message: "boom".into(),
        };
        assert!(event_to_persisted_message(&init, "ws").is_none());
        assert!(event_to_persisted_message(&status, "ws").is_none());
        assert!(event_to_persisted_message(&err, "ws").is_none());
    }

    // ── reattach_agent_inner tests ────────────────────────────────────────────
    #[test]
    fn reattach_agent_inner_returns_err_when_no_agent() {
        let state = make_state();
        let result = reattach_agent_inner(state, "ws_missing");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no agent"));
    }

    #[test]
    fn reattach_agent_inner_returns_subscriber_when_agent_running() {
        let state = make_state();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (event_tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(64);
        state.lock().unwrap().agents.insert(
            "ws_re".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_re".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let result = reattach_agent_inner(state, "ws_re");
        assert!(result.is_ok());
    }

    #[test]
    fn reattach_subscriber_receives_events_emitted_after_subscription() {
        let state = make_state();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (event_tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(64);
        let event_tx_for_handle = event_tx.clone();
        state.lock().unwrap().agents.insert(
            "ws_sub".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_sub".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: event_tx_for_handle,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        let mut sub = reattach_agent_inner(state, "ws_sub").unwrap();
        event_tx
            .send(AgentEvent::Status {
                status: crate::state::AgentStatus::Running,
            })
            .unwrap();
        let received = sub.try_recv().unwrap();
        assert!(matches!(received, AgentEvent::Status { .. }));
    }

    #[test]
    fn stderr_line_to_event_wraps_with_cli_prefix() {
        let ev = stderr_line_to_event("invalid_request_error: bad token");
        match ev {
            AgentEvent::Error { message } => {
                assert!(message.starts_with("CLI: "));
                assert!(message.contains("invalid_request_error"));
                assert!(message.contains("bad token"));
            }
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn stderr_line_to_event_trims_trailing_newline() {
        let ev = stderr_line_to_event("oops\n");
        match ev {
            AgentEvent::Error { message } => {
                assert_eq!(message, "CLI: oops");
                assert!(!message.ends_with('\n'));
            }
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn stderr_line_to_event_handles_blank_lines() {
        let ev = stderr_line_to_event("");
        match ev {
            AgentEvent::Error { message } => assert_eq!(message, "CLI: "),
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn process_reader_events_exits_when_cancel_token_set() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Arc;

        // Reader that yields a short init line on every read with a
        // 10 ms sleep, capped at 100 reads. The cancel must cut the
        // loop short well before the cap so we observe < cap events.
        struct CountingReader {
            count: Arc<AtomicUsize>,
            cap: usize,
        }
        impl std::io::Read for CountingReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                std::thread::sleep(std::time::Duration::from_millis(10));
                let n = self.count.fetch_add(1, Ordering::Relaxed);
                if n >= self.cap {
                    return Ok(0);
                }
                let line = b"{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s\",\"model\":\"m\"}\n";
                let len = line.len().min(buf.len());
                buf[..len].copy_from_slice(&line[..len]);
                Ok(len)
            }
        }

        let cap = 200usize; // Without cancel, the test would take ~2s.
        let read_count = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let state = make_state();
        write_workspace(
            &state,
            "ws_cx",
            "repo_x",
            std::path::PathBuf::from("/tmp/x"),
        );
        // Insert an agent handle whose cancel Arc *is* the same one we'll
        // pass to process_reader_events_with_cancel — that's what the
        // ptr_eq ownership check uses to authorize EOF cleanup.
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_cx".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_cx".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(8).0,
                cancel: cancel.clone(),
            },
        );
        let cancel_for_thread = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            cancel_for_thread.store(true, Ordering::Relaxed);
        });

        let event_count = Arc::new(AtomicUsize::new(0));
        let event_count_for_cb = event_count.clone();
        let start = std::time::Instant::now();
        process_reader_events_with_cancel(
            Box::new(CountingReader {
                count: read_count.clone(),
                cap,
            }),
            state.clone(),
            "ws_cx",
            cancel.clone(),
            &|_| {
                event_count_for_cb.fetch_add(1, Ordering::Relaxed);
            },
        );

        // The loop must exit well before the natural EOF. With a 10 ms
        // per-read latency, ~5 reads should land inside the 50 ms cancel
        // window — generously cap the upper bound at half the reader cap.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "cancel did not abort reader: elapsed={:?}",
            start.elapsed()
        );
        assert!(cancel.load(Ordering::Relaxed));
        let observed = event_count.load(Ordering::Relaxed);
        assert!(
            observed < cap,
            "cancel must cut loop short: observed {observed} events out of cap {cap}"
        );
        // EOF/cancel cleanup must still run — workspace flips to Waiting.
        let s = state.lock().unwrap();
        assert_eq!(
            s.workspaces.get("ws_cx").unwrap().status,
            crate::state::WorkspaceStatus::Waiting
        );
    }

    #[test]
    fn reader_cleanup_does_not_remove_handle_after_respawn() {
        // After a Stop+respawn race, the OLD reader thread's cleanup must
        // not blow away the NEW agent handle that has just been inserted by
        // spawn_agent_inner. Each handle carries its own cancel Arc; the
        // cleanup verifies ownership via Arc::ptr_eq.
        use std::sync::atomic::Ordering;
        use tokio::sync::mpsc;
        let state = make_state();
        write_workspace(
            &state,
            "ws_resp",
            "repo_resp",
            PathBuf::from("/tmp/ws_resp"),
        );

        // Pretend we are the *old* reader thread: hold a cancel Arc that
        // matches a handle that was already removed by stop_agent.
        let old_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Now simulate respawn: insert a fresh handle with a *different*
        // cancel Arc.
        let (new_tx, _new_rx) = mpsc::unbounded_channel::<String>();
        let new_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        state.lock().unwrap().agents.insert(
            "ws_resp".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_resp".into(),
                stdin_tx: new_tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(8).0,
                cancel: new_cancel.clone(),
            },
        );
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_resp")
            .unwrap()
            .status = crate::state::WorkspaceStatus::Running;

        // Run the old reader's tail-end cleanup with an empty reader (immediate
        // EOF). The cleanup must NOT remove the new handle.
        let empty: &[u8] = &[];
        process_reader_events_with_cancel(
            Box::new(empty),
            state.clone(),
            "ws_resp",
            old_cancel,
            &|_| {},
        );

        let s = state.lock().unwrap();
        assert!(
            s.agents.contains_key("ws_resp"),
            "old reader cleanup must not remove the freshly-respawned handle"
        );
        assert!(
            std::sync::Arc::ptr_eq(&s.agents.get("ws_resp").unwrap().cancel, &new_cancel),
            "the surviving handle must be the new one, not a leftover from the old reader"
        );
        assert_eq!(
            s.workspaces.get("ws_resp").unwrap().status,
            crate::state::WorkspaceStatus::Running,
            "the new spawn's Running status must not be reset to Waiting by the old reader"
        );
        // sanity: cancel state untouched
        assert!(!new_cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn reader_cleanup_removes_handle_when_still_ours() {
        // The "process died on its own" path: same cancel Arc still owns the
        // handle in state, so cleanup proceeds (drop handle, flip workspace
        // to Waiting). This guards against accidentally widening the ptr_eq
        // check into a no-op for the EOF case.
        use tokio::sync::mpsc;
        let state = make_state();
        write_workspace(&state, "ws_eof", "repo_eof", PathBuf::from("/tmp/ws_eof"));
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        state.lock().unwrap().agents.insert(
            "ws_eof".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_eof".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(8).0,
                cancel: cancel.clone(),
            },
        );
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_eof")
            .unwrap()
            .status = crate::state::WorkspaceStatus::Running;

        let empty: &[u8] = &[];
        process_reader_events_with_cancel(
            Box::new(empty),
            state.clone(),
            "ws_eof",
            cancel,
            &|_| {},
        );

        let s = state.lock().unwrap();
        assert!(
            !s.agents.contains_key("ws_eof"),
            "EOF cleanup must drop the owning handle"
        );
        assert_eq!(
            s.workspaces.get("ws_eof").unwrap().status,
            crate::state::WorkspaceStatus::Waiting
        );
    }

    #[test]
    fn agent_process_reader_returns_stdout_pipe_then_errors_on_repeat() {
        // AgentProcess::reader takes ownership of the stdout pipe so the
        // reader thread can stream lines without holding the Child. A second
        // call after the take must return AppError::Command rather than
        // panicking on an Option::take of None.
        let mut child = Command::new(if cfg!(windows) { "cmd" } else { "sh" })
            .args::<&[&str], _>(if cfg!(windows) {
                &["/C", "echo hi"]
            } else {
                &["-c", "echo hi"]
            })
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn helper for AgentProcess test");
        let stdout = child.stdout.take();
        let mut proc = AgentProcess { child, stdout };
        // First call hands out the stdout reader.
        let reader = proc.reader();
        assert!(reader.is_ok(), "first reader() must succeed");
        // Drain so the child can exit cleanly.
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut reader.unwrap(), &mut buf);
        // Second call must surface a structured error, not a panic.
        let again = proc.reader();
        assert!(again.is_err(), "second reader() must error after take");
        assert!(
            proc.try_wait().is_ok(),
            "try_wait must surface Ok even after exit"
        );
    }

    #[test]
    fn stop_agent_inner_flips_cancel_token() {
        use std::sync::atomic::Ordering;
        use tokio::sync::mpsc;
        let state = make_state();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        state.lock().unwrap().agents.insert(
            "ws_cancel".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_cancel".into(),
                stdin_tx: tx,
                session_id: None,
                event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
                cancel: cancel.clone(),
            },
        );
        assert!(!cancel.load(Ordering::Relaxed));
        stop_agent_inner(state, "ws_cancel").unwrap();
        assert!(
            cancel.load(Ordering::Relaxed),
            "stop_agent must flip the cancel token before dropping the handle"
        );
    }
}
