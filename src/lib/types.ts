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
