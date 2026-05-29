// Phase 2a · Task 3 — `workspace_search` command.
//
// Streams search hits to the frontend over a `Channel<SearchHit>`. Two
// modes:
//
//   - filename: case-insensitive substring against the relative path,
//     gitignore-aware (via the `ignore` crate). Always available — no
//     external binaries required.
//
//   - content: shells out to `rg --json`. If `rg` is missing the command
//     emits a single `RipgrepUnavailable` sentinel followed by `Eof` so
//     the frontend can surface the "install rg" CTA without silently
//     degrading.
//
// Hit cap: 500 in either mode — keeps the modal responsive on huge repos
// and matches what korlap does.

use crate::error::{AppError, Result};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchHit {
    Filename {
        path: String,
    },
    Content {
        path: String,
        line_number: u32,
        line_text: String,
    },
    /// Emitted exactly once if the `rg` binary is not available.
    RipgrepUnavailable {
        reason: String,
    },
    /// Emitted exactly once at end-of-stream.
    Eof,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Filename,
    Content,
}

const HIT_CAP: usize = 500;

pub fn workspace_search_inner<F>(
    workspace_id: &str,
    query: &str,
    mode: SearchMode,
    rg_path: Option<&Path>,
    state: Arc<Mutex<AppState>>,
    mut emit: F,
) -> Result<()>
where
    F: FnMut(SearchHit),
{
    if query.trim().is_empty() {
        return Err(AppError::Other("search query is empty".into()));
    }

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

    match mode {
        SearchMode::Filename => search_filenames(&worktree_dir, query, &mut emit),
        SearchMode::Content => match rg_path {
            Some(rg) => search_content_with_rg(rg, &worktree_dir, query, &mut emit)?,
            None => emit(SearchHit::RipgrepUnavailable {
                reason: "ripgrep (`rg`) was not found in PATH".into(),
            }),
        },
    }

    emit(SearchHit::Eof);
    Ok(())
}

