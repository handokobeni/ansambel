// src/lib/types.ts

export type WorkspaceStatus = 'not_started' | 'running' | 'waiting' | 'done' | 'error';

export type KanbanColumn = 'todo' | 'in_progress' | 'review' | 'done';

export type Repo = {
  id: string;
  name: string;
  path: string;
  gh_profile: string | null;
  default_branch: string;
  created_at: number;
  updated_at: number;
};

export type Workspace = {
  id: string;
  repo_id: string;
  branch: string;
  base_branch: string;
  custom_branch: boolean;
  title: string;
  description: string;
  status: WorkspaceStatus;
  column: KanbanColumn;
  created_at: number;
  updated_at: number;
};

export type AppSettings = {
  schema_version: number;
  theme: string;
  selected_repo_id: string | null;
  selected_workspace_id: string | null;
  recent_repos: string[];
  window_width: number;
  window_height: number;
  onboarding_completed: boolean;
};

export type CreateWorkspaceArgs = {
  repoId: string;
  title: string;
  description: string;
  branchName?: string;
};

// --- Task types (Phase 1b) ---

export type Task = {
  id: string; // tk_xxxxxx
  repo_id: string;
  workspace_id: string | null;
  title: string;
  description: string;
  column: KanbanColumn;
  order: number;
  created_at: number;
  updated_at: number;
};

export type CreateTaskArgs = {
  repoId: string;
  title: string;
  description: string;
  column?: KanbanColumn;
};

export type TaskPatch = {
  title?: string;
  description?: string;
  order?: number;
};

export type Mode = 'plan' | 'work';

// --- Agent types (Phase 1c) ---

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';

export type ToolUse = {
  id: string;
  name: string;
  input: unknown;
};

export type ToolResult = {
  tool_use_id: string;
  content: string;
  is_error: boolean;
};

export type AttachmentKind = 'image';

export type Attachment = {
  kind: AttachmentKind;
  /** MIME type (e.g. "image/png"). */
  media_type: string;
  /** Canonical path on disk under the app data dir, written by the
   *  backend after copying the source file. */
  path: string;
  /** Original basename, displayed in chips & alt text. */
  filename: string | null;
};

/** What the picker hands the IPC layer before the file is copied: source
 *  path on the user's filesystem plus its inferred MIME type. The backend
 *  copies the file and turns it into a full `Attachment` for persistence. */
export type AttachmentDraft = {
  sourcePath: string;
  mediaType: string;
  filename: string | null;
};

export type Message = {
  id: string;
  workspace_id: string;
  role: MessageRole;
  text: string;
  is_partial: boolean;
  tool_use: ToolUse | null;
  tool_result: ToolResult | null;
  created_at: number;
  /** Attached files (currently images only). Optional on the wire so old
   *  persisted records load without it. */
  attachments?: Attachment[];
  /** Full thinking-block text for `role='system'` messages emitted by the
   *  thinking event handler. The `text` field still carries a truncated
   *  preview so listings stay compact; the bubble component reaches into
   *  `thinking_text` to power the expandable `<details>` view. Optional
   *  because non-thinking system messages and pre-G10 persisted records
   *  don't carry it. */
  thinking_text?: string;
};

export type AgentStatus = 'running' | 'waiting' | 'error' | 'stopped';

// WorkspaceInfo extends Workspace with the resolved worktree directory path.
export type WorkspaceInfo = Workspace & {
  worktree_dir: string;
};

export type AgentEvent =
  | { type: 'init'; session_id: string; model: string }
  | {
      type: 'message';
      id: string;
      role: MessageRole;
      text: string;
      is_partial: boolean;
    }
  | { type: 'tool_use'; message_id: string; tool_use: ToolUse }
  | { type: 'tool_result'; message_id: string; tool_result: ToolResult }
  | { type: 'status'; status: AgentStatus }
  | { type: 'error'; message: string }
  | { type: 'compact'; trigger: string; pre_tokens: number | null }
  | { type: 'thinking'; message_id: string; text: string; is_partial: boolean }
  | {
      type: 'usage';
      message_id: string;
      input_tokens: number;
      cache_creation_input_tokens: number;
      cache_read_input_tokens: number;
      output_tokens: number;
      /** Sum of input + cache_creation + cache_read per project rule. */
      total_input: number;
    };

/** Live-turn telemetry surfaced above the input while the agent is running.
 *  Resets on every status:running edge so each turn shows its own elapsed
 *  time and cumulative token spend.
 *
 *  Split into three counters because Claude's cache_read tokens are
 *  re-read on every step — accumulating them into a single "input" total
 *  inflates the displayed figure by an order of magnitude on any
 *  multi-step turn. The status bar shows them separately so the user
 *  can see real new spend vs cache replays at a glance. */
// --- Phase 2a: Work Mode read-only surfaces ----------------------------

/** One streamed slice of a workspace diff. The frontend concatenates `text`
 *  from every `text` chunk into one buffer, then parses on `eof`. `error`
 *  is rare — non-zero exit from `git diff` — and is surfaced as an inline
 *  banner in the DiffView. */
export type DiffChunk =
  | { kind: 'text'; text: string }
  | { kind: 'error'; message: string }
  | { kind: 'eof' };

export type FileKind = 'file' | 'dir';

export type FileEntry = {
  /** Basename — the leaf name shown in the tree row. */
  name: string;
  /** Path relative to the worktree root, forward-slash separated on every OS. */
  path: string;
  kind: FileKind;
};

export type SearchMode = 'filename' | 'content';

export type SearchHit =
  | { kind: 'filename'; path: string }
  | { kind: 'content'; path: string; line_number: number; line_text: string }
  | { kind: 'ripgrep_unavailable'; reason: string }
  | { kind: 'eof' };

/** Tab id within the per-workspace tab strip. */
export type WorkspaceTabId = 'chat' | 'diff' | 'files';

export type TurnState = {
  startedAt: number;
  /** Cumulative `input_tokens + cache_creation_input_tokens` — bytes
   *  freshly fed into the model and billed at full input rate. */
  inputTokens: number;
  /** Cumulative `cache_read_input_tokens` — bytes re-read from prompt
   *  cache and billed at 10% of input rate. Repeats across every
   *  multi-step assistant message in a turn, so this can be much larger
   *  than `inputTokens` even for short prompts. */
  cachedTokens: number;
  /** Cumulative `output_tokens`. */
  outputTokens: number;
};
