// Phase 2a · Task 2 — `workspace_files` command.
//
// Returns the immediate children of a worktree directory, gitignore-aware
// (so `target/`, `node_modules/`, `dist/` don't drown the tree). Lazy
// expansion is the frontend's job — re-invoke with a sub-path on each
// directory click.
//
// Path traversal is rejected by canonicalizing the joined path and
// verifying it stays within the canonicalized worktree root.

use crate::error::{AppError, Result};
use crate::state::AppState;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    File,
    Dir,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileEntry {
    /// Basename — e.g. `src` for `<worktree>/src`.
    pub name: String,
    /// Path relative to the worktree root, forward-slash on every OS so
    /// the frontend never has to think about separators.
    pub path: String,
    pub kind: FileKind,
}

pub fn workspace_files_inner(
    workspace_id: &str,
    rel_path: Option<&str>,
    state: Arc<Mutex<AppState>>,
) -> Result<Vec<FileEntry>> {
    let worktree_dir = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        ws.worktree_dir.clone()
    };

    if !worktree_dir.exists() {
        return Err(AppError::PathNotFound(worktree_dir));
    }

    let rel_path_str = rel_path.unwrap_or("");
    let target = resolve_within_worktree(&worktree_dir, rel_path_str)?;
    if !target.is_dir() {
        return Err(AppError::Other(format!(
            "not a directory: {}",
            target.display()
        )));
    }

    let canonical_root = dunce::canonicalize(&worktree_dir)
        .map_err(|e| AppError::Other(format!("canonicalize worktree: {e}")))?;

    // The user-facing path of every result is `<rel_path>/<basename>` —
    // normalized to forward slashes. Computing it from `target` (which we
    // already canonicalized) plus the basename sidesteps the
    // strip_prefix-vs-canonicalization mismatch we otherwise hit on
    // Windows when the worktree path contains a tempdir short name like
    // `RUNNER~1` that the entry's path shows as `runneradmin`.
    let rel_prefix = rel_path_str.replace('\\', "/");
    let rel_prefix = rel_prefix.trim_end_matches('/').to_string();

    let mut entries: Vec<FileEntry> = Vec::new();
    let walker = ignore::WalkBuilder::new(&target)
        .max_depth(Some(1))
        .follow_links(false)
        .standard_filters(true)
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue, // a single unreadable entry shouldn't sink the listing
        };
        if entry.depth() == 0 {
            // The walker yields the root itself first.
            continue;
        }
        let abs = entry.path();
        // Skip anything that escapes the worktree (symlinks pointing outside).
        let canonical = match dunce::canonicalize(abs) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canonical.starts_with(&canonical_root) {
            continue;
        }
        let basename = entry.file_name().to_string_lossy().to_string();
        let path = if rel_prefix.is_empty() {
            basename.clone()
        } else {
            format!("{rel_prefix}/{basename}")
        };
        let kind = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            FileKind::Dir
        } else {
            FileKind::File
        };
        entries.push(FileEntry {
            name: basename,
            path,
            kind,
        });
    }

    // Directories first, then alphabetical case-insensitive — matches
    // file-explorer convention without surprising users with a custom
    // collation.
    entries.sort_by(|a, b| match (&a.kind, &b.kind) {
        (FileKind::Dir, FileKind::File) => std::cmp::Ordering::Less,
        (FileKind::File, FileKind::Dir) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

#[tauri::command]
pub async fn workspace_files(
    workspace_id: String,
    path: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<FileEntry>, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        workspace_files_inner(&workspace_id, path.as_deref(), state).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("files task join error: {e}"))?
}

// ── helpers ─────────────────────────────────────────────────────────

fn resolve_within_worktree(worktree: &Path, rel: &str) -> Result<PathBuf> {
    // Reject obviously-absolute inputs *before* the join. On Unix
    // `PathBuf::join("/etc")` resets to "/etc"; on Windows it instead
    // becomes `<drive>:/etc` which then fails canonicalize with
    // path-not-found, masking the real "outside worktree" intent. Catching
    // the leading slash + drive-letter shapes here keeps the error
    // message the same on every OS.
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err(AppError::Other(format!("path '{rel}' is outside worktree")));
    }
    if rel.len() >= 2 {
        let mut chars = rel.chars();
        let c0 = chars.next().unwrap();
        let c1 = chars.next().unwrap();
        if c0.is_ascii_alphabetic() && c1 == ':' {
            return Err(AppError::Other(format!("path '{rel}' is outside worktree")));
        }
    }
    let normalized = rel.replace('\\', "/");
    if normalized.split('/').any(|seg| seg == "..") {
        return Err(AppError::Other(format!("path '{rel}' is outside worktree")));
    }
    let joined = if normalized.is_empty() {
        worktree.to_path_buf()
    } else {
        worktree.join(&normalized)
    };
    let canonical_root = dunce::canonicalize(worktree)
        .map_err(|e| AppError::Other(format!("canonicalize worktree: {e}")))?;
    let canonical_joined =
        dunce::canonicalize(&joined).map_err(|_| AppError::PathNotFound(joined.clone()))?;
    if !canonical_joined.starts_with(&canonical_root) {
        return Err(AppError::Other(format!("path '{rel}' is outside worktree")));
    }
    Ok(canonical_joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
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

    #[test]
    fn workspace_files_inner_lists_root_children() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("a.txt"), b"a").unwrap();
        std::fs::write(wt.join("b.txt"), b"b").unwrap();
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        let state = make_state("ws_root", &wt);
        let entries = workspace_files_inner("ws_root", None, state).unwrap();
        assert_eq!(entries.len(), 3);
        // Dirs sort first.
        assert_eq!(entries[0].name, "sub");
        assert_eq!(entries[0].kind, FileKind::Dir);
        // Files alphabetical.
        assert_eq!(entries[1].name, "a.txt");
        assert_eq!(entries[2].name, "b.txt");
    }

    #[test]
    fn workspace_files_inner_lists_subdir_children() {
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        std::fs::write(wt.join("sub/c.txt"), b"c").unwrap();
        let state = make_state("ws_sub", &wt);
        let entries = workspace_files_inner("ws_sub", Some("sub"), state).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "c.txt");
        // Path is relative to worktree root, not to the subdir.
        assert_eq!(entries[0].path, "sub/c.txt");
    }

    #[test]
    fn workspace_files_inner_respects_gitignore() {
        let (_tmp, wt) = make_worktree();
        // `ignore` only honors gitignore inside an actual git repo, so
        // initialize one. The tree-walker is used for *unstarted*
        // workspaces too, so we also exercise the .gitignore-only path
        // via a separate test.
        let out = std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&wt)
            .output()
            .unwrap();
        assert!(out.status.success(), "git init failed");
        std::fs::write(wt.join(".gitignore"), b"target/\n").unwrap();
        std::fs::create_dir_all(wt.join("target")).unwrap();
        std::fs::write(wt.join("target/x.txt"), b"x").unwrap();
        std::fs::write(wt.join("a.txt"), b"a").unwrap();
        let state = make_state("ws_ignore", &wt);
        let entries = workspace_files_inner("ws_ignore", None, state).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&"target"), "target should be ignored");
        assert!(names.contains(&"a.txt"));
    }

    #[test]
    fn workspace_files_inner_does_not_recurse() {
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("a/b/c")).unwrap();
        std::fs::write(wt.join("a/b/c/deep.txt"), b"deep").unwrap();
        let state = make_state("ws_norec", &wt);
        let entries = workspace_files_inner("ws_norec", None, state).unwrap();
        // Only the top-level `a` directory — its children must come from a
        // separate call.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[0].kind, FileKind::Dir);
    }

    #[test]
    fn workspace_files_inner_rejects_path_traversal_dotdot() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_trav", &wt);
        let err = workspace_files_inner("ws_trav", Some("../etc"), state).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("outside worktree"),
            "expected 'outside worktree', got: {err}"
        );
    }

    #[test]
    fn workspace_files_inner_rejects_absolute_path() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_abs", &wt);
        let err = workspace_files_inner("ws_abs", Some("/etc"), state).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("outside worktree"),
            "expected 'outside worktree', got: {err}"
        );
    }

    #[test]
    fn workspace_files_inner_rejects_windows_drive_letter() {
        // Regression guard for the Windows CI failure: `C:\etc` would
        // canonicalize to a not-found path on Linux and a drive-relative
        // path on Windows, both of which masked the "outside worktree"
        // message. The drive-letter shape must be rejected up-front.
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_drive", &wt);
        let err = workspace_files_inner("ws_drive", Some("C:\\Windows"), state).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("outside worktree"),
            "expected 'outside worktree', got: {err}"
        );
    }

    #[test]
    fn workspace_files_inner_rejects_backslash_absolute() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_bs", &wt);
        let err = workspace_files_inner("ws_bs", Some("\\etc\\hosts"), state).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("outside worktree"),
            "expected 'outside worktree', got: {err}"
        );
    }

    #[test]
    fn workspace_files_inner_returns_error_for_invalid_workspace_id() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let err = workspace_files_inner("ws_missing", None, state).unwrap_err();
        assert!(err.to_string().contains("ws_missing"));
    }

    #[test]
    fn workspace_files_inner_returns_error_for_missing_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("never_created");
        let state = make_state("ws_gone", &wt);
        let err = workspace_files_inner("ws_gone", None, state).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("path") || msg.contains("not found"),
            "expected path-not-found-style error, got: {msg}"
        );
    }

    #[test]
    fn workspace_files_inner_target_must_be_directory() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("a.txt"), b"a").unwrap();
        let state = make_state("ws_file_target", &wt);
        let err = workspace_files_inner("ws_file_target", Some("a.txt"), state).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("not a directory"),
            "expected 'not a directory', got: {err}"
        );
    }

    #[test]
    fn workspace_files_inner_uses_forward_slash_in_returned_paths() {
        // Regression guard: even after the refactor that drops the
        // dedicated `to_forward_slash` helper, returned paths must use
        // `/` (not `\`) on every OS.
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("a")).unwrap();
        std::fs::write(wt.join("a/b.txt"), b"x").unwrap();
        let state = make_state("ws_slash", &wt);
        let entries = workspace_files_inner("ws_slash", Some("a"), state).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a/b.txt");
        assert!(!entries[0].path.contains('\\'));
    }
}
