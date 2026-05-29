// Phase 2a · Task 1 — `workspace_diff` command.
//
// Returns the unified diff for a workspace's worktree. Streams 64 KB chunks
// over a `Channel<DiffChunk>` so a multi-megabyte diff doesn't serialize
// through one IPC payload. Untracked files are surfaced as synthetic
// `+++ b/<path>` blocks because `git diff` excludes them by default.
//
// The inner function takes a closure rather than a `Channel<T>` so it can
// be unit-tested without a Tauri runtime — the same pattern Phase 1 uses
// for agent_core.
//
// TODO(phase-3a-3-followup): emit `WorkspaceEvent::DiffSummaryUpdated`
// from a future commit/push/diff-snapshot flow. `workspace_diff` itself
// is a pull-driven read — emitting from here would fire on every UI
// refresh. The clean emission point is a backend commit/push handler
// that doesn't yet exist; until then, the publisher's `diff_summary`
// column on the team-activity row stays unpopulated. The variant is
// already defined on `WorkspaceEvent` (state.rs) and exercised by the
// publisher's debounce/private-lock tests, so wiring it later is a
// pure additive change.

use crate::error::{AppError, Result};
use crate::state::AppState;
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// One chunk of streamed diff output. Tagged so the frontend can pattern-
/// match without an extra discriminator.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffChunk {
    /// A slice of unified-diff text. Multiple chunks concatenate to the
    /// full diff.
    Text { text: String },
    /// Non-zero exit code from `git diff`. Always followed by `Eof`.
    Error { message: String },
    /// End-of-stream sentinel. Frontend uses this to release its loading
    /// indicator.
    Eof,
}

const CHUNK_SIZE: usize = 64 * 1024;
const MAX_UNTRACKED_BYTES: u64 = 1024 * 1024; // 1 MB cap matches `git diff` default for binary content
const EMPTY_TREE_OID: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Inner function — exposed for unit tests. The `emit` closure is invoked
/// once per [`DiffChunk`]. The implementation is responsible for closing
/// the stream with [`DiffChunk::Eof`] before returning `Ok(())` (errors
/// short-circuit before `Eof` and the caller is expected to surface them).
pub fn workspace_diff_inner<F>(
    workspace_id: &str,
    state: Arc<Mutex<AppState>>,
    mut emit: F,
) -> Result<()>
where
    F: FnMut(DiffChunk),
{
    let worktree_dir = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        ws.worktree_dir.clone()
    };

    if !worktree_dir.join(".git").exists() && !worktree_dir.join(".git").is_file() {
        // `.git` may be a directory (regular clone) or a file (worktree
        // pointer). Both are valid; nothing else is.
        let exists = worktree_dir.join(".git").exists();
        if !exists {
            return Err(AppError::Git(format!(
                "not a git repository: {}",
                worktree_dir.display()
            )));
        }
    }

    let mut output = String::new();

    // Tracked changes — try HEAD first, fall back to the empty tree on
    // repos without any commits yet.
    match run_git_diff(&worktree_dir, "HEAD") {
        Ok(text) => output.push_str(&text),
        Err(_) => {
            let text = run_git_diff(&worktree_dir, EMPTY_TREE_OID)?;
            output.push_str(&text);
        }
    }

    // Untracked files — concatenate as synthetic additions.
    let untracked = run_git_ls_untracked(&worktree_dir)?;
    for rel in untracked {
        let abs = worktree_dir.join(&rel);
        match synthesize_untracked_diff(&rel, &abs) {
            Ok(snippet) => output.push_str(&snippet),
            Err(e) => {
                // A read failure on one untracked file shouldn't sink the
                // whole diff — record it inline and keep going.
                output.push_str(&format!(
                    "diff --git a/{rel} b/{rel}\n\
                     # error reading untracked file: {e}\n"
                ));
            }
        }
    }

    // Chunk output into ≤ CHUNK_SIZE slices for streaming. Splitting at a
    // byte boundary is fine because the consumer concatenates `text` back
    // into one buffer before parsing — UTF-8 boundary breaks would only
    // matter if each chunk were rendered standalone, which it isn't.
    if !output.is_empty() {
        let mut start = 0;
        while start < output.len() {
            let end = (start + CHUNK_SIZE).min(output.len());
            let end = adjust_to_char_boundary(&output, end);
            emit(DiffChunk::Text {
                text: output[start..end].to_string(),
            });
            start = end;
        }
    }

    emit(DiffChunk::Eof);
    Ok(())
}

