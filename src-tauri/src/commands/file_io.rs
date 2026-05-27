// Phase 2b · Task 4 — file_read + file_write commands.
//
// Editor-backing IPC. Read returns content + binary detection + sha1
// for race-detect on save. Write is atomic via .tmp+rename and
// rejects with FileChangedOnDisk when the on-disk sha1 doesn't match
// what the frontend received from its last read — catches the
// agent-edited-file-while-user-typed race that plagues korlap.
//
// Path resolution + traversal defense reuses
// `commands::files::resolve_within_worktree` so editor and tree share
// the same canonicalize-then-prefix-check rules.

use crate::commands::files::resolve_within_worktree;
use crate::error::{AppError, Result};
use crate::state::{AppState, WorkspaceEvent, WorkspaceEventTx};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::sync::{Arc, Mutex};
use tauri::State;

/// 1 MB cap on editor-loaded files. Anything larger surfaces as
/// `is_binary: false, size: N` with a content stub — the editor refuses
/// to open and shows a "file too large" placeholder.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileReadResponse {
    /// Decoded content. Empty when `is_binary` is true or size > 1 MB.
    pub content: String,
    pub is_binary: bool,
    pub size: u64,
    /// sha1 of the on-disk bytes at read time. Frontend must round-trip
    /// this back to `file_write` so the race-detect can fire.
    pub sha1: String,
}

pub fn file_read_inner(
    workspace_id: &str,
    rel_path: &str,
    state: Arc<Mutex<AppState>>,
) -> Result<FileReadResponse> {
    if rel_path.trim().is_empty() {
        return Err(AppError::Other("file path is empty".into()));
    }

    let worktree = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        ws.worktree_dir.clone()
    };

    if !worktree.exists() {
        return Err(AppError::PathNotFound(worktree));
    }

    let absolute = resolve_within_worktree(&worktree, rel_path)?;
    if !absolute.is_file() {
        return Err(AppError::Other(format!(
            "not a regular file: {}",
            absolute.display()
        )));
    }

    let bytes = std::fs::read(&absolute)?;
    let size = bytes.len() as u64;
    let sha1 = sha1_hex(&bytes);

    if size > MAX_FILE_BYTES {
        return Ok(FileReadResponse {
            content: String::new(),
            is_binary: false,
            size,
            sha1,
        });
    }

    let is_binary = looks_binary(&bytes);
    let content = if is_binary {
        String::new()
    } else {
        // Lossy decode — invalid UTF-8 surfaces as � rather than an
        // error so the editor can still show "best-effort" rendering
        // for files with edge-case encoding (rare in real codebases).
        String::from_utf8_lossy(&bytes).into_owned()
    };

    Ok(FileReadResponse {
        content,
        is_binary,
        size,
        sha1,
    })
}

pub fn file_write_inner(
    workspace_id: &str,
    rel_path: &str,
    content: &str,
    expected_sha1: &str,
    state: Arc<Mutex<AppState>>,
) -> Result<FileWriteResponse> {
    file_write_inner_with_publisher(workspace_id, rel_path, content, expected_sha1, state, None)
}

