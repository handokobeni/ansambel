// src/lib/ipc.ts
import { invoke } from '@tauri-apps/api/core';
import type {
  Repo,
  Workspace,
  CreateWorkspaceArgs,
  Task,
  CreateTaskArgs,
  TaskPatch,
  KanbanColumn,
} from './types';

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
    create: (args: CreateWorkspaceArgs): Promise<Workspace> => invoke('create_workspace', args),

    list: (repoId?: string): Promise<Workspace[]> => invoke('list_workspaces', { repoId }),

    remove: (workspaceId: string): Promise<void> => invoke('remove_workspace', { workspaceId }),
  },

  task: {
    add: (args: CreateTaskArgs): Promise<Task> => invoke('add_task', args),

    list: (repoId: string): Promise<Task[]> => invoke('list_tasks', { repoId }),

    update: (taskId: string, patch: TaskPatch): Promise<void> =>
      invoke('update_task', { taskId, patch }),

    move: (taskId: string, column: KanbanColumn, order: number): Promise<void> =>
      invoke('move_task', { taskId, column, order }),

    remove: (taskId: string, force?: boolean): Promise<void> =>
      invoke('remove_task', { taskId, force }),
  },
};
