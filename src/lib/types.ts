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
  /** Phase 3a-3 Task 18: per-workspace privacy toggle. When true, the
   *  team-activity publisher suppresses sensitive columns for this
   *  workspace (and clears any previously published values via the
   *  `private_lock` semantics on the Rust side). Optional so workspaces
   *  persisted before Task 18 deserialise without the field. */
  team_activity_private?: boolean;
  /** IDs of the cards linked to this workspace. Refcount = task_ids.length. */
  task_ids: string[];
};

/** Result of `api.task.unlinkFromWorkspace`. The backend returns `would_remove`
 *  when called in preview mode (`force=false`) and the unlink would trigger
 *  cleanup; the UI uses that to show the confirm modal before re-calling with
 *  `force=true`. `removed`/`unlinked` are returned by the actual execution. */
export type UnlinkResult =
  | { kind: 'unlinked' }
  | { kind: 'removed' }
  | { kind: 'would_remove'; workspace_title: string };

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
  /** Person-in-charge names resolved from the optional `pic` field on the
   *  repo's Lark binding. Empty array when no PIC field is mapped or the
   *  record has no assignees. Optional so older persisted records load. */
  pic_names?: string[];
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

/** Tab id within the per-workspace tab strip. Phase 2b adds 'editor'
 *  and 'terminal' to the read-only Phase 2a triple. */
export type WorkspaceTabId = 'chat' | 'diff' | 'files' | 'editor' | 'terminal';

// --- Phase 2b: interactive surfaces ----------------------------------

/** A user-configured script per repo. The Terminal tab's script picker
 *  reads this list; clicking a script invokes `script_run` which streams
 *  output as TerminalChunks into the same buffer the interactive shell
 *  writes to. */
export type RepoScript = {
  id: string;
  name: string;
  command: string;
};

/** Streamed terminal output. xterm.js-shaped: bytes (so ANSI escapes
 *  arrive intact, no UTF-8 round-trip) plus a single Exited terminator. */
export type TerminalChunk =
  | { kind: 'bytes'; bytes: number[] }
  | { kind: 'exited'; code: number | null };

/** Editor file_read response — content + binary detection + sha1 for
 *  race-detect on save. */
export type FileReadResponse = {
  content: string;
  is_binary: boolean;
  size: number;
  /** Frontend round-trips this back via `file_write({ expected_sha1 })`
   *  so the backend can reject when the file was changed under us. */
  sha1: string;
};

/** Editor file_write response — sha1 of the bytes that were written. */
export type FileWriteResponse = {
  sha1: string;
  size: number;
};

// --- Lark types (Phase 3a) ---

/** Configuration status returned by the backend. The actual app_secret
 *  never crosses the IPC boundary — `has_secret` only reports whether
 *  one is stored. */
export type LarkStatus = {
  configured: boolean;
  app_id: string | null;
  base_url: string;
  has_secret: boolean;
};

export type SetLarkCredentialsArgs = {
  appId: string;
  appSecret: string;
  baseUrl?: string;
};

export type FieldRef = {
  field_id: string;
  field_name: string;
};

export type FieldMapping = {
  title: FieldRef;
  description: FieldRef | null;
  status: FieldRef | null;
  order: FieldRef | null;
  /** Optional Person-type (Lark type 11) field whose resolved names are
   *  surfaced on task cards. Null when the user opted not to map a PIC
   *  field in the wizard. */
  pic: FieldRef | null;
};

export type KanbanColumnLiteral = 'todo' | 'in_progress' | 'review' | 'done';

export type StatusValueMapping = {
  entries: Record<string, KanbanColumnLiteral>;
  default_column: KanbanColumnLiteral;
};

export type FilterOperator =
  | 'is'
  | 'isNot'
  | 'contains'
  | 'doesNotContain'
  | 'isEmpty'
  | 'isNotEmpty'
  | 'isGreater'
  | 'isGreaterEqual'
  | 'isLess'
  | 'isLessEqual';

export type FilterConjunction = 'and' | 'or';

export type FilterCondition = {
  field_id: string;
  field_name: string;
  operator: FilterOperator;
  value: string[];
};

export type FilterSpec = {
  conjunction: FilterConjunction;
  conditions: FilterCondition[];
};

export type BitableBinding = {
  app_token: string;
  table_id: string;
  filters: FilterSpec;
  field_mapping: FieldMapping;
  status_value_mapping: StatusValueMapping;
  created_at: number;
  updated_at: number;
};

export type BitableField = {
  field_id: string;
  field_name: string;
  type: number;
  property?: { options?: { id: string; name: string }[] } | null;
  is_primary: boolean;
};

export type BitableOption = { id: string; name: string };

/** A person returned by list_lark_person_options. Used by FilterBar to
 *  populate the value picker for Person-type (type 11) fields. */
export type PersonOption = { open_id: string; name: string };

/** One option resolved by following a Lookup (type 19) field's chain to its
 *  source SingleSelect. Used by FilterBar to render a dropdown for Lookup
 *  conditions. The `option_id` is sent as the filter value — Lark stores
 *  Lookup values as option_ids in records. */
export type SingleSelectOption = { option_id: string; name: string };

export type ProposedMapping = {
  fields: BitableField[];
  suggested: FieldMapping;
  status_options: BitableOption[] | null;
  suggested_status_values: StatusValueMapping;
};

// --- Phase 3a-3 Task 15: Team activity publisher config ---

/** Persisted in `<data_dir>/team_activity_config.json`. Empty `app_token`
 *  is treated as "publisher disabled" by the backend persister and surfaces
 *  as `null` to the IPC caller. The Lark `app_id` / `app_secret` come from
 *  the existing global Lark credentials (`api.lark.setCredentials`). */
export type TeamActivityConfig = {
  app_token: string;
  table_id: string;
  /** User-editable display label, e.g., "handoko@laptop-1". */
  machine_label: string;
};

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

// ── Phase 3a-4: Team Activity reader ─────────────────────────────

/** One row of the Phase 3a-4 sidebar / mirror view, mirroring the Rust
 *  `TeamActivityRow` shape and the columns the Phase 3a-3 publisher
 *  writes. All-string fields default to `''` when missing on the wire;
 *  `last_activity_at` is epoch ms (`0` when missing). */
export type TeamActivityRow = {
  workspace_id: string;
  repo_remote_url: string;
  repo_display_name: string;
  task_title: string;
  assignee_machine: string;
  ansambel_status: string;
  last_activity_at: number; // epoch ms
  last_message_preview: string;
  branch_name: string;
  diff_summary: string;
  pr_url: string;
  private: boolean;
};

/** Tagged-enum mirror of the Rust `FetchResult`. The `kind` discriminator
 *  matches the Rust `#[serde(tag = "kind", rename_all = "snake_case")]`
 *  shape. */
export type FetchResult =
  | { kind: 'disabled' }
  | { kind: 'machine_label_empty' }
  | { kind: 'no_overlap_repos' }
  | { kind: 'rows'; rows: TeamActivityRow[] };