#[tauri::command]
pub async fn workspace_search(
    workspace_id: String,
    query: String,
    mode: SearchMode,
    channel: tauri::ipc::Channel<SearchHit>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let state = state.inner().clone();
    let rg_path = which::which("rg").ok();
    tokio::task::spawn_blocking(move || {
        let emit = |hit: SearchHit| {
            let _ = channel.send(hit);
        };
        workspace_search_inner(&workspace_id, &query, mode, rg_path.as_deref(), state, emit)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("search task join error: {e}"))?
}

// ── filename mode ───────────────────────────────────────────────────

fn search_filenames<F: FnMut(SearchHit)>(worktree: &Path, query: &str, emit: &mut F) {
    let needle = query.to_lowercase();
    let walker = ignore::WalkBuilder::new(worktree)
        .follow_links(false)
        .standard_filters(true)
        .build();
    let mut hits = 0usize;
    for entry in walker.flatten() {
        if hits >= HIT_CAP {
            break;
        }
        if entry.depth() == 0 {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            continue;
        }
        let rel = match entry.path().strip_prefix(worktree) {
            Ok(p) => p.to_path_buf(),
            Err(_) => continue,
        };
        let rel_str = path_to_unix(&rel);
        if rel_str.to_lowercase().contains(&needle) {
            emit(SearchHit::Filename { path: rel_str });
            hits += 1;
        }
    }
}

// ── content mode ────────────────────────────────────────────────────

fn search_content_with_rg<F: FnMut(SearchHit)>(
    rg: &Path,
    worktree: &Path,
    query: &str,
    emit: &mut F,
) -> Result<()> {
    let mut child = Command::new(rg)
        .args([
            "--json",
            "--max-count",
            "100",
            "--max-filesize",
            "1M",
            "--",
            query,
        ])
        .current_dir(worktree)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Command {
            cmd: "rg".into(),
            msg: e.to_string(),
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Other("rg stdout missing".into()))?;
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stdout);
    let mut hits = 0usize;
    for line in reader.lines().map_while(std::result::Result::ok) {
        if hits >= HIT_CAP {
            break;
        }
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if parsed.get("type").and_then(|v| v.as_str()) != Some("match") {
            continue;
        }
        let data = match parsed.get("data") {
            Some(d) => d,
            None => continue,
        };
        let path = data
            .pointer("/path/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let line_text = data
            .pointer("/lines/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('\n')
            .to_string();
        let line_number = data
            .get("line_number")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if path.is_empty() {
            continue;
        }
        emit(SearchHit::Content {
            path,
            line_number,
            line_text,
        });
        hits += 1;
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

fn path_to_unix(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_state(id: &str, worktree: &Path) -> Arc<Mutex<AppState>> {
        let mut state = AppState::default();
        state.workspaces.insert(
            id.into(),
            WorkspaceInfo {
                id: id.into(),
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

    fn make_repo(files: &[(&str, &[u8])]) -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        // Initialize a git repo so `ignore` honors gitignore.
        let _ = std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&wt)
            .output()
            .unwrap();
        for (rel, content) in files {
            let abs = wt.join(rel);
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(abs, content).unwrap();
        }
        (tmp, wt)
    }

    fn collect<F>(run: F) -> Vec<SearchHit>
    where
        F: FnOnce(&mut dyn FnMut(SearchHit)) -> Result<()>,
    {
        let hits = Arc::new(Mutex::new(Vec::new()));
        let hits_clone = Arc::clone(&hits);
        let mut emit = move |h: SearchHit| {
            hits_clone.lock().unwrap().push(h);
        };
        run(&mut emit).unwrap();
        let v = hits.lock().unwrap().clone();
        v
    }

    // ── filename mode ────────────────────────────────────────────────

    #[test]
    fn workspace_search_inner_filename_finds_substring() {
        let (_tmp, wt) = make_repo(&[("foo/bar.ts", b"x"), ("baz.md", b"y")]);
        let state = make_state("ws_fn", &wt);
        let hits = collect(|emit| {
            workspace_search_inner(
                "ws_fn",
                "bar",
                SearchMode::Filename,
                None,
                Arc::clone(&state),
                emit,
            )
        });
        let filenames: Vec<_> = hits
            .iter()
            .filter_map(|h| match h {
                SearchHit::Filename { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(filenames, vec!["foo/bar.ts".to_string()]);
        assert_eq!(hits.last(), Some(&SearchHit::Eof));
    }

    #[test]
    fn workspace_search_inner_filename_is_case_insensitive() {
        let (_tmp, wt) = make_repo(&[("README.md", b"x"), ("note.txt", b"y")]);
        let state = make_state("ws_case", &wt);
        let hits = collect(|emit| {
            workspace_search_inner(
                "ws_case",
                "readme",
                SearchMode::Filename,
                None,
                Arc::clone(&state),
                emit,
            )
        });
        let filenames: Vec<_> = hits
            .iter()
            .filter_map(|h| match h {
                SearchHit::Filename { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(filenames, vec!["README.md".to_string()]);
    }

    #[test]
    fn workspace_search_inner_filename_respects_gitignore() {
        let (_tmp, wt) = make_repo(&[
            (".gitignore", b"target/\n"),
            ("target/x.txt", b"a"),
            ("src/x.txt", b"b"),
        ]);
        let state = make_state("ws_ign", &wt);
        let hits = collect(|emit| {
            workspace_search_inner(
                "ws_ign",
                "x.txt",
                SearchMode::Filename,
                None,
                Arc::clone(&state),
                emit,
            )
        });
        let filenames: Vec<_> = hits
            .iter()
            .filter_map(|h| match h {
                SearchHit::Filename { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        // gitignore filtering — `target/x.txt` is excluded.
        assert_eq!(filenames, vec!["src/x.txt".to_string()]);
    }

    #[test]
    fn workspace_search_inner_filename_caps_hits_at_hit_cap() {
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..(HIT_CAP + 50) {
            files.push((format!("hit_{i}.txt"), b"x".to_vec()));
        }
        let refs: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_slice()))
            .collect();
        let (_tmp, wt) = make_repo(&refs);
        let state = make_state("ws_cap", &wt);
        let hits = collect(|emit| {
            workspace_search_inner(
                "ws_cap",
                "hit_",
                SearchMode::Filename,
                None,
                Arc::clone(&state),
                emit,
            )
        });
        let filenames: Vec<_> = hits
            .iter()
            .filter(|h| matches!(h, SearchHit::Filename { .. }))
            .collect();
        assert_eq!(filenames.len(), HIT_CAP);
        assert_eq!(hits.last(), Some(&SearchHit::Eof));
    }

    // ── content mode ─────────────────────────────────────────────────

    #[test]
    fn workspace_search_inner_content_emits_unavailable_when_rg_missing() {
        let (_tmp, wt) = make_repo(&[("a.txt", b"hello world\n")]);
        let state = make_state("ws_no_rg", &wt);
        let hits = collect(|emit| {
            workspace_search_inner(
                "ws_no_rg",
                "hello",
                SearchMode::Content,
                None,
                Arc::clone(&state),
                emit,
            )
        });
        // First hit is the unavailable sentinel; second is Eof.
        assert_eq!(hits.len(), 2);
        assert!(matches!(hits[0], SearchHit::RipgrepUnavailable { .. }));
        assert_eq!(hits[1], SearchHit::Eof);
    }

    #[test]
    fn workspace_search_inner_content_finds_match_when_rg_present() {
        let rg = match which::which("rg") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skipping content-mode rg test — rg not on PATH");
                return;
            }
        };
        let (_tmp, wt) = make_repo(&[("a.txt", b"alpha\nbeta gamma\n"), ("b.txt", b"unrelated\n")]);
        let state = make_state("ws_rg", &wt);
        let hits = collect(|emit| {
            workspace_search_inner(
                "ws_rg",
                "gamma",
                SearchMode::Content,
                Some(rg.as_path()),
                Arc::clone(&state),
                emit,
            )
        });
        let contents: Vec<_> = hits
            .iter()
            .filter_map(|h| match h {
                SearchHit::Content {
                    path,
                    line_number,
                    line_text,
                } => Some((path.clone(), *line_number, line_text.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(contents.len(), 1, "got hits: {hits:?}");
        let (path, line, text) = &contents[0];
        assert!(path.ends_with("a.txt"), "expected a.txt match, got {path}");
        assert_eq!(*line, 2);
        assert!(text.contains("gamma"), "expected 'gamma' in line: {text}");
        assert_eq!(hits.last(), Some(&SearchHit::Eof));
    }

    // ── error paths ──────────────────────────────────────────────────

    #[test]
    fn workspace_search_inner_returns_error_for_empty_query() {
        let (_tmp, wt) = make_repo(&[("a.txt", b"x")]);
        let state = make_state("ws_empty", &wt);
        let err =
            workspace_search_inner("ws_empty", "   ", SearchMode::Filename, None, state, |_| {})
                .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("empty"),
            "expected 'empty' in error, got: {err}"
        );
    }

    #[test]
    fn workspace_search_inner_returns_error_for_invalid_workspace_id() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let err =
            workspace_search_inner("ws_missing", "x", SearchMode::Filename, None, state, |_| {})
                .unwrap_err();
        assert!(err.to_string().contains("ws_missing"));
    }

    #[test]
    fn workspace_search_inner_returns_error_for_missing_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("nope");
        let state = make_state("ws_gone", &wt);
        let err = workspace_search_inner("ws_gone", "x", SearchMode::Filename, None, state, |_| {})
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("path") || msg.contains("not found"),
            "expected path-not-found-style error, got: {msg}"
        );
    }
}
