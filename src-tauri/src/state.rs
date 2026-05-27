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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KanbanColumn {
    #[default]
    Todo,
    InProgress,
    Review,
    Done,
}

/// A reference to a Bitable field. `field_id` is the stable lookup key
/// (survives renames). `field_name` is cached for UI display; refreshed
/// lazily whenever we re-fetch the Bitable schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FieldRef {
    pub field_id: String,
    pub field_name: String,
}

/// Field mapping for one Bitable. Only `title` is required; everything
/// else has a runtime fallback so a partially-populated mapping still
/// produces usable tasks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FieldMapping {
    pub title: FieldRef,
    #[serde(default)]
    pub description: Option<FieldRef>,
    #[serde(default)]
    pub status: Option<FieldRef>,
    #[serde(default)]
    pub order: Option<FieldRef>,
    /// Optional Person-type (Lark type 11) field whose names are surfaced
    /// on task cards. `serde(default)` so bindings persisted before this
    /// field existed deserialize without migration.
    #[serde(default)]
    pub pic: Option<FieldRef>,
}

/// Maps Bitable status field values to kanban columns. Keys are
/// `option_id` for single-select fields or lowercased text values for
/// Text fields. `default_column` covers values not in `entries`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StatusValueMapping {
    #[serde(default)]
    pub entries: std::collections::HashMap<String, KanbanColumn>,
    #[serde(default)]
    pub default_column: KanbanColumn,
}

impl Default for StatusValueMapping {
    fn default() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            default_column: KanbanColumn::Todo,
        }
    }
}

/// Operator for a Bitable filter condition. Serializes to Lark's
/// `records/search` operator string (camelCase).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Is,
    IsNot,
    Contains,
    DoesNotContain,
    IsEmpty,
    IsNotEmpty,
    IsGreater,
    IsGreaterEqual,
    IsLess,
    IsLessEqual,
}

/// Conjunction joining multiple filter conditions (AND / OR).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterConjunction {
    #[default]
    And,
    Or,
}

/// One filter condition matching Lark `records/search` schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FilterCondition {
    /// Bitable field id (stable lookup key — survives renames).
    pub field_id: String,
    /// Cached field name (UI display + outgoing API body). Refreshed from
    /// the LarkProvider field cache before each send.
    pub field_name: String,
    pub operator: FilterOperator,
    /// Per-type value (string for text, option name(s) for select,
    /// ISO-8601 for date, number-as-string, email/display for person).
    /// Empty Vec for unary operators (`isEmpty` / `isNotEmpty`).
    pub value: Vec<String>,
}

/// Set of filter conditions joined by a single top-level conjunction.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FilterSpec {
    #[serde(default)]
    pub conjunction: FilterConjunction,
    #[serde(default)]
    pub conditions: Vec<FilterCondition>,
}

impl FilterSpec {
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
}

/// One repo's binding to a Bitable: which table, plus how to map its
/// fields and status options to Ansambel's task model.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BitableBinding {
    pub app_token: String,
    pub table_id: String,
    /// Optional filter applied at the Lark server side via the
    /// `records/search` endpoint. Empty (default) → fetch all records
    /// via the existing list endpoint.
    #[serde(default)]
    pub filters: FilterSpec,
    pub field_mapping: FieldMapping,
    #[serde(default)]
    pub status_value_mapping: StatusValueMapping,
    pub created_at: u64,
    pub updated_at: u64,
}

/// A person that appears in a Bitable Person-type field. Used by the
/// FilterBar to build a dropdown of real users instead of a free-text
/// input.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PersonOption {
    pub open_id: String,
    pub name: String,
}

/// One option resolved by following a Lookup field's chain to its source
/// SingleSelect field. Used by the FilterBar to render a dropdown for
/// Lookup (type 19) conditions instead of a free-text input.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SingleSelectOption {
    pub option_id: String,
    pub name: String,
}

/// What `BitableSchemaDetector::propose_mapping` returns to the wizard.
/// Carries the raw field list (for dropdown population), an auto-detected
/// initial guess at the mapping, and (when status is single-select) the
/// option list with a fuzzy-parsed initial value mapping.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProposedMapping {
    pub fields: Vec<crate::platform::lark_client::BitableField>,
    pub suggested: FieldMapping,
    #[serde(default)]
    pub status_options: Option<Vec<crate::platform::lark_client::BitableOption>>,
    #[serde(default)]
    pub suggested_status_values: StatusValueMapping,
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
    /// Person-in-charge names resolved from the optional `pic` field on the
    /// repo's Lark binding. Empty when no PIC field is mapped or the record
    /// has no assignees. `serde(default)` so older persisted tasks load.
    #[serde(default)]
    pub pic_names: Vec<String>,
}

