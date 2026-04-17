use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    #[default]
    NotStarted,
    Running,
    Waiting,
    Done,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KanbanColumn {
    #[default]
    Todo,
    InProgress,
    Review,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppSettings {
    pub schema_version: u32,
    pub theme: String,
    pub selected_repo_id: Option<String>,
    pub selected_workspace_id: Option<String>,
    pub recent_repos: Vec<String>,
    pub window_width: u32,
    pub window_height: u32,
    pub onboarding_completed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: "warm-dark".into(),
            selected_repo_id: None,
            selected_workspace_id: None,
            recent_repos: Vec::new(),
            window_width: 1400,
            window_height: 900,
            onboarding_completed: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Task {
    pub id: String,                   // prefix `tk_`
    pub repo_id: String,              // owning repo
    pub workspace_id: Option<String>, // populated when moved to InProgress
    pub title: String,
    pub description: String,
    pub column: KanbanColumn, // reuses Phase 1a enum
    pub order: i32,           // within-column sort order (higher = top)
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: std::collections::HashMap<String, RepoInfo>,
    pub workspaces: std::collections::HashMap<String, WorkspaceInfo>,
    pub tasks: std::collections::HashMap<String, Task>, // NEW
    pub settings: AppSettings,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
    pub base_branch: String,
    pub custom_branch: bool,
    pub title: String,
    pub description: String,
    pub status: WorkspaceStatus,
    pub column: KanbanColumn,
    pub created_at: i64,
    pub updated_at: i64,
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
    fn app_settings_default_values() {
        let s = AppSettings::default();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.theme, "warm-dark");
        assert_eq!(s.selected_repo_id, None);
        assert_eq!(s.selected_workspace_id, None);
        assert!(s.recent_repos.is_empty());
        assert_eq!(s.window_width, 1400);
        assert_eq!(s.window_height, 900);
        assert!(!s.onboarding_completed);
    }

    #[test]
    fn app_settings_round_trips_json() {
        let s = AppSettings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn app_state_has_settings_field() {
        let state = AppState::default();
        assert_eq!(state.settings.schema_version, 1);
    }

    #[test]
    fn workspace_info_round_trips_json() {
        let ws = WorkspaceInfo {
            id: "ws_abc123".into(),
            repo_id: "repo_xyz".into(),
            branch: "ws/abc123".into(),
            base_branch: "main".into(),
            custom_branch: false,
            title: "Fix login bug".into(),
            description: "Broken on mobile".into(),
            status: WorkspaceStatus::Waiting,
            column: KanbanColumn::InProgress,
            created_at: 1_776_000_000,
            updated_at: 1_776_099_500,
        };
        let json = serde_json::to_string(&ws).unwrap();
        let back: WorkspaceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ws);
    }

    #[test]
    fn workspace_info_status_is_not_started_by_default() {
        // Verify Default derive would give NotStarted / Todo if we could use it
        // (WorkspaceInfo doesn't derive Default, but status field default is)
        assert_eq!(WorkspaceStatus::default(), WorkspaceStatus::NotStarted);
        assert_eq!(KanbanColumn::default(), KanbanColumn::Todo);
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

    #[test]
    fn task_round_trips_json() {
        let t = Task {
            id: "tk_abc123".into(),
            repo_id: "repo_xyz".into(),
            workspace_id: None,
            title: "Fix login bug".into(),
            description: "Auth fails on mobile".into(),
            column: KanbanColumn::Todo,
            order: 1024,
            created_at: 1_776_000_000,
            updated_at: 1_776_099_000,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn task_workspace_id_nullable() {
        let t = Task {
            id: "tk_aaa111".into(),
            repo_id: "repo_r1".into(),
            workspace_id: Some("ws_xyz".into()),
            title: "With workspace".into(),
            description: String::new(),
            column: KanbanColumn::InProgress,
            order: 2048,
            created_at: 0,
            updated_at: 0,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"workspace_id\":\"ws_xyz\""));
        let none_task = Task {
            workspace_id: None,
            id: "tk_bbb222".into(),
            repo_id: "repo_r2".into(),
            title: String::new(),
            description: String::new(),
            column: KanbanColumn::Todo,
            order: 0,
            created_at: 0,
            updated_at: 0,
        };
        let none_json = serde_json::to_string(&none_task).unwrap();
        assert!(none_json.contains("\"workspace_id\":null"));
    }

    #[test]
    fn app_state_has_tasks_field() {
        let state = AppState::default();
        assert!(state.tasks.is_empty());
    }

    #[test]
    fn task_column_uses_kanban_column_enum() {
        let t = Task {
            id: "tk_c1".into(),
            repo_id: "repo_r1".into(),
            workspace_id: None,
            title: "Review task".into(),
            description: String::new(),
            column: KanbanColumn::Review,
            order: 3072,
            created_at: 0,
            updated_at: 0,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"column\":\"review\""));
    }
}