/// Tauri command wrapper. Forwards each chunk over the channel; backpressure
/// is the consumer's problem (the channel buffers internally).
#[tauri::command]
pub async fn workspace_diff(
    workspace_id: String,
    channel: tauri::ipc::Channel<DiffChunk>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let state = state.inner().clone();
    // Run on the blocking pool — `git` shell-out + file I/O are sync and
    // could be lengthy on a large worktree.
    tokio::task::spawn_blocking(move || {
        let emit = |chunk: DiffChunk| {
            let _ = channel.send(chunk);
        };
        workspace_diff_inner(&workspace_id, state, emit).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("diff task join error: {e}"))?
}

// ── helpers ─────────────────────────────────────────────────────────

fn run_git_diff(worktree: &Path, target: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["diff", "--no-color", target])
        .current_dir(worktree)
        .output()
        .map_err(|e| AppError::Command {
            cmd: "git diff".into(),
            msg: e.to_string(),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(AppError::Git(format!("git diff: {}", stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_git_ls_untracked(worktree: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(worktree)
        .output()
        .map_err(|e| AppError::Command {
            cmd: "git ls-files".into(),
            msg: e.to_string(),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(AppError::Git(format!("git ls-files: {}", stderr.trim())));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.lines().map(|s| s.to_string()).collect())
}

/// Build a unified-diff-shaped block for an untracked file: header,
/// `--- /dev/null`, `+++ b/<rel>`, then each line of the file as `+<line>`.
/// Files larger than [`MAX_UNTRACKED_BYTES`] surface as a "Binary file …
/// not shown" line, matching `git diff`'s default for binary content.
fn synthesize_untracked_diff(rel: &str, abs: &Path) -> Result<String> {
    let meta = std::fs::metadata(abs)?;
    let size = meta.len();
    let mut out = String::new();
    out.push_str(&format!(
        "diff --git a/{rel} b/{rel}\nnew file mode 100644\n--- /dev/null\n+++ b/{rel}\n"
    ));
    if size > MAX_UNTRACKED_BYTES {
        out.push_str(&format!("Binary file {rel} not shown\n"));
        return Ok(out);
    }
    let bytes = std::fs::read(abs)?;
    // Skip the heuristic for empty files — `git diff` shows them with no
    // hunks, so we do the same.
    if bytes.is_empty() {
        return Ok(out);
    }
    if looks_binary(&bytes) {
        out.push_str(&format!("Binary file {rel} not shown\n"));
        return Ok(out);
    }
    let text = String::from_utf8_lossy(&bytes);
    let line_count = text.lines().count();
    out.push_str(&format!("@@ -0,0 +1,{line_count} @@\n"));
    for line in text.lines() {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

fn looks_binary(bytes: &[u8]) -> bool {
    // Same trick `git` uses: a NUL byte in the first 8 KB means binary.
    bytes.iter().take(8 * 1024).any(|&b| b == 0)
}

/// Walk `idx` left until it lands on a UTF-8 char boundary. Prevents
/// chunks from splitting a multi-byte sequence and emitting invalid UTF-8.
fn adjust_to_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Creates a freshly-initialized git repo with a single committed file
    /// `foo.txt` containing `old\n`. Returns the temp dir + worktree path.
    fn init_repo() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path().join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        run(&worktree, &["init", "-q", "-b", "main"]);
        run(&worktree, &["config", "user.email", "t@t.com"]);
        run(&worktree, &["config", "user.name", "T"]);
        std::fs::write(worktree.join("foo.txt"), b"old\n").unwrap();
        run(&worktree, &["add", "foo.txt"]);
        run(&worktree, &["commit", "-q", "-m", "init"]);
        (tmp, worktree)
    }

    fn run(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

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
                task_ids: Vec::new(),
            },
        );
        Arc::new(Mutex::new(state))
    }

    fn collect(workspace_id: &str, state: Arc<Mutex<AppState>>) -> Result<Vec<DiffChunk>> {
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let chunks_clone = Arc::clone(&chunks);
        workspace_diff_inner(workspace_id, state, move |c| {
            chunks_clone.lock().unwrap().push(c);
        })?;
        let out = chunks.lock().unwrap().clone();
        Ok(out)
    }

    fn concat_text(chunks: &[DiffChunk]) -> String {
        chunks
            .iter()
            .filter_map(|c| match c {
                DiffChunk::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    // ── happy paths ──────────────────────────────────────────────────

    #[test]
    fn workspace_diff_inner_returns_eof_only_for_clean_worktree() {
        let (_tmp, worktree) = init_repo();
        let state = make_state("ws_clean", &worktree);
        let chunks = collect("ws_clean", state).unwrap();
        assert_eq!(chunks, vec![DiffChunk::Eof]);
    }

    #[test]
    fn workspace_diff_inner_returns_unified_diff_for_modified_file() {
        let (_tmp, worktree) = init_repo();
        std::fs::write(worktree.join("foo.txt"), b"new\n").unwrap();
        let state = make_state("ws_mod", &worktree);
        let chunks = collect("ws_mod", state).unwrap();
        let text = concat_text(&chunks);
        assert!(text.contains("--- a/foo.txt"), "missing --- header: {text}");
        assert!(text.contains("+++ b/foo.txt"), "missing +++ header: {text}");
        assert!(text.contains("-old"), "missing -old: {text}");
        assert!(text.contains("+new"), "missing +new: {text}");
        assert_eq!(chunks.last(), Some(&DiffChunk::Eof));
    }

    #[test]
    fn workspace_diff_inner_includes_untracked_as_full_addition() {
        let (_tmp, worktree) = init_repo();
        std::fs::write(worktree.join("untracked.txt"), b"hello\nworld\n").unwrap();
        let state = make_state("ws_untracked", &worktree);
        let chunks = collect("ws_untracked", state).unwrap();
        let text = concat_text(&chunks);
        assert!(
            text.contains("+++ b/untracked.txt"),
            "missing untracked header: {text}"
        );
        assert!(text.contains("+hello"), "missing +hello: {text}");
        assert!(text.contains("+world"), "missing +world: {text}");
    }

    #[test]
    fn workspace_diff_inner_marks_large_untracked_as_binary() {
        let (_tmp, worktree) = init_repo();
        // 2 MB of zeros — both binary (NUL bytes) and over the size cap.
        std::fs::write(worktree.join("blob.bin"), vec![0u8; 2 * 1024 * 1024]).unwrap();
        let state = make_state("ws_bin", &worktree);
        let chunks = collect("ws_bin", state).unwrap();
        let text = concat_text(&chunks);
        assert!(
            text.contains("Binary file blob.bin not shown"),
            "expected binary marker, got: {text}"
        );
    }

    #[test]
    fn workspace_diff_inner_chunks_large_diff() {
        let (_tmp, worktree) = init_repo();
        let big = (0..8_192)
            .map(|i| format!("line {i}\n"))
            .collect::<String>();
        std::fs::write(worktree.join("foo.txt"), big.as_bytes()).unwrap();
        let state = make_state("ws_big", &worktree);
        let chunks = collect("ws_big", state).unwrap();
        let text_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| matches!(c, DiffChunk::Text { .. }))
            .collect();
        assert!(
            text_chunks.len() >= 2,
            "expected ≥2 text chunks, got {}",
            text_chunks.len()
        );
        // Every text chunk must respect the size cap.
        for c in &text_chunks {
            if let DiffChunk::Text { text } = c {
                assert!(
                    text.len() <= CHUNK_SIZE,
                    "chunk {} exceeds CHUNK_SIZE",
                    text.len()
                );
            }
        }
        // Total reconstruction still contains the addition markers.
        let full = concat_text(&chunks);
        assert!(full.contains("+line 0"));
        assert!(full.contains("+line 8191"));
    }

    #[test]
    fn workspace_diff_inner_handles_repo_with_no_commits() {
        // Initializing a repo without any commits — `git diff HEAD` errors,
        // but the empty-tree fallback should still produce useful output.
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path().join("fresh");
        std::fs::create_dir_all(&worktree).unwrap();
        run(&worktree, &["init", "-q", "-b", "main"]);
        run(&worktree, &["config", "user.email", "t@t.com"]);
        run(&worktree, &["config", "user.name", "T"]);
        std::fs::write(worktree.join("seed.txt"), b"hi\n").unwrap();
        // `seed.txt` is untracked — we still expect the synthesized
        // addition path to fire.
        let state = make_state("ws_fresh", &worktree);
        let chunks = collect("ws_fresh", state).unwrap();
        let text = concat_text(&chunks);
        assert!(text.contains("+++ b/seed.txt"), "expected seed header");
        assert!(text.contains("+hi"), "expected +hi");
    }

    // ── error paths ──────────────────────────────────────────────────

    #[test]
    fn workspace_diff_inner_returns_error_for_invalid_workspace_id() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let err = workspace_diff_inner("ws_missing", state, |_| {}).unwrap_err();
        assert!(
            err.to_string().contains("ws_missing"),
            "error should name the workspace id: {err}"
        );
    }

    #[test]
    fn workspace_diff_inner_returns_error_for_non_git_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path().join("not_git");
        std::fs::create_dir_all(&worktree).unwrap();
        let state = make_state("ws_no_git", &worktree);
        let err = workspace_diff_inner("ws_no_git", state, |_| {}).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("not a git repository"),
            "expected 'not a git repository' in: {msg}"
        );
    }

    // ── chunk helper ─────────────────────────────────────────────────

    #[test]
    fn adjust_to_char_boundary_walks_back_on_multibyte_split() {
        let s = "héllo"; // é = 2 bytes (0xc3 0xa9)
                         // raw byte index 2 lands inside é; expect walk-back to byte 1.
        assert_eq!(adjust_to_char_boundary(s, 2), 1);
        // boundary already aligned.
        assert_eq!(adjust_to_char_boundary(s, 3), 3);
        // past-end clamps.
        assert_eq!(adjust_to_char_boundary(s, 99), s.len());
    }

    #[test]
    fn looks_binary_detects_nul_byte() {
        assert!(looks_binary(&[1, 2, 0, 3]));
        assert!(!looks_binary(b"hello world"));
    }
}
