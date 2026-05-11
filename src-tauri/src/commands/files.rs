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

// ── Phase 2c · workspace_files_recursive ────────────────────────────
//
// Returns ALL file paths (not directories) under the worktree in one
// call, gitignore-aware and `/`-normalized. Used by the @-mention
// autocomplete in the chat input: the frontend fetches once per
// workspace, caches, and filters client-side per keystroke. Hard cap
// to avoid pathological repos sending megabytes of paths over IPC.

/// Hard cap on returned paths. Exceeding this is a sign the worktree
/// has too many tracked files for an interactive autocomplete; we
/// truncate (no error) and the frontend shows a "limited results"
/// hint. 5000 is well past what fits in a usefully-rankable dropdown.
const RECURSIVE_HARD_CAP: usize = 5000;

pub fn workspace_files_recursive_inner(
    workspace_id: &str,
    state: Arc<Mutex<AppState>>,
) -> Result<Vec<String>> {
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

    let canonical_root = dunce::canonicalize(&worktree_dir)
        .map_err(|e| AppError::Other(format!("canonicalize worktree: {e}")))?;

    let mut paths: Vec<String> = Vec::new();
    let walker = ignore::WalkBuilder::new(&worktree_dir)
        .follow_links(false)
        .standard_filters(true)
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.depth() == 0 {
            continue;
        }
        // Files only — directories aren't @-mention targets.
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let abs = entry.path();
        let canonical = match dunce::canonicalize(abs) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canonical.starts_with(&canonical_root) {
            continue;
        }
        let rel = match canonical.strip_prefix(&canonical_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        paths.push(rel_str);
        if paths.len() >= RECURSIVE_HARD_CAP {
            break;
        }
    }

    // Alpha order so client-side ranking has a stable baseline for
    // ties.
    paths.sort_by_key(|a| a.to_lowercase());
    Ok(paths)
}

#[tauri::command]
pub async fn workspace_files_recursive(
    workspace_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<String>, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        workspace_files_recursive_inner(&workspace_id, state).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("recursive files task join error: {e}"))?
}

// ── helpers ─────────────────────────────────────────────────────────

pub(crate) fn resolve_within_worktree(worktree: &Path, rel: &str) -> Result<PathBuf> {
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

    // ── workspace_files_recursive ────────────────────────────────────

    #[test]
    fn workspace_files_recursive_inner_returns_nested_files() {
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("src/lib")).unwrap();
        std::fs::write(wt.join("README.md"), b"r").unwrap();
        std::fs::write(wt.join("src/main.rs"), b"m").unwrap();
        std::fs::write(wt.join("src/lib/util.rs"), b"u").unwrap();
        let state = make_state("ws_rec", &wt);
        let paths = workspace_files_recursive_inner("ws_rec", state).unwrap();
        assert!(paths.contains(&"README.md".to_string()));
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(paths.contains(&"src/lib/util.rs".to_string()));
        // 3 files, no directories included.
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn workspace_files_recursive_inner_respects_gitignore() {
        let (_tmp, wt) = make_worktree();
        // gitignore is only honored inside a git repo (ignore crate
        // semantic). Mirror the pattern used by workspace_files_inner.
        let out = std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&wt)
            .output()
            .unwrap();
        assert!(out.status.success(), "git init failed");
        std::fs::write(wt.join(".gitignore"), b"target/\nnode_modules/\n").unwrap();
        std::fs::create_dir_all(wt.join("target")).unwrap();
        std::fs::create_dir_all(wt.join("node_modules/foo")).unwrap();
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(wt.join("target/x.bin"), b"x").unwrap();
        std::fs::write(wt.join("node_modules/foo/lib.js"), b"j").unwrap();
        std::fs::write(wt.join("src/main.rs"), b"m").unwrap();
        let state = make_state("ws_recignore", &wt);
        let paths = workspace_files_recursive_inner("ws_recignore", state).unwrap();
        assert!(paths.iter().any(|p| p == "src/main.rs"));
        assert!(!paths.iter().any(|p| p.starts_with("target/")));
        assert!(!paths.iter().any(|p| p.starts_with("node_modules/")));
    }

    #[test]
    fn workspace_files_recursive_inner_uses_forward_slash_on_all_os() {
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("a/b/c")).unwrap();
        std::fs::write(wt.join("a/b/c/d.txt"), b"d").unwrap();
        let state = make_state("ws_recslash", &wt);
        let paths = workspace_files_recursive_inner("ws_recslash", state).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "a/b/c/d.txt");
        assert!(!paths[0].contains('\\'));
    }

    #[test]
    fn workspace_files_recursive_inner_returns_files_only_not_directories() {
        let (_tmp, wt) = make_worktree();
        std::fs::create_dir_all(wt.join("emptydir")).unwrap();
        std::fs::write(wt.join("file.txt"), b"f").unwrap();
        let state = make_state("ws_filesonly", &wt);
        let paths = workspace_files_recursive_inner("ws_filesonly", state).unwrap();
        assert_eq!(paths, vec!["file.txt".to_string()]);
    }

    #[test]
    fn workspace_files_recursive_inner_handles_empty_worktree() {
        let (_tmp, wt) = make_worktree();
        let state = make_state("ws_empty", &wt);
        let paths = workspace_files_recursive_inner("ws_empty", state).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn workspace_files_recursive_inner_returns_paths_sorted_case_insensitive() {
        let (_tmp, wt) = make_worktree();
        std::fs::write(wt.join("Zebra.md"), b"z").unwrap();
        std::fs::write(wt.join("apple.md"), b"a").unwrap();
        std::fs::write(wt.join("Banana.md"), b"b").unwrap();
        let state = make_state("ws_sort", &wt);
        let paths = workspace_files_recursive_inner("ws_sort", state).unwrap();
        assert_eq!(
            paths,
            vec![
                "apple.md".to_string(),
                "Banana.md".to_string(),
                "Zebra.md".to_string(),
            ]
        );
    }

    #[test]
    fn workspace_files_recursive_inner_returns_error_for_invalid_workspace_id() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let err = workspace_files_recursive_inner("ws_missing_rec", state).unwrap_err();
        assert!(err.to_string().contains("ws_missing_rec"));
    }

    #[test]
    fn workspace_files_recursive_inner_returns_error_for_missing_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("never_created");
        let state = make_state("ws_rec_gone", &wt);
        let err = workspace_files_recursive_inner("ws_rec_gone", state).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("path") || msg.contains("not found"),
            "expected path-not-found-style error, got: {msg}"
        );
    }

    #[test]
    fn workspace_files_recursive_inner_caps_results_at_hard_cap() {
        let (_tmp, wt) = make_worktree();
        // Create RECURSIVE_HARD_CAP + 10 files. Verify result is capped.
        for i in 0..(RECURSIVE_HARD_CAP + 10) {
            std::fs::write(wt.join(format!("f{i}.txt")), b"x").unwrap();
        }
        let state = make_state("ws_recap", &wt);
        let paths = workspace_files_recursive_inner("ws_recap", state).unwrap();
        assert_eq!(paths.len(), RECURSIVE_HARD_CAP);
    }
}
