use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// User-configured path to the Claude CLI binary; overrides PATH lookup when set.
    #[serde(default)]
    pub claude_binary_override: Option<PathBuf>,
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
            claude_binary_override: None,
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

/// Runtime-only handle to a spawned Claude agent process. Not persisted —
/// dies on app restart, so workspace status resets Running → Waiting.
#[derive(Debug)]
pub struct AgentHandle {
    pub workspace_id: String,
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub session_id: Option<String>,
}

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: std::collections::HashMap<String, RepoInfo>,
    pub workspaces: std::collections::HashMap<String, WorkspaceInfo>,
    pub tasks: std::collections::HashMap<String, Task>,
    pub agents: std::collections::HashMap<String, AgentHandle>, // runtime-only
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
    /// Absolute path to the git worktree checkout directory for this workspace.
    /// Defaults to empty path for backward compatibility with existing persisted data.
    #[serde(default)]
    pub worktree_dir: PathBuf,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    #[default]
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    pub workspace_id: String,
    pub role: MessageRole,
    pub text: String,
    pub is_partial: bool,
    pub tool_use: Option<ToolUse>,
    pub tool_result: Option<ToolResult>,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Waiting,
    Error,
    Stopped,
}

/// Streaming event from a running agent, sent over the Tauri Channel API.
/// All variants use struct form so JSON is uniform:
/// {"type":"status","status":"running"}, {"type":"error","message":"..."}.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentEvent {
    Init {
        session_id: String,
        model: String,
    },
    Message {
        id: String,
        role: MessageRole,
        text: String,
        is_partial: bool,
    },
    ToolUse {
        message_id: String,
        tool_use: ToolUse,
    },
    ToolResult {
        message_id: String,
        tool_result: ToolResult,
    },
    Status {
        status: AgentStatus,
    },
    Error {
        message: String,
    },
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
            worktree_dir: PathBuf::from("/data/workspaces/ws_abc123"),
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

    #[test]
    fn message_role_round_trips_json() {
        for (role, want) in [
            (MessageRole::User, "\"user\""),
            (MessageRole::Assistant, "\"assistant\""),
            (MessageRole::System, "\"system\""),
            (MessageRole::Tool, "\"tool\""),
        ] {
            let s = serde_json::to_string(&role).unwrap();
            assert_eq!(s, want, "role {role:?}");
        }
    }

    #[test]
    fn message_role_default_is_user() {
        assert_eq!(MessageRole::default(), MessageRole::User);
    }

    #[test]
    fn message_round_trips_json() {
        let m = Message {
            id: "msg_abc123".into(),
            workspace_id: "ws_xyz".into(),
            role: MessageRole::Assistant,
            text: "Hello world".into(),
            is_partial: false,
            tool_use: None,
            tool_result: None,
            created_at: 1_776_000_000,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn message_partial_flag_serializes() {
        let m = Message {
            id: "msg_p1".into(),
            workspace_id: "ws_a".into(),
            role: MessageRole::Assistant,
            text: "streaming...".into(),
            is_partial: true,
            tool_use: None,
            tool_result: None,
            created_at: 0,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"is_partial\":true"));
    }

    #[test]
    fn message_tool_use_optional() {
        let plain = Message {
            id: "msg_x".into(),
            workspace_id: "ws_a".into(),
            role: MessageRole::Assistant,
            text: "no tools".into(),
            is_partial: false,
            tool_use: None,
            tool_result: None,
            created_at: 0,
        };
        let json = serde_json::to_string(&plain).unwrap();
        assert!(json.contains("\"tool_use\":null"));
    }

    #[test]
    fn message_tool_use_round_trip() {
        let m = Message {
            id: "msg_t".into(),
            workspace_id: "ws_a".into(),
            role: MessageRole::Assistant,
            text: String::new(),
            is_partial: false,
            tool_use: Some(ToolUse {
                id: "toolu_01".into(),
                name: "Read".into(),
                input: serde_json::json!({"path": "/etc/hosts"}),
            }),
            tool_result: None,
            created_at: 0,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn message_tool_result_round_trip() {
        let m = Message {
            id: "msg_r".into(),
            workspace_id: "ws_a".into(),
            role: MessageRole::Tool,
            text: String::new(),
            is_partial: false,
            tool_use: None,
            tool_result: Some(ToolResult {
                tool_use_id: "toolu_01".into(),
                content: "127.0.0.1 localhost".into(),
                is_error: false,
            }),
            created_at: 0,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn message_role_lowercase_in_json() {
        let m = Message {
            id: "msg_r".into(),
            workspace_id: "ws_a".into(),
            role: MessageRole::User,
            text: "hi".into(),
            is_partial: false,
            tool_use: None,
            tool_result: None,
            created_at: 0,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"role\":\"user\""));
    }

    #[test]
    fn agent_status_round_trips_json() {
        for (s, want) in [
            (AgentStatus::Running, "\"running\""),
            (AgentStatus::Waiting, "\"waiting\""),
            (AgentStatus::Error, "\"error\""),
            (AgentStatus::Stopped, "\"stopped\""),
        ] {
            let j = serde_json::to_string(&s).unwrap();
            assert_eq!(j, want);
        }
    }

    #[test]
    fn agent_event_message_serializes_with_type_tag() {
        let ev = AgentEvent::Message {
            id: "msg_a".into(),
            role: MessageRole::Assistant,
            text: "Hi".into(),
            is_partial: true,
        };
        let j = serde_json::to_string(&ev).unwrap();
        assert!(j.contains("\"type\":\"message\""));
        assert!(j.contains("\"is_partial\":true"));
    }

    #[test]
    fn agent_event_status_serializes_with_type_tag() {
        let ev = AgentEvent::Status {
            status: AgentStatus::Running,
        };
        let j = serde_json::to_string(&ev).unwrap();
        assert!(j.contains("\"type\":\"status\""));
        assert!(j.contains("\"status\":\"running\""));
    }

    #[test]
    fn agent_event_error_serializes() {
        let ev = AgentEvent::Error {
            message: "spawn failed".into(),
        };
        let j = serde_json::to_string(&ev).unwrap();
        assert!(j.contains("\"type\":\"error\""));
        assert!(j.contains("\"message\":\"spawn failed\""));
    }

    #[test]
    fn agent_event_init_carries_session_id() {
        let ev = AgentEvent::Init {
            session_id: "ses_xyz".into(),
            model: "claude-sonnet-4-6".into(),
        };
        let j = serde_json::to_string(&ev).unwrap();
        assert!(j.contains("\"type\":\"init\""));
        assert!(j.contains("\"session_id\":\"ses_xyz\""));
    }

    #[test]
    fn app_state_has_agents_field() {
        let state = AppState::default();
        assert!(state.agents.is_empty());
    }

    #[test]
    fn app_state_construction_with_agents_compiles() {
        let _state = AppState {
            repos: std::collections::HashMap::new(),
            workspaces: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
            agents: std::collections::HashMap::new(),
            settings: AppSettings::default(),
        };
    }

    #[test]
    fn agent_handle_has_required_fields() {
        use tokio::sync::mpsc;
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let h = AgentHandle {
            workspace_id: "ws_xyz".into(),
            stdin_tx: tx,
            session_id: None,
        };
        assert_eq!(h.workspace_id, "ws_xyz");
        assert!(h.session_id.is_none());
    }
}