/// Events published by various command handlers and consumed by the
/// team-activity state publisher (Phase 3a-3). Each variant carries the
/// `workspace_id` of the workspace whose state changed; the publisher
/// aggregates events per workspace, debounces, and upserts the matching
/// Bitable row.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceEvent {
    StatusChanged {
        workspace_id: String,
        new_status: WorkspaceStatus,
    },
    MessageAppended {
        workspace_id: String,
        role: String, // "user" | "assistant" | "system" | "tool"
        /// Already truncated to ≤400 chars at the emission site. The
        /// publisher's sanitiser runs the credential redaction pass.
        text_preview: String,
    },
    FileTouched {
        workspace_id: String,
    },
    /// Emitted after a successful PR-creation flow (`gh pr create` or
    /// equivalent). Phase 3a-3 Task 13 ships the
    /// `emit_pr_created` helper that constructs this variant but leaves
    /// the call site BLOCKED — no PR-creation Tauri handler exists in
    /// the app yet. When that handler lands, it should call
    /// `emit_pr_created(publisher_tx, workspace_id, url)` after the
    /// `gh pr create` invocation succeeds.
    PrCreated {
        workspace_id: String,
        url: String,
    },
    BranchChanged {
        workspace_id: String,
        branch_name: String,
    },
    DiffSummaryUpdated {
        workspace_id: String,
        summary: String,
    },
    PrivacyChanged {
        workspace_id: String,
        is_private: bool,
    },
}

impl WorkspaceEvent {
    pub fn workspace_id(&self) -> &str {
        match self {
            WorkspaceEvent::StatusChanged { workspace_id, .. }
            | WorkspaceEvent::MessageAppended { workspace_id, .. }
            | WorkspaceEvent::FileTouched { workspace_id }
            | WorkspaceEvent::PrCreated { workspace_id, .. }
            | WorkspaceEvent::BranchChanged { workspace_id, .. }
            | WorkspaceEvent::DiffSummaryUpdated { workspace_id, .. }
            | WorkspaceEvent::PrivacyChanged { workspace_id, .. } => workspace_id,
        }
    }
}

/// Broadcast-sender alias, registered as a separate Tauri-managed state so
/// command handlers can emit without holding the AppState lock. Created in
/// `lib.rs::run()` with capacity 256 (well above expected event rate).
pub type WorkspaceEventTx = std::sync::Arc<tokio::sync::broadcast::Sender<WorkspaceEvent>>;

/// Connection details for the team-activity Bitable (Phase 3a-3 publisher).
/// Stored in `<data_dir>/team_activity_config.json`. Reuses the global
/// Lark `app_id`/`app_secret` from `commands::lark_auth`, so this config
/// is just the table coordinates + machine label.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct TeamActivityConfig {
    pub app_token: String,
    pub table_id: String,
    /// User-editable display label, e.g., "handoko@laptop-1". Auto-filled
    /// on first launch from `$USER@$(hostname)`.
    pub machine_label: String,
}

/// One streamed slice of terminal output. Tagged so the frontend (and
/// future tests) can pattern-match without an extra discriminator.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalChunk {
    /// Raw bytes from PTY stdout. xterm.js wants bytes (not strings) so
    /// it can parse ANSI escape sequences without UTF-8 round-tripping.
    Bytes { bytes: Vec<u8> },
    /// PTY child has exited. The frontend renders an inline
    /// `[process exited with code N]` marker and stops accepting input.
    Exited { code: Option<i32> },
}

