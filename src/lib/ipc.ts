// src/lib/ipc.ts
import { invoke } from '@tauri-apps/api/core';
import type { Repo, Workspace, CreateWorkspaceArgs } from './types';

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
};
