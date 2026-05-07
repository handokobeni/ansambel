// Phase 2b · Task 2 + Task 3 — per-workspace terminal session.
//
// Five Tauri commands managing one PTY per workspace:
//
//   - terminal_spawn      starts the shell, kicks off reader/writer
//                         threads, registers a TerminalHandle in
//                         AppState.terminals.
//   - terminal_write      pushes bytes (typically keystrokes) onto the
//                         writer-thread channel.
//   - terminal_resize     re-flows the PTY to new cols/rows.
//   - terminal_kill       drops the handle + terminates the child.
//   - terminal_reattach   subscribes a fresh Tauri Channel to the
//                         existing broadcaster so the frontend can
//                         re-attach after a workspace switch + back.
//
// The `_inner` helpers take an `Arc<Mutex<AppState>>` directly so they
// can be unit-tested without a Tauri runtime — same pattern Phase 1's
// agent_core uses.

use crate::error::{AppError, Result};
use crate::platform::pty;
use crate::state::{AppState, TerminalChunk, TerminalHandle};
use portable_pty::CommandBuilder;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::{broadcast, mpsc};

/// Channel buffer size for terminal-output broadcaster. Same size as
/// `AgentHandle.event_tx` — slow consumers drop oldest with `Lagged`.
const BROADCAST_CAPACITY: usize = 256;

/// Default PTY dimensions when the frontend hasn't measured the
/// container yet. xterm.js will call `terminal_resize` very shortly
/// after attaching its FitAddon.
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

/// Hard caps on PTY dimensions. Tiny windows are valid; gigantic ones
/// are almost certainly a bug or a malformed UI input.
const MIN_DIM: u16 = 1;
const MAX_DIM: u16 = 1000;

// ── inner functions (testable without Tauri runtime) ─────────────────

pub fn spawn_terminal_inner(
    workspace_id: &str,
    cols: u16,
    rows: u16,
    state: Arc<Mutex<AppState>>,
) -> Result<broadcast::Receiver<TerminalChunk>> {
    // Resolve the workspace's worktree dir — the shell's CWD is rooted
    // there. Fail fast if the workspace doesn't exist or its worktree
    // is missing on disk.
    let worktree_dir = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        if st.terminals.contains_key(workspace_id) {
            return Err(AppError::InvalidState(format!(
                "terminal already active for workspace '{workspace_id}' — call reattach instead"
            )));
        }
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        ws.worktree_dir.clone()
    };

    if !worktree_dir.exists() {
        return Err(AppError::PathNotFound(worktree_dir));
    }

    // Build the shell command: respect $SHELL on Unix, fall back to
    // /bin/sh; Windows uses cmd.exe. xterm.js's interactive UX assumes
    // a shell prompt, so this is what the user expects.
    let cmd = build_shell_command(&worktree_dir);

    let session = pty::spawn(cmd)?;
    // Resize before starting threads so the first prompt renders at
    // the requested dimensions.
    let cols = cols.clamp(MIN_DIM, MAX_DIM);
    let rows = rows.clamp(MIN_DIM, MAX_DIM);
    let _ = session.resize(rows, cols);

    let reader = session.reader()?;
    let writer = session.writer()?;
    let session = Arc::new(Mutex::new(session));

    let (event_tx, event_rx) = broadcast::channel::<TerminalChunk>(BROADCAST_CAPACITY);
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let cancel = Arc::new(AtomicBool::new(false));

    spawn_writer_thread(writer, stdin_rx);
    spawn_reader_thread(
        reader,
        event_tx.clone(),
        Arc::clone(&cancel),
        Arc::clone(&session),
    );

    let handle = TerminalHandle {
        workspace_id: workspace_id.into(),
        stdin_tx,
        event_tx,
        cancel,
        pty: session,
    };
    state
        .lock()
        .map_err(|e| AppError::Other(e.to_string()))?
        .terminals
        .insert(workspace_id.into(), handle);

    Ok(event_rx)
}

pub fn write_terminal_inner(
    workspace_id: &str,
    bytes: Vec<u8>,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let handle = st.terminals.get(workspace_id).ok_or_else(|| {
        AppError::NotFound(format!("no active terminal for workspace '{workspace_id}'"))
    })?;
    handle
        .stdin_tx
        .send(bytes)
        .map_err(|e| AppError::Other(format!("terminal stdin closed: {e}")))?;
    Ok(())
}