/// Runtime-only handle to a per-workspace terminal session. Mirrors the
/// `AgentHandle` shape — same broadcaster + cancel pattern so the
/// frontend can switch workspaces and reattach without losing buffer.
/// Not persisted; dies on app restart.
#[derive(Debug)]
pub struct TerminalHandle {
    pub workspace_id: String,
    /// Sends raw bytes (typically keystrokes) to the PTY's stdin writer
    /// thread. Bytes-typed because terminal input is opaque — even
    /// keystrokes can be ANSI escape sequences.
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Broadcast sender for terminal output. The reader thread emits
    /// into this; `terminal_spawn` and `terminal_reattach` both
    /// subscribe and forward chunks to a Tauri Channel. Buffer of 256
    /// matches `AgentHandle.event_tx`.
    pub event_tx: tokio::sync::broadcast::Sender<TerminalChunk>,
    /// Cancel signal for the reader thread.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// PTY master — kept around so `terminal_resize` and
    /// `terminal_kill` can call into it. Wrapped in a Mutex because the
    /// concrete PTY is `!Sync` (master is `Send` but not shared).
    /// Trait object so tests can inject `MockPty`.
    pub pty: std::sync::Arc<std::sync::Mutex<Box<dyn crate::platform::pty::Pty + Send>>>,
}

/// Runtime-only handle to a spawned Claude agent process. Not persisted —
/// dies on app restart, so workspace status resets Running → Waiting.
#[derive(Debug)]
pub struct AgentHandle {
    pub workspace_id: String,
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub session_id: Option<String>,
    /// Broadcast sender for agent events. The reader thread emits into
    /// this; spawn_agent and reattach_agent both subscribe and forward
    /// events to a Tauri Channel so the UI can re-attach when the user
    /// switches workspaces and back. Buffer of 256 absorbs partial-
    /// message bursts; slow consumers drop oldest with `Lagged`, which
    /// is acceptable for a UI that re-renders on the next message.
    pub event_tx: tokio::sync::broadcast::Sender<AgentEvent>,
    /// Cancel signal for the reader thread. `stop_agent` flips this to
    /// `true` before dropping the handle so the reader exits its loop
    /// even if EOF on stdout is slow to arrive (e.g. hung CLI child).
    /// Defense-in-depth — the dropped stdin_tx still closes the child's
    /// stdin which usually forces EOF.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Tauri-managed handle to the per-repo task providers. Lives separately
/// from AppState so async provider calls don't hold the AppState lock.
/// Keyed by repo_id; entries are inserted when a binding is activated
/// (Task 10). Repos without an entry fall back to LocalProvider at
/// call sites via `provider_for_repo`.
pub type RepoId = String;

pub type TaskProviderHandle = std::sync::Arc<
    tokio::sync::RwLock<
        std::collections::HashMap<RepoId, std::sync::Arc<dyn crate::task_provider::TaskProvider>>,
    >,
>;

/// Build a LocalProvider for `data_dir`. Used as the fallback when a
/// repo has no explicit entry in `TaskProviderHandle`.
pub fn make_default_local_provider(
    data_dir: &std::path::Path,
) -> std::sync::Arc<dyn crate::task_provider::TaskProvider> {
    std::sync::Arc::new(crate::task_provider::local::LocalProvider::new(
        data_dir.to_path_buf(),
    ))
}

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: std::collections::HashMap<String, RepoInfo>,
    pub workspaces: std::collections::HashMap<String, WorkspaceInfo>,
    pub tasks: std::collections::HashMap<String, Task>,
    pub agents: std::collections::HashMap<String, AgentHandle>, // runtime-only
    /// Per-workspace terminal sessions (Phase 2b). Runtime-only — dies
    /// on app restart. Keyed by workspace id.
    pub terminals: std::collections::HashMap<String, TerminalHandle>,
    pub settings: AppSettings,
}

/// A user-configured script per repo. The script runner picks one of
/// these and spawns the command via PTY rooted at the worktree dir.
/// Phase 2b ships read-only listing; the set command is wired so a
/// future settings UI (Phase 8) can mutate without further backend
/// work.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RepoScript {
    pub id: String,
    pub name: String,
    pub command: String,
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
    /// Per-repo scripts surfaced in the workspace Terminal tab. Empty
    /// for repos persisted before Phase 2b — the `#[serde(default)]`
    /// keeps older `repos.json` files loading without migration.
    #[serde(default)]
    pub scripts: Vec<RepoScript>,
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
    /// When true, the team-activity publisher (Phase 3a-3) suppresses
    /// emission of sensitive columns for this workspace and clears any
    /// previously published values via the `private_lock` semantics in
    /// `commands::team_activity::AggregatedState`. `serde(default)` so
    /// workspaces persisted before Task 18 deserialise without migration.
    #[serde(default)]
    pub team_activity_private: bool,
    /// Originating kanban task id when the workspace was auto-created by
    /// moving a card into In Progress. `serde(default)` → workspaces
    /// persisted before this change deserialise as `None`. Used to
    /// reattach a card to its existing workspace instead of creating a
    /// duplicate when the local `task.workspace_id` link is lost (e.g. a
    /// Lark refresh blanks it).
    #[serde(default)]
    pub task_id: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
}