/// Variant of [`file_write_inner`] that emits
/// `WorkspaceEvent::FileTouched` to the team-activity publisher on a
/// successful write. Kept as a separate function so existing unit tests
/// (which don't need to assert telemetry) keep calling the no-arg form.
pub fn file_write_inner_with_publisher(
    workspace_id: &str,
    rel_path: &str,
    content: &str,
    expected_sha1: &str,
    state: Arc<Mutex<AppState>>,
    publisher_tx: Option<&WorkspaceEventTx>,
) -> Result<FileWriteResponse> {
    if rel_path.trim().is_empty() {
        return Err(AppError::Other("file path is empty".into()));
    }

    let worktree = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        ws.worktree_dir.clone()
    };

    if !worktree.exists() {
        return Err(AppError::PathNotFound(worktree));
    }

    // Resolve safely — manually because the file may not exist yet
    // (editor "save as" path), in which case `dunce::canonicalize`
    // would fail. Build the path via the worktree-relative form and
    // verify the canonicalized parent stays inside the worktree.
    let canonical_root = dunce::canonicalize(&worktree)
        .map_err(|e| AppError::Other(format!("canonicalize worktree: {e}")))?;
    let normalized = rel_path.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.starts_with('\\')
        || normalized.split('/').any(|seg| seg == "..")
    {
        return Err(AppError::Other(format!(
            "path '{rel_path}' is outside worktree"
        )));
    }
    if rel_path.len() >= 2 {
        let mut chars = rel_path.chars();
        let c0 = chars.next().unwrap();
        let c1 = chars.next().unwrap();
        if c0.is_ascii_alphabetic() && c1 == ':' {
            return Err(AppError::Other(format!(
                "path '{rel_path}' is outside worktree"
            )));
        }
    }
    let absolute = worktree.join(&normalized);
    let parent = absolute
        .parent()
        .ok_or_else(|| AppError::Other(format!("path '{rel_path}' has no parent directory")))?;
    std::fs::create_dir_all(parent)?;
    let canonical_parent =
        dunce::canonicalize(parent).map_err(|_| AppError::PathNotFound(parent.to_path_buf()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(AppError::Other(format!(
            "path '{rel_path}' is outside worktree"
        )));
    }

    // Race-detect: when the file already exists, hash its current
    // bytes and compare with the frontend's expected_sha1. Empty
    // expected_sha1 means "this is a new file, no comparison needed".
    if absolute.exists() && !expected_sha1.is_empty() {
        let on_disk = std::fs::read(&absolute)?;
        let on_disk_sha = sha1_hex(&on_disk);
        if on_disk_sha != expected_sha1 {
            return Err(AppError::InvalidState(format!(
                "FileChangedOnDisk: expected sha1 {expected_sha1}, got {on_disk_sha}"
            )));
        }
    }

    // Atomic write via .tmp + rename. Mirrors persistence/atomic.rs
    // but bytes-shaped, so it can carry editor content (which may
    // include non-UTF-8 sequences).
    let new_bytes = content.as_bytes();
    let tmp = absolute.with_extension(format!(
        "{}.tmp",
        absolute
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, new_bytes)?;
    std::fs::rename(&tmp, &absolute)?;

    let resp = FileWriteResponse {
        sha1: sha1_hex(new_bytes),
        size: new_bytes.len() as u64,
    };
    // Task 12: surface user-driven file edits on the team-activity row.
    // Fire-and-forget; SendError just means no publisher is wired (or it
    // has shut down) and is not actionable from a single file write.
    if let Some(tx) = publisher_tx {
        let _ = tx.send(WorkspaceEvent::FileTouched {
            workspace_id: workspace_id.to_string(),
        });
    }
    Ok(resp)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileWriteResponse {
    /// sha1 of the bytes that were written. Frontend stamps this onto
    /// the editor-tabs store as the new "expected_sha1" for the next
    /// save round-trip.
    pub sha1: String,
    pub size: u64,
}

#[tauri::command]
pub async fn file_read(
    workspace_id: String,
    path: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<FileReadResponse, String> {
    let inner_state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        file_read_inner(&workspace_id, &path, inner_state).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("file_read task join error: {e}"))?
}

#[tauri::command]
pub async fn file_write(
    workspace_id: String,
    path: String,
    content: String,
    expected_sha1: String,
    state: State<'_, Arc<Mutex<AppState>>>,
    event_tx: State<'_, WorkspaceEventTx>,
) -> std::result::Result<FileWriteResponse, String> {
    let inner_state = state.inner().clone();
    let publisher_tx: WorkspaceEventTx = event_tx.inner().clone();
    tokio::task::spawn_blocking(move || {
        file_write_inner_with_publisher(
            &workspace_id,
            &path,
            &content,
            &expected_sha1,
            inner_state,
            Some(&publisher_tx),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("file_write task join error: {e}"))?
}

// ── helpers ──────────────────────────────────────────────────────────

fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(40);
    for b in digest.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|&b| b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
    use std::path::{Path, PathBuf};
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
                team_activity_private: false,
                task_id: None,
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

    // ── file_read ───────────────────────────────────────────────────

    #[test]
    fn file_read_inner_returns_text_content_and_sha1() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("hello.txt"), b"hello world\n").unwrap();
        let state = make_state("ws_read", &wt);
        let resp = file_read_inner("ws_read", "hello.txt", state).unwrap();
        assert_eq!(resp.content, "hello world\n");
        assert!(!resp.is_binary);
        assert_eq!(resp.size, 12);
        // Pre-computed sha1("hello world\n").
        assert_eq!(resp.sha1, "22596363b3de40b06f981fb85d82312e8c0ed511");
    }

    #[test]
    fn file_read_inner_marks_binary_for_nul_bytes() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("blob.bin"), [0x00, 0x01, 0x02, 0x03]).unwrap();
        let state = make_state("ws_bin", &wt);
        let resp = file_read_inner("ws_bin", "blob.bin", state).unwrap();
        assert!(resp.is_binary);
        assert!(resp.content.is_empty());
        assert_eq!(resp.size, 4);
    }

    #[test]
    fn file_read_inner_caps_files_above_1mb() {
        let (_tmp, wt) = make_worktree();
        let big = vec![b'a'; (1024 * 1024 + 1) as usize];
        std::fs::write(wt.join("big.txt"), &big).unwrap();
        let state = make_state("ws_big", &wt);
        let resp = file_read_inner("ws_big", "big.txt", state).unwrap();
        assert!(resp.content.is_empty());
        assert_eq!(resp.size, big.len() as u64);
        // sha1 still computed so the frontend can re-render the file
        // header without "loading…" forever.
        assert_eq!(resp.sha1.len(), 40);
    }

    #[test]
    fn file_read_inner_rejects_path_traversal() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_trav", &wt);
        let err = file_read_inner("ws_trav", "../etc/passwd", state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("outside worktree"));
    }

    #[test]
    fn file_read_inner_rejects_empty_path() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_empty", &wt);
        let err = file_read_inner("ws_empty", "  ", state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("empty"));
    }

    #[test]
    fn file_read_inner_returns_error_for_missing_file() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_missing", &wt);
        let err = file_read_inner("ws_missing", "ghost.txt", state).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("path") || msg.contains("not found"));
    }

    #[test]
    fn file_read_inner_returns_error_for_directory() {
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        let state = make_state("ws_dir", &wt);
        let err = file_read_inner("ws_dir", "sub", state).unwrap_err();
        assert!(err
            .to_string()
            .to_lowercase()
            .contains("not a regular file"));
    }

    #[test]
    fn file_read_inner_returns_error_for_invalid_workspace() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = file_read_inner("ws_unknown", "x.txt", state).unwrap_err();
        assert!(err.to_string().contains("ws_unknown"));
    }

    // ── file_write ──────────────────────────────────────────────────

    #[test]
    fn file_write_inner_creates_new_file_with_empty_expected_sha1() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_new", &wt);
        let resp = file_write_inner("ws_new", "fresh.txt", "hi", "", state).unwrap();
        assert_eq!(resp.size, 2);
        assert_eq!(resp.sha1.len(), 40);
        assert_eq!(std::fs::read_to_string(wt.join("fresh.txt")).unwrap(), "hi");
    }

    #[test]
    fn file_write_inner_overwrites_existing_when_sha1_matches() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("a.txt"), b"old").unwrap();
        let state = make_state("ws_match", &wt);
        let read = file_read_inner("ws_match", "a.txt", Arc::clone(&state)).unwrap();
        let resp = file_write_inner("ws_match", "a.txt", "new content", &read.sha1, state).unwrap();
        assert_eq!(resp.size, 11);
        assert_eq!(
            std::fs::read_to_string(wt.join("a.txt")).unwrap(),
            "new content"
        );
    }

    #[test]
    fn file_write_inner_rejects_when_expected_sha1_mismatches() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("a.txt"), b"original").unwrap();
        let state = make_state("ws_race", &wt);
        let err = file_write_inner(
            "ws_race",
            "a.txt",
            "would overwrite",
            "0000000000000000000000000000000000000000",
            state,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("FileChangedOnDisk"),
            "expected FileChangedOnDisk, got: {err}"
        );
        // The on-disk file is unchanged.
        assert_eq!(
            std::fs::read_to_string(wt.join("a.txt")).unwrap(),
            "original"
        );
    }

    #[test]
    fn file_write_inner_creates_parent_directories() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_mkdir", &wt);
        file_write_inner("ws_mkdir", "nested/deep/file.txt", "content", "", state).unwrap();
        assert_eq!(
            std::fs::read_to_string(wt.join("nested/deep/file.txt")).unwrap(),
            "content"
        );
    }

    #[test]
    fn file_write_inner_rejects_path_traversal() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_trav", &wt);
        let err = file_write_inner("ws_trav", "../escape.txt", "x", "", state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("outside worktree"));
    }

    #[test]
    fn file_write_inner_rejects_absolute_path() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_abs", &wt);
        let err = file_write_inner("ws_abs", "/etc/hosts", "x", "", state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("outside worktree"));
    }

    #[test]
    fn file_write_inner_rejects_drive_letter_path() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_drive", &wt);
        let err = file_write_inner("ws_drive", "C:\\Windows\\hosts", "x", "", state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("outside worktree"));
    }

    #[test]
    fn file_write_inner_rejects_empty_path() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_empty", &wt);
        let err = file_write_inner("ws_empty", " ", "x", "", state).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("empty"));
    }

    #[test]
    fn file_write_inner_returns_error_for_invalid_workspace() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let err = file_write_inner("ws_x", "a.txt", "x", "", state).unwrap_err();
        assert!(err.to_string().contains("ws_x"));
    }

    // ── helpers ─────────────────────────────────────────────────────

    #[test]
    fn sha1_hex_returns_40_lowercase_hex_chars() {
        let s = sha1_hex(b"abc");
        // Pre-computed sha1("abc").
        assert_eq!(s, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(s.len(), 40);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn sha1_hex_handles_empty_input() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn looks_binary_detects_nul_byte_within_first_8kb() {
        assert!(looks_binary(&[1, 2, 0, 3]));
        assert!(!looks_binary(b"hello world"));
    }

    // ── Task 12 — WorkspaceEvent::FileTouched emission ───────────────────────

    fn make_publisher_tx() -> (
        crate::state::WorkspaceEventTx,
        tokio::sync::broadcast::Receiver<crate::state::WorkspaceEvent>,
    ) {
        let (tx, rx) = tokio::sync::broadcast::channel::<crate::state::WorkspaceEvent>(8);
        (std::sync::Arc::new(tx), rx)
    }

    #[test]
    fn file_write_with_publisher_emits_file_touched_on_success() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_emit_write", &wt);
        let (tx, mut rx) = make_publisher_tx();
        file_write_inner_with_publisher("ws_emit_write", "x.txt", "hi", "", state, Some(&tx))
            .unwrap();
        let ev = rx.try_recv().expect("FileTouched should be emitted");
        match ev {
            crate::state::WorkspaceEvent::FileTouched { workspace_id } => {
                assert_eq!(workspace_id, "ws_emit_write");
            }
            other => panic!("expected FileTouched, got {other:?}"),
        }
    }

    #[test]
    fn file_write_with_publisher_skips_emit_when_write_rejected() {
        // sha1 mismatch → InvalidState; we must NOT emit a FileTouched
        // for a write that never actually landed on disk.
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("a.txt"), b"original").unwrap();
        let state = make_state("ws_emit_skip", &wt);
        let (tx, mut rx) = make_publisher_tx();
        let err = file_write_inner_with_publisher(
            "ws_emit_skip",
            "a.txt",
            "would overwrite",
            "0000000000000000000000000000000000000000",
            state,
            Some(&tx),
        )
        .unwrap_err();
        assert!(err.to_string().contains("FileChangedOnDisk"));
        assert!(rx.try_recv().is_err(), "no event expected on failure");
    }

    #[test]
    fn file_write_without_publisher_still_writes_and_is_noop() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_nopub", &wt);
        let resp =
            file_write_inner_with_publisher("ws_nopub", "x.txt", "hi", "", state, None).unwrap();
        assert_eq!(resp.size, 2);
        // Backwards-compat wrapper threads `None`:
        let (_t2, wt2) = make_worktree();
        let state2 = make_state("ws_nopub2", &wt2);
        file_write_inner("ws_nopub2", "y.txt", "yo", "", state2).unwrap();
        assert_eq!(std::fs::read_to_string(wt2.join("y.txt")).unwrap(), "yo");
    }
}