pub fn resize_terminal_inner(
    workspace_id: &str,
    cols: u16,
    rows: u16,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let cols = cols.clamp(MIN_DIM, MAX_DIM);
    let rows = rows.clamp(MIN_DIM, MAX_DIM);
    let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let handle = st.terminals.get(workspace_id).ok_or_else(|| {
        AppError::NotFound(format!("no active terminal for workspace '{workspace_id}'"))
    })?;
    let pty = handle
        .pty
        .lock()
        .map_err(|e| AppError::Other(e.to_string()))?;
    pty.resize(rows, cols)?;
    Ok(())
}

pub fn kill_terminal_inner(workspace_id: &str, state: Arc<Mutex<AppState>>) -> Result<()> {
    // Remove the handle first so concurrent reattach attempts can't
    // race with the kill. Idempotent: a missing handle returns Ok(()).
    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let Some(handle) = st.terminals.remove(workspace_id) else {
        return Ok(());
    };
    handle.cancel.store(true, Ordering::SeqCst);
    let mut pty = handle
        .pty
        .lock()
        .map_err(|e| AppError::Other(e.to_string()))?;
    let _ = pty.kill();
    Ok(())
}

pub fn reattach_terminal_inner(
    workspace_id: &str,
    state: Arc<Mutex<AppState>>,
) -> Result<broadcast::Receiver<TerminalChunk>> {
    let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let handle = st.terminals.get(workspace_id).ok_or_else(|| {
        AppError::NotFound(format!("no active terminal for workspace '{workspace_id}'"))
    })?;
    Ok(handle.event_tx.subscribe())
}

// ── Tauri command wrappers ───────────────────────────────────────────

#[tauri::command]
pub async fn terminal_spawn(
    workspace_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
    channel: Channel<TerminalChunk>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let cols = cols.unwrap_or(DEFAULT_COLS);
    let rows = rows.unwrap_or(DEFAULT_ROWS);
    let inner_state = state.inner().clone();
    let rx = spawn_terminal_inner(&workspace_id, cols, rows, inner_state).map_err(|e| {
        tracing::error!(error = %e, "terminal_spawn failed");
        e.to_string()
    })?;
    forward_to_channel(rx, channel);
    Ok(())
}