/// File attached to a chat message. Stored alongside the Message in
/// messages.json. Files are copied into `<data_dir>/attachments/<ws>/<msg>/`
/// on send so the chat is self-contained even after the user moves or
/// deletes the original.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Attachment {
    pub kind: AttachmentKind,
    /// MIME type, e.g. "image/png". Pinned by the picker's filter list.
    pub media_type: String,
    /// Canonical path of the copied file under the app data dir.
    pub path: String,
    /// Original basename, kept for display purposes only.
    pub filename: Option<String>,
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
    /// Attached files (currently images only). Defaulted on deserialise so
    /// pre-attachment Message records on disk still load cleanly.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
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
// snake_case (NOT lowercase) — `lowercase` would serialise the `ToolUse`
// variant as `tooluse` without an underscore, and the TypeScript discriminant
// would silently miss every tool event coming over the channel. The
// `agent_event_wire_shape_*` tests below pin this so a future rename can't
// regress it.
#[serde(tag = "type", rename_all = "snake_case")]
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
    /// Conversation history was auto-compacted by Claude. Surfaced so the
    /// chat can show a thin marker between turns — without this the user
    /// loses track of why earlier context "disappeared" mid-thread.
    Compact {
        /// "auto" or "manual" per the CLI; passed through unchanged so the
        /// UI can distinguish user-triggered /compact from automatic ones.
        trigger: String,
        /// Pre-compact token count when the CLI reports it. Optional because
        /// the field has appeared and disappeared across CLI releases.
        pre_tokens: Option<u64>,
    },
    /// Extended-thinking content from an assistant turn. Treated separately
    /// from regular text because the chat renders it as a thin "Claude is
    /// thinking…" marker rather than a normal bubble — without this the
    /// user only sees long pauses while the model deliberates.
    Thinking {
        /// Owning assistant message id, so the UI can co-locate the marker
        /// with its turn.
        message_id: String,
        /// Full thinking text (or the partial accumulated so far when
        /// `is_partial` is true).
        text: String,
        /// True while the thinking block is still streaming. Mirrors the
        /// Message variant so the same bubble can update in place.
        is_partial: bool,
    },
    /// Per-message token usage as reported by Claude in the assistant line's
    /// `message.usage` block. Drives the live "Cooking… (Xs · ↓ Yk tokens)"
    /// indicator above the input. `total_input` sums the three input sources
    /// (input + cache_creation + cache_read) per the project's token rule.
    Usage {
        message_id: String,
        input_tokens: u64,
        cache_creation_input_tokens: u64,
        cache_read_input_tokens: u64,
        output_tokens: u64,
        total_input: u64,
    },
}

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AgentEvent variants serialise with their `type` field exactly matching
    /// the TypeScript discriminants in `src/lib/types.ts`. A mismatch here
    /// (e.g. lowercase vs snake_case) causes the JS dispatcher to silently
    /// drop events — the kind of bug that ships fine in tests and breaks in
    /// production. Each variant gets its own test rather than a table-driven
    /// loop so a regression names exactly which variant broke.
    fn type_field(value: &serde_json::Value) -> &str {
        value
            .as_object()
            .and_then(|m| m.get("type"))
            .and_then(|v| v.as_str())
            .expect("AgentEvent must serialise as an object with a `type` discriminant")
    }

    fn to_value(ev: AgentEvent) -> serde_json::Value {
        serde_json::to_value(&ev).expect("AgentEvent must serialise to JSON")
    }

    #[test]
    fn agent_event_wire_shape_init_is_init() {
        let v = to_value(AgentEvent::Init {
            session_id: "ses_a".into(),
            model: "claude-sonnet-4-6".into(),
        });
        assert_eq!(type_field(&v), "init");
    }

    #[test]
    fn agent_event_wire_shape_message_is_message() {
        let v = to_value(AgentEvent::Message {
            id: "msg_a".into(),
            role: MessageRole::Assistant,
            text: "hi".into(),
            is_partial: false,
        });
        assert_eq!(type_field(&v), "message");
    }

    #[test]
    fn agent_event_wire_shape_tool_use_is_tool_use_with_underscore() {
        // Regression: previously serialised as "tooluse" with no underscore
        // because `rename_all = "lowercase"` collapses the variant name.
        // Frontend expected `tool_use` and silently dropped every event.
        let v = to_value(AgentEvent::ToolUse {
            message_id: "msg_a".into(),
            tool_use: ToolUse {
                id: "toolu_a".into(),
                name: "Read".into(),
                input: serde_json::Value::Null,
            },
        });
        assert_eq!(type_field(&v), "tool_use");
        // Field names also matter — the frontend reads `message_id` and
        // `tool_use` keys. Pin them.
        assert!(v.get("message_id").is_some());
        assert!(v.get("tool_use").is_some());
    }

    #[test]
    fn agent_event_wire_shape_tool_result_is_tool_result_with_underscore() {
        let v = to_value(AgentEvent::ToolResult {
            message_id: "msg_a".into(),
            tool_result: ToolResult {
                tool_use_id: "toolu_a".into(),
                content: "ok".into(),
                is_error: false,
            },
        });
        assert_eq!(type_field(&v), "tool_result");
        assert!(v.get("tool_result").is_some());
    }

    #[test]
    fn agent_event_wire_shape_status_is_status() {
        let v = to_value(AgentEvent::Status {
            status: AgentStatus::Running,
        });
        assert_eq!(type_field(&v), "status");
    }

    #[test]
    fn agent_event_wire_shape_error_is_error() {
        let v = to_value(AgentEvent::Error {
            message: "boom".into(),
        });
        assert_eq!(type_field(&v), "error");
    }

    #[test]
    fn agent_event_wire_shape_compact_is_compact() {
        let v = to_value(AgentEvent::Compact {
            trigger: "auto".into(),
            pre_tokens: Some(45_000),
        });
        assert_eq!(type_field(&v), "compact");
        assert!(v.get("trigger").is_some());
        assert!(v.get("pre_tokens").is_some());
    }

    #[test]
    fn agent_event_wire_shape_thinking_is_thinking() {
        let v = to_value(AgentEvent::Thinking {
            message_id: "msg_a".into(),
            text: "considering".into(),
            is_partial: true,
        });
        assert_eq!(type_field(&v), "thinking");
        assert!(v.get("message_id").is_some());
        assert!(v.get("is_partial").is_some());
    }

    #[test]
    fn agent_event_wire_shape_usage_is_usage() {
        let v = to_value(AgentEvent::Usage {
            message_id: "msg_a".into(),
            input_tokens: 12,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 4500,
            output_tokens: 230,
            total_input: 4512,
        });
        assert_eq!(type_field(&v), "usage");
        assert_eq!(v.get("input_tokens").and_then(|x| x.as_u64()), Some(12));
        assert_eq!(
            v.get("cache_read_input_tokens").and_then(|x| x.as_u64()),
            Some(4500)
        );
        assert_eq!(v.get("output_tokens").and_then(|x| x.as_u64()), Some(230));
        assert_eq!(v.get("total_input").and_then(|x| x.as_u64()), Some(4512));
    }

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
            team_activity_private: false,
            task_id: None,
        };
        let json = serde_json::to_string(&ws).unwrap();
        let back: WorkspaceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ws);
    }

    #[test]
    fn workspace_info_team_activity_private_defaults_false_on_legacy_load() {
        // Workspaces persisted before Task 18 don't have the field. The
        // serde(default) attribute must let them deserialise cleanly with
        // `team_activity_private = false`.
        let legacy_json = r#"{
            "id": "ws_legacy",
            "repo_id": "repo_x",
            "branch": "main",
            "base_branch": "main",
            "custom_branch": false,
            "title": "old",
            "description": "",
            "status": "not_started",
            "column": "todo",
            "created_at": 0,
            "updated_at": 0
        }"#;
        let ws: WorkspaceInfo = serde_json::from_str(legacy_json).unwrap();
        assert!(!ws.team_activity_private);
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
            scripts: vec![RepoScript {
                id: "sc_test".into(),
                name: "Run tests".into(),
                command: "bun test".into(),
            }],
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
            scripts: Vec::new(),
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
            pic_names: Vec::new(),
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
            pic_names: Vec::new(),
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
            pic_names: Vec::new(),
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
            pic_names: Vec::new(),
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
            attachments: Vec::new(),
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
            attachments: Vec::new(),
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
            attachments: Vec::new(),
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
            attachments: Vec::new(),
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
            attachments: Vec::new(),
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
            attachments: Vec::new(),
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
            terminals: std::collections::HashMap::new(),
            settings: AppSettings::default(),
        };
    }

    #[test]
    fn terminal_chunk_bytes_serializes_with_kind_tag() {
        let c = TerminalChunk::Bytes {
            bytes: vec![0x68, 0x69],
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"kind\":\"bytes\""), "got: {json}");
        // bytes serialize as a JSON number array — preserves binary-safety
        // for ANSI escapes that aren't valid UTF-8.
        assert!(json.contains("[104,105]"), "got: {json}");
    }

    #[test]
    fn terminal_chunk_exited_serializes_with_kind_tag_and_optional_code() {
        let c = TerminalChunk::Exited { code: Some(0) };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"kind\":\"exited\""));
        assert!(json.contains("\"code\":0"));
        let c2 = TerminalChunk::Exited { code: None };
        let json2 = serde_json::to_string(&c2).unwrap();
        assert!(json2.contains("\"code\":null"));
    }

    #[test]
    fn agent_handle_has_required_fields() {
        use tokio::sync::{broadcast, mpsc};
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, _) = broadcast::channel::<AgentEvent>(64);
        let h = AgentHandle {
            workspace_id: "ws_xyz".into(),
            stdin_tx: tx,
            session_id: None,
            event_tx,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        assert_eq!(h.workspace_id, "ws_xyz");
        assert!(h.session_id.is_none());
        assert!(!h.cancel.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn agent_handle_event_broadcaster_delivers_to_multiple_subscribers() {
        let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(64);
        let mut sub_a = tx.subscribe();
        let mut sub_b = tx.subscribe();
        tx.send(AgentEvent::Status {
            status: AgentStatus::Running,
        })
        .unwrap();
        assert!(sub_a.try_recv().is_ok());
        assert!(sub_b.try_recv().is_ok());
    }

    #[test]
    fn filter_operator_serializes_as_lark_camel_case() {
        use serde_json::json;
        assert_eq!(
            serde_json::to_value(FilterOperator::Is).unwrap(),
            json!("is")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsNot).unwrap(),
            json!("isNot")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::Contains).unwrap(),
            json!("contains")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::DoesNotContain).unwrap(),
            json!("doesNotContain")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsEmpty).unwrap(),
            json!("isEmpty")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsNotEmpty).unwrap(),
            json!("isNotEmpty")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsGreater).unwrap(),
            json!("isGreater")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsGreaterEqual).unwrap(),
            json!("isGreaterEqual")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsLess).unwrap(),
            json!("isLess")
        );
        assert_eq!(
            serde_json::to_value(FilterOperator::IsLessEqual).unwrap(),
            json!("isLessEqual")
        );
    }

    #[test]
    fn filter_conjunction_default_is_and_lowercase() {
        use serde_json::json;
        assert_eq!(FilterConjunction::default(), FilterConjunction::And);
        assert_eq!(
            serde_json::to_value(FilterConjunction::And).unwrap(),
            json!("and")
        );
        assert_eq!(
            serde_json::to_value(FilterConjunction::Or).unwrap(),
            json!("or")
        );
    }

    #[test]
    fn filter_spec_default_is_and_with_empty_conditions() {
        let spec = FilterSpec::default();
        assert_eq!(spec.conjunction, FilterConjunction::And);
        assert!(spec.conditions.is_empty());
        assert!(spec.is_empty());
    }

    #[test]
    fn filter_spec_is_empty_false_when_has_condition() {
        let spec = FilterSpec {
            conjunction: FilterConjunction::And,
            conditions: vec![FilterCondition {
                field_id: "fld123".into(),
                field_name: "Status".into(),
                operator: FilterOperator::Is,
                value: vec!["Done".into()],
            }],
        };
        assert!(!spec.is_empty());
    }

    #[test]
    fn filter_spec_roundtrips_through_json() {
        let spec = FilterSpec {
            conjunction: FilterConjunction::Or,
            conditions: vec![
                FilterCondition {
                    field_id: "fld1".into(),
                    field_name: "Sprint".into(),
                    operator: FilterOperator::Is,
                    value: vec!["S1".into()],
                },
                FilterCondition {
                    field_id: "fld2".into(),
                    field_name: "Owner".into(),
                    operator: FilterOperator::IsEmpty,
                    value: vec![],
                },
            ],
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: FilterSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn legacy_binding_without_filters_loads_as_default_empty() {
        let legacy_json = r#"{
            "app_token": "appXYZ",
            "table_id": "tblABC",
            "field_mapping": {
                "title": { "field_id": "fld1", "field_name": "Title" }
            },
            "status_value_mapping": { "entries": {}, "default_column": "todo" },
            "created_at": 1700000000,
            "updated_at": 1700000000
        }"#;
        let binding: BitableBinding = serde_json::from_str(legacy_json).unwrap();
        assert!(binding.filters.is_empty());
        assert_eq!(binding.filters.conjunction, FilterConjunction::And);
    }

    #[test]
    fn binding_with_filters_roundtrips() {
        let binding = BitableBinding {
            app_token: "appXYZ".into(),
            table_id: "tblABC".into(),
            filters: FilterSpec {
                conjunction: FilterConjunction::And,
                conditions: vec![FilterCondition {
                    field_id: "fld1".into(),
                    field_name: "Status".into(),
                    operator: FilterOperator::Is,
                    value: vec!["Done".into()],
                }],
            },
            field_mapping: FieldMapping {
                title: FieldRef {
                    field_id: "fld1".into(),
                    field_name: "Title".into(),
                },
                description: None,
                status: None,
                order: None,
                pic: None,
            },
            status_value_mapping: StatusValueMapping::default(),
            created_at: 1700000000,
            updated_at: 1700000000,
        };
        let json = serde_json::to_string(&binding).unwrap();
        let back: BitableBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(binding, back);
    }

    #[test]
    fn workspace_info_task_id_defaults_to_none_for_legacy_json() {
        // Legacy workspaces.json predates task_id; it must deserialise as None.
        let legacy = r#"{
            "id": "ws_1", "repo_id": "repo_1", "branch": "ansambel/x",
            "base_branch": "main", "custom_branch": false, "title": "T",
            "description": "", "status": "not_started", "column": "todo",
            "created_at": 0, "updated_at": 0
        }"#;
        let ws: WorkspaceInfo = serde_json::from_str(legacy).unwrap();
        assert_eq!(ws.task_id, None);
    }

    #[test]
    fn workspace_info_round_trips_task_id() {
        let mut ws = WorkspaceInfo {
            id: "ws_2".into(),
            repo_id: "repo_1".into(),
            branch: "ansambel/y".into(),
            base_branch: "main".into(),
            custom_branch: false,
            title: "T".into(),
            description: String::new(),
            status: WorkspaceStatus::NotStarted,
            column: KanbanColumn::Todo,
            created_at: 0,
            updated_at: 0,
            worktree_dir: std::path::PathBuf::new(),
            team_activity_private: false,
            task_id: Some("tk_42".into()),
        };
        let json = serde_json::to_string(&ws).unwrap();
        ws = serde_json::from_str(&json).unwrap();
        assert_eq!(ws.task_id.as_deref(), Some("tk_42"));
    }

    #[tokio::test]
    async fn workspace_event_broadcasts_to_multiple_subscribers() {
        use tokio::sync::broadcast;
        let (tx, _) = broadcast::channel::<WorkspaceEvent>(32);
        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();
        let event = WorkspaceEvent::StatusChanged {
            workspace_id: "ws_test".into(),
            new_status: crate::state::WorkspaceStatus::Running,
        };
        tx.send(event.clone()).unwrap();
        assert_eq!(rx1.recv().await.unwrap(), event);
        assert_eq!(rx2.recv().await.unwrap(), event);
    }

    #[test]
    fn binding_serde_round_trip_preserves_fields() {
        let b = BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            filters: FilterSpec::default(),
            field_mapping: FieldMapping {
                title: FieldRef {
                    field_id: "fld_pri".into(),
                    field_name: "Task name".into(),
                },
                description: None,
                status: Some(FieldRef {
                    field_id: "fld_status".into(),
                    field_name: "Task Status".into(),
                }),
                order: None,
                pic: None,
            },
            status_value_mapping: StatusValueMapping {
                entries: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("opt_a".into(), KanbanColumn::Todo);
                    m.insert("opt_b".into(), KanbanColumn::Done);
                    m
                },
                default_column: KanbanColumn::Todo,
            },
            created_at: 1747200000,
            updated_at: 1747200000,
        };
        let json = serde_json::to_string(&b).unwrap();
        let parsed: BitableBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, b);
    }
}
