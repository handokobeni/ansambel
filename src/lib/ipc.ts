// src/lib/ipc.ts
import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  Repo,
  WorkspaceInfo,
  CreateWorkspaceArgs,
  Task,
  CreateTaskArgs,
  TaskPatch,
  KanbanColumn,
  AgentEvent,
  AttachmentDraft,
  Message,
  DiffChunk,
  FileEntry,
  SearchHit,
  SearchMode,
  RepoScript,
  TerminalChunk,
  FileReadResponse,
  FileWriteResponse,
  LarkStatus,
  SetLarkCredentialsArgs,
  BitableBinding,
  ProposedMapping,
  BitableField,
  PersonOption,
  SingleSelectOption,
  TeamActivityConfig,
} from './types';

export type ListMessagesOpts = {
  limit?: number;
  beforeId?: string;
};

export const api = {
  system: {
    getAppVersion: (): Promise<string> => invoke('get_app_version'),
  },

  repo: {
    add: (path: string): Promise<Repo> => invoke('add_repo', { path }),

    list: (): Promise<Repo[]> => invoke('list_repos'),

    remove: (repoId: string): Promise<void> => invoke('remove_repo', { repoId }),

    updateGhProfile: (repoId: string, ghProfile: string | null): Promise<void> =>
      invoke('update_repo_gh_profile', { repoId, ghProfile }),
  },

  workspace: {
    create: (args: CreateWorkspaceArgs): Promise<WorkspaceInfo> => invoke('create_workspace', args),

    list: (repoId?: string): Promise<WorkspaceInfo[]> => invoke('list_workspaces', { repoId }),

    remove: (workspaceId: string): Promise<void> => invoke('remove_workspace', { workspaceId }),

    /** Phase 3a-3 Task 18: flip the per-workspace privacy flag.
     *
     *  On `isPrivate = true`, the team-activity publisher suppresses
     *  sensitive columns for this workspace and clears any previously
     *  published values on its next flush. The new value is persisted
     *  to `workspaces.json` so it survives app restart. */
    setTeamActivityPrivate: (workspaceId: string, isPrivate: boolean): Promise<void> =>
      invoke<void>('set_workspace_team_activity_private', { workspaceId, isPrivate }),

    /** Stream the unified diff for a workspace's worktree. The backend
     *  emits `text` chunks ≤ 64 KB, then a single `eof`. `error` is rare
     *  and surfaces as an inline banner in the DiffView. */
    diff: (workspaceId: string, channel: Channel<DiffChunk>): Promise<void> =>
      invoke('workspace_diff', { workspaceId, channel }),

    /** List immediate children of `path` (or the worktree root when `path`
     *  is omitted) — gitignore-aware, sorted directories-first. */
    files: (workspaceId: string, path?: string): Promise<FileEntry[]> =>
      invoke('workspace_files', { workspaceId, path }),

    /** List ALL files (not directories) under the worktree recursively,
     *  gitignore-aware, forward-slash normalized, capped at 5000. Used
     *  by the @-mention autocomplete in the chat input — fetch once per
     *  workspace, cache, filter client-side. */
    filesRecursive: (workspaceId: string): Promise<string[]> =>
      invoke('workspace_files_recursive', { workspaceId }),

    /** Stream filename-or-content hits. Emits a single
     *  `ripgrep_unavailable` sentinel when content mode is requested and
     *  the `rg` binary is missing. */
    search: (
      workspaceId: string,
      query: string,
      mode: SearchMode,
      channel: Channel<SearchHit>
    ): Promise<void> => invoke('workspace_search', { workspaceId, query, mode, channel }),
  },

  task: {
    add: (args: CreateTaskArgs): Promise<Task> => invoke('add_task', args),

    list: (repoId: string): Promise<Task[]> => invoke('list_tasks', { repoId }),

    update: (taskId: string, patch: TaskPatch): Promise<void> =>
      invoke('update_task', { taskId, patch }),

    move: (taskId: string, column: KanbanColumn, order: number): Promise<Task> =>
      invoke('move_task', { taskId, column, order }),

    remove: (taskId: string, force?: boolean): Promise<void> =>
      invoke('remove_task', { taskId, force }),

    /** Pull fresh tasks from the active provider into the backend
     *  mirror, then return them. Used by the manual Refresh button
     *  and the window-focus listener. */
    refresh: (repoId?: string): Promise<Task[]> => invoke('refresh_tasks', { repoId }),
  },

  agent: {
    spawn: (workspaceId: string, onEvent: Channel<AgentEvent>): Promise<void> =>
      invoke('spawn_agent', { workspaceId, onEvent }),

    send: (workspaceId: string, text: string, attachments?: AttachmentDraft[]): Promise<void> =>
      invoke('send_message', {
        workspaceId,
        text,
        // Backend's AttachmentInput uses camelCase via #[serde(rename_all = "camelCase")];
        // pass them through unchanged.
        attachments:
          attachments && attachments.length > 0
            ? attachments.map((a) => ({
                sourcePath: a.sourcePath,
                mediaType: a.mediaType,
                filename: a.filename,
              }))
            : null,
      }),

    stop: (workspaceId: string): Promise<void> => invoke('stop_agent', { workspaceId }),

    reattach: (workspaceId: string, onEvent: Channel<AgentEvent>): Promise<void> =>
      invoke('reattach_agent', { workspaceId, onEvent }),
  },

  messages: {
    list: (workspaceId: string, opts: ListMessagesOpts = {}): Promise<Message[]> =>
      invoke('list_messages', {
        workspaceId,
        limit: opts.limit ?? null,
        beforeId: opts.beforeId ?? null,
      }),
  },

  // ── Phase 2b: interactive surfaces ─────────────────────────────────

  terminal: {
    /** Spawn a per-workspace shell rooted at the worktree dir. Stream
     *  raw bytes (xterm.js needs them ANSI-intact) over the channel. */
    spawn: (
      workspaceId: string,
      channel: Channel<TerminalChunk>,
      cols?: number,
      rows?: number
    ): Promise<void> => invoke('terminal_spawn', { workspaceId, channel, cols, rows }),

    /** Push raw bytes (typically keystrokes) onto the terminal's stdin. */
    write: (workspaceId: string, bytes: number[]): Promise<void> =>
      invoke('terminal_write', { workspaceId, bytes }),

    /** Reflow the PTY to new dimensions. Backend clamps to [1, 1000]. */
    resize: (workspaceId: string, cols: number, rows: number): Promise<void> =>
      invoke('terminal_resize', { workspaceId, cols, rows }),

    /** Kill the workspace's terminal. Idempotent. */
    kill: (workspaceId: string): Promise<void> => invoke('terminal_kill', { workspaceId }),

    /** Subscribe a fresh channel to the existing broadcaster — used on
     *  workspace switch + back, the same shape as agent reattach. */
    reattach: (workspaceId: string, channel: Channel<TerminalChunk>): Promise<void> =>
      invoke('terminal_reattach', { workspaceId, channel }),
  },

  file: {
    /** Read a file's contents (worktree-relative path). Returns binary
     *  detection + sha1 for round-trip race-detect on save. */
    read: (workspaceId: string, path: string): Promise<FileReadResponse> =>
      invoke('file_read', { workspaceId, path }),

    /** Atomically write `content` to `path`. Pass back the `expected_sha1`
     *  from the last `read()` so the backend can reject when the file
     *  changed under us. Empty `expected_sha1` skips the check (new
     *  file). */
    write: (
      workspaceId: string,
      path: string,
      content: string,
      expectedSha1: string
    ): Promise<FileWriteResponse> =>
      invoke('file_write', { workspaceId, path, content, expectedSha1 }),
  },

  // ── Phase 3a: Lark Open Platform credentials ─────────────────────

  lark: {
    /** Persist the app credentials. `appSecret` lands in the OS
     *  keyring; the other fields (plus `baseUrl`) go into
     *  `lark_settings.json`. Returns the post-write status so the UI can
     *  re-render without a follow-up `getStatus()`. */
    setCredentials: (args: SetLarkCredentialsArgs): Promise<LarkStatus> =>
      invoke('set_lark_credentials', {
        appId: args.appId,
        appSecret: args.appSecret,
        baseUrl: args.baseUrl ?? null,
      }),

    /** Returns whether Lark is configured, plus the non-secret fields
     *  so the form can pre-populate. The `app_secret` itself never
     *  crosses IPC — only `has_secret: bool`. */
    getStatus: (): Promise<LarkStatus> => invoke('get_lark_status'),

    /** Round-trip the credentials by testing a connection to a specific
     *  Bitable table. Resolves on success, rejects with the Lark API
     *  error on failure. Never logs the resulting token. */
    testConnection: (appToken: string, tableId: string): Promise<void> =>
      invoke('test_lark_connection', { appToken, tableId }),

    /** Wipe both the keyring entry and the on-disk settings. Idempotent. */
    clear: (): Promise<void> => invoke('clear_lark_credentials'),

    // ── Phase 3a-3: per-repo Bitable bindings ────────────────────────

    /** Retrieve the Bitable binding for a specific repo, or null if none. */
    getRepoBinding: (repoId: string): Promise<BitableBinding | null> =>
      invoke('get_lark_repo_binding', { repoId }),

    /** Persist a Bitable binding for a repo. */
    setRepoBinding: (repoId: string, binding: BitableBinding): Promise<void> =>
      invoke('set_lark_repo_binding', { repoId, binding }),

    /** Remove the Bitable binding for a repo. Idempotent. */
    deleteRepoBinding: (repoId: string): Promise<void> =>
      invoke('delete_lark_repo_binding', { repoId }),

    /** Return all repo bindings as a map of repoId → BitableBinding. */
    listRepoBindings: (): Promise<Record<string, BitableBinding>> =>
      invoke('list_lark_repo_bindings'),

    /** Inspect a Bitable table and propose a FieldMapping + StatusValueMapping. */
    detectSchema: (appToken: string, tableId: string): Promise<ProposedMapping> =>
      invoke('detect_lark_schema', { appToken, tableId }),

    /** List all fields for a Bitable table. */
    listFields: (appToken: string, tableId: string): Promise<BitableField[]> =>
      invoke('list_lark_fields', { appToken, tableId }),

    /** Enumerate distinct persons present in a Person-type field across all
     *  records. Used by FilterBar to build a dropdown for type-11 conditions.
     *  Returns persons sorted by name ascending. */
    listPersonOptions: (
      appToken: string,
      tableId: string,
      fieldName: string
    ): Promise<PersonOption[]> =>
      invoke('list_lark_person_options', { appToken, tableId, fieldName }),

    /** Follow a Lookup (type 19) field's chain to its source SingleSelect
     *  and return the options. Returns [] when the field is not a Lookup,
     *  the source is not a SingleSelect, or options are absent — the
     *  FilterBar falls back to a text input in those cases. */
    listLookupOptions: (
      appToken: string,
      tableId: string,
      fieldId: string
    ): Promise<SingleSelectOption[]> =>
      invoke('list_lark_lookup_options', { appToken, tableId, fieldId }),
  },

  // ── Phase 3a-3 Task 15: Team activity publisher config ────────────

  teamActivity: {
    /** Returns the persisted publisher config, or `null` when no config
     *  exists or the config's `app_token` is empty (backend treats an
     *  empty token as "publisher disabled"). */
    get: (): Promise<TeamActivityConfig | null> =>
      invoke<TeamActivityConfig | null>('get_team_activity_config'),

    /** Persists the publisher config atomically.
     *
     *  Note: changing the config does NOT restart the running publisher —
     *  the new app_token / table_id / machine_label only takes effect on
     *  next app launch. See the backend TODO in
     *  `commands/team_activity.rs::set_team_activity_config_inner`. */
    set: (cfg: TeamActivityConfig): Promise<void> =>
      invoke<void>('set_team_activity_config', { cfg }),

    /** Idempotently provisions the 12 canonical team-activity columns on
     *  the user's Bitable. Returns the list of column names that were
     *  created — empty when the table is already fully provisioned. */
    setupTable: (appToken: string, tableId: string): Promise<string[]> =>
      invoke<string[]>('setup_team_activity_table', { appToken, tableId }),
  },

  script: {
    /** List the configured scripts for a repo. */
    list: (repoId: string): Promise<RepoScript[]> => invoke('script_list', { repoId }),

    /** Replace the repo's scripts atomically. Phase 8 settings UI uses
     *  this; Phase 2b ships read-only. */
    set: (repoId: string, scripts: RepoScript[]): Promise<void> =>
      invoke('script_set', { repoId, scripts }),

    /** Run a script via PTY rooted at the workspace's worktree. Output
     *  streams as TerminalChunks — the frontend feeds these into the
     *  same xterm.js buffer the interactive shell writes to. */
    run: (workspaceId: string, scriptId: string, channel: Channel<TerminalChunk>): Promise<void> =>
      invoke('script_run', { workspaceId, scriptId, channel }),
  },
};

export function agentChannel(): Channel<AgentEvent> {
  return new Channel<AgentEvent>();
}