#[tauri::command]
pub async fn terminal_write(
    workspace_id: String,
    bytes: Vec<u8>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    write_terminal_inner(&workspace_id, bytes, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_resize(
    workspace_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    resize_terminal_inner(&workspace_id, cols, rows, state.inner().clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_kill(
    workspace_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    kill_terminal_inner(&workspace_id, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_reattach(
    workspace_id: String,
    channel: Channel<TerminalChunk>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let rx =
        reattach_terminal_inner(&workspace_id, state.inner().clone()).map_err(|e| e.to_string())?;
    forward_to_channel(rx, channel);
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────

fn build_shell_command(cwd: &Path) -> CommandBuilder {
    let mut cmd = if cfg!(windows) {
        CommandBuilder::new("cmd.exe")
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        CommandBuilder::new(shell)
    };
    cmd.cwd(cwd);
    cmd
}

fn spawn_writer_thread(
    mut writer: Box<dyn std::io::Write + Send>,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    std::thread::spawn(move || {
        while let Some(bytes) = rx.blocking_recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
            if writer.flush().is_err() {
                break;
            }
        }
    });
}

fn spawn_reader_thread(
    mut reader: Box<dyn std::io::Read + Send>,
    event_tx: broadcast::Sender<TerminalChunk>,
    cancel: Arc<AtomicBool>,
    session: Arc<Mutex<pty::PtySession>>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let _ = event_tx.send(TerminalChunk::Bytes {
                        bytes: buf[..n].to_vec(),
                    });
                }
                Err(_) => break,
            }
        }
        // Process exited (or errored). Capture exit code and emit an
        // Exited chunk so the frontend can render its inline marker.
        let code = session
            .lock()
            .ok()
            .and_then(|mut s| s.try_wait().ok().flatten())
            .and_then(|status| {
                let raw = status.exit_code();
                i32::try_from(raw).ok()
            });
        let _ = event_tx.send(TerminalChunk::Exited { code });
    });
}

fn forward_to_channel(mut rx: broadcast::Receiver<TerminalChunk>, channel: Channel<TerminalChunk>) {
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(chunk) => {
                    if channel.send(chunk).is_err() {
                        // Channel closed (frontend unmounted). The
                        // broadcaster keeps streaming for any other
                        // subscriber; we just stop forwarding.
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Slow consumer dropped chunks; keep going. The
                    // frontend's next render will pick up where we are.
                    continue;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_state(workspace_id: &str, worktree: &Path) -> Arc<Mutex<AppState>> {
        let mut state = AppState::default();
        state.workspaces.insert(
            workspace_id.into(),
            WorkspaceInfo {
                id: workspace_id.into(),
                repo_id: "repo_test".into(),
                branch: "ansambel/test".into(),
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

    fn make_worktree() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        (tmp, wt)
    }

    /// Drains the broadcaster receiver into a Vec, blocking until either
    /// `predicate` returns true or `deadline` passes. Used to assert
    /// "we eventually saw bytes containing X" without infinite loops.
    fn drain_until<F>(
        mut rx: broadcast::Receiver<TerminalChunk>,
        deadline: Duration,
        mut predicate: F,
    ) -> Vec<TerminalChunk>
    where
        F: FnMut(&[TerminalChunk]) -> bool,
    {
        let start = std::time::Instant::now();
        let mut out = Vec::new();
        while start.elapsed() < deadline {
            match rx.try_recv() {
                Ok(chunk) => {
                    out.push(chunk);
                    if predicate(&out) {
                        return out;
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
        out
    }

    fn collected_bytes(chunks: &[TerminalChunk]) -> Vec<u8> {
        let mut out = Vec::new();
        for c in chunks {
            if let TerminalChunk::Bytes { bytes } = c {
                out.extend_from_slice(bytes);
            }
        }
        out
    }

    fn has_exit(chunks: &[TerminalChunk]) -> bool {
        chunks
            .iter()
            .any(|c| matches!(c, TerminalChunk::Exited { .. }))
    }

    // ── spawn ────────────────────────────────────────────────────────

    #[test]
    fn spawn_terminal_inner_starts_shell_and_streams_output() {
        // Use a deterministic non-interactive shell command that exits
        // quickly: `echo hello`. The Bytes chunk should contain "hello"
        // and an Exited chunk should follow.
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_spawn", &wt);

        // Override SHELL so our build_shell_command picks an exit-fast
        // shell. Doesn't matter on Windows because cmd.exe is used.
        // We can't easily mock build_shell_command from tests, so this
        // test runs against the user's real shell — if the user has
        // a quirky $SHELL the test might be flaky. Cap with a short
        // deadline so a hang surfaces fast.
        let rx = spawn_terminal_inner("ws_spawn", 80, 24, Arc::clone(&state)).unwrap();

        // Push `exit\n` so the shell terminates quickly.
        write_terminal_inner("ws_spawn", b"exit\n".to_vec(), Arc::clone(&state)).unwrap();

        let chunks = drain_until(rx, Duration::from_secs(5), has_exit);
        assert!(
            has_exit(&chunks),
            "expected an Exited chunk, got: {chunks:?}"
        );
        // Some bytes should have arrived (shell prompt or echo).
        let bytes = collected_bytes(&chunks);
        assert!(
            !bytes.is_empty(),
            "expected at least one Bytes chunk, got none"
        );

        // Cleanup: remove the (now-exited) handle.
        kill_terminal_inner("ws_spawn", state).unwrap();
    }

    #[test]
    fn spawn_terminal_inner_rejects_double_spawn() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_dup", &wt);

        let _rx = spawn_terminal_inner("ws_dup", 80, 24, Arc::clone(&state)).unwrap();
        let err = spawn_terminal_inner("ws_dup", 80, 24, Arc::clone(&state)).unwrap_err();
        assert!(
            err.to_string().contains("already active"),
            "expected 'already active', got: {err}"
        );
        kill_terminal_inner("ws_dup", state).unwrap();
    }

    #[test]
    fn spawn_terminal_inner_returns_error_for_unknown_workspace() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = spawn_terminal_inner("ws_missing", 80, 24, state).unwrap_err();
        assert!(err.to_string().contains("ws_missing"));
    }

    #[test]
    fn spawn_terminal_inner_returns_error_for_missing_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("nope");
        let state = make_state("ws_gone", &wt);
        let err = spawn_terminal_inner("ws_gone", 80, 24, state).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("path") || msg.contains("not found"),
            "expected path-not-found-style error, got: {msg}"
        );
    }

    // ── write ────────────────────────────────────────────────────────

    #[test]
    fn write_terminal_inner_returns_error_when_no_handle() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = write_terminal_inner("ws_no_handle", b"hi".to_vec(), state).unwrap_err();
        assert!(err
            .to_string()
            .to_lowercase()
            .contains("no active terminal"));
    }

    // ── resize ───────────────────────────────────────────────────────

    #[test]
    fn resize_terminal_inner_clamps_extreme_values_and_returns_ok_for_active_session() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_resize", &wt);
        let _rx = spawn_terminal_inner("ws_resize", 80, 24, Arc::clone(&state)).unwrap();

        // Both extremes should clamp internally and not error.
        resize_terminal_inner("ws_resize", 0, 0, Arc::clone(&state)).unwrap();
        // u16::MAX exercises the upper-clamp branch.
        resize_terminal_inner("ws_resize", u16::MAX, u16::MAX, Arc::clone(&state)).unwrap();
        resize_terminal_inner("ws_resize", 120, 40, Arc::clone(&state)).unwrap();

        kill_terminal_inner("ws_resize", state).unwrap();
    }

    #[test]
    fn resize_terminal_inner_returns_error_when_no_handle() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = resize_terminal_inner("ws_x", 80, 24, state).unwrap_err();
        assert!(err
            .to_string()
            .to_lowercase()
            .contains("no active terminal"));
    }

    // ── kill ─────────────────────────────────────────────────────────

    #[test]
    fn kill_terminal_inner_is_idempotent() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_kill", &wt);
        let _rx = spawn_terminal_inner("ws_kill", 80, 24, Arc::clone(&state)).unwrap();

        kill_terminal_inner("ws_kill", Arc::clone(&state)).unwrap();
        // Second call: handle is gone, but inner returns Ok.
        kill_terminal_inner("ws_kill", Arc::clone(&state)).unwrap();
        // Third call on a workspace that never had a terminal: still Ok.
        kill_terminal_inner("ws_never_had_one", state).unwrap();
    }

    #[test]
    fn kill_terminal_inner_drops_handle() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_drop", &wt);
        let _rx = spawn_terminal_inner("ws_drop", 80, 24, Arc::clone(&state)).unwrap();
        assert!(state.lock().unwrap().terminals.contains_key("ws_drop"));
        kill_terminal_inner("ws_drop", Arc::clone(&state)).unwrap();
        assert!(!state.lock().unwrap().terminals.contains_key("ws_drop"));
    }

    // ── reattach ─────────────────────────────────────────────────────

    #[test]
    fn reattach_terminal_inner_returns_error_when_no_active_session() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = reattach_terminal_inner("ws_x", state).unwrap_err();
        assert!(err
            .to_string()
            .to_lowercase()
            .contains("no active terminal"));
    }

    #[test]
    fn reattach_terminal_inner_delivers_subsequent_chunks_to_new_subscriber() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_re", &wt);
        let _initial = spawn_terminal_inner("ws_re", 80, 24, Arc::clone(&state)).unwrap();

        // Subscribe a fresh receiver via reattach. The next bytes the
        // shell emits (via our `exit\n` push) must arrive on the new
        // receiver — proves reattach plugs into the broadcaster, not a
        // private channel.
        let rx = reattach_terminal_inner("ws_re", Arc::clone(&state)).unwrap();
        write_terminal_inner("ws_re", b"exit\n".to_vec(), Arc::clone(&state)).unwrap();

        let chunks = drain_until(rx, Duration::from_secs(5), has_exit);
        assert!(has_exit(&chunks), "expected Exited on reattached rx");

        kill_terminal_inner("ws_re", state).unwrap();
    }

    // ── helpers ──────────────────────────────────────────────────────

    #[test]
    fn build_shell_command_uses_worktree_cwd() {
        // The CommandBuilder doesn't expose its cwd accessor, so we
        // exercise this indirectly: build, spawn, read pwd output.
        let (_tmp, wt) = make_worktree();
        let cmd = build_shell_command(&wt);
        let session = pty::spawn(cmd).unwrap();
        // Push `pwd\nexit\n` and verify output contains the worktree
        // path. Skip on Windows where `cmd.exe` uses different syntax.
        if cfg!(windows) {
            return;
        }
        let mut writer = session.writer().unwrap();
        std::io::Write::write_all(&mut writer, b"pwd\nexit\n").unwrap();
        std::io::Write::flush(&mut writer).unwrap();
        drop(writer);

        let mut reader = session.reader().unwrap();
        let mut buf = String::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut tmp = [0u8; 4096];
        while std::time::Instant::now() < deadline {
            match std::io::Read::read(&mut reader, &mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.push_str(&String::from_utf8_lossy(&tmp[..n]));
                    if buf.contains(&wt.to_string_lossy().to_string()) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        assert!(
            buf.contains(&wt.to_string_lossy().to_string()),
            "expected worktree path in shell output, got: {buf:?}"
        );
    }
}
