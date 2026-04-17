use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    NotStarted,
    Running,
    Waiting,
    Done,
    Error,
}

impl Default for WorkspaceStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KanbanColumn {
    Todo,
    InProgress,
    Review,
    Done,
}

impl Default for KanbanColumn {
    fn default() -> Self {
        Self::Todo
    }
}

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: HashMap<String, RepoInfo>,
    pub workspaces: HashMap<String, WorkspaceInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub path: std::path::PathBuf,
    pub gh_profile: Option<String>,
    pub default_branch: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
}

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_default_is_empty() {
        let s = AppState::default();
        assert!(s.repos.is_empty());
        assert!(s.workspaces.is_empty());
    }

    #[test]
    fn app_version_matches_cargo_pkg_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn repo_info_round_trips_json() {
        let r = RepoInfo {
            id: "repo_abc123".into(),
            name: "my-repo".into(),
            path: std::path::PathBuf::from("/home/user/my-repo"),
            gh_profile: Some("handokoben".into()),
            default_branch: "main".into(),
            created_at: 1_776_000_000,
            updated_at: 1_776_099_000,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: RepoInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn repo_info_gh_profile_nullable() {
        let r = RepoInfo {
            id: "repo_xyz".into(),
            name: "other".into(),
            path: std::path::PathBuf::from("/tmp/other"),
            gh_profile: None,
            default_branch: "main".into(),
            created_at: 0,
            updated_at: 0,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"gh_profile\":null"));
        let back: RepoInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.gh_profile, None);
    }

    #[test]
    fn workspace_status_default_is_not_started() {
        assert_eq!(WorkspaceStatus::default(), WorkspaceStatus::NotStarted);
    }

    #[test]
    fn kanban_column_default_is_todo() {
        assert_eq!(KanbanColumn::default(), KanbanColumn::Todo);
    }

    #[test]
    fn workspace_status_round_trips_json() {
        let cases = [
            (WorkspaceStatus::NotStarted, "\"not_started\""),
            (WorkspaceStatus::Running, "\"running\""),
            (WorkspaceStatus::Waiting, "\"waiting\""),
            (WorkspaceStatus::Done, "\"done\""),
            (WorkspaceStatus::Error, "\"error\""),
        ];
        for (variant, expected_json) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected_json);
            let back: WorkspaceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn kanban_column_round_trips_json() {
        let cases = [
            (KanbanColumn::Todo, "\"todo\""),
            (KanbanColumn::InProgress, "\"in_progress\""),
            (KanbanColumn::Review, "\"review\""),
            (KanbanColumn::Done, "\"done\""),
        ];
        for (variant, expected_json) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected_json);
            let back: KanbanColumn = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }
}
