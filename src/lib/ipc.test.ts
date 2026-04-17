import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock @tauri-apps/api/core before importing ipc
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { api } from './ipc';
import type { Repo, Workspace } from './types';
import type { Task, CreateTaskArgs, TaskPatch } from './types';

const mockRepo: Repo = {
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
};

const mockWorkspace: Workspace = {
  id: 'ws_abc123',
  repo_id: 'repo_abc123',
  branch: 'feat/task-1',
  base_branch: 'main',
  custom_branch: false,
  title: 'Fix login',
  description: 'Fixing the login bug',
  status: 'not_started',
  column: 'todo',
  created_at: 1776000000,
  updated_at: 1776000000,
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('api.system.getAppVersion', () => {
  it('calls invoke with get_app_version and no args', async () => {
    vi.mocked(invoke).mockResolvedValue('0.1.0-pre');
    const v = await api.system.getAppVersion();
    expect(invoke).toHaveBeenCalledWith('get_app_version');
    expect(v).toBe('0.1.0-pre');
  });

  it('rejects on backend error', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('boom'));
    await expect(api.system.getAppVersion()).rejects.toThrow('boom');
  });
});

describe('api.repo', () => {
  it('add: invokes add_repo with path and returns Repo', async () => {
    vi.mocked(invoke).mockResolvedValue(mockRepo);
    const result = await api.repo.add('/home/user/my-project');
    expect(invoke).toHaveBeenCalledWith('add_repo', {
      path: '/home/user/my-project',
    });
    expect(result).toEqual(mockRepo);
  });

  it('add: propagates rejection from invoke', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Not a git repo'));
    await expect(api.repo.add('/tmp/not-git')).rejects.toThrow('Not a git repo');
  });

  it('list: invokes list_repos and returns Repo[]', async () => {
    vi.mocked(invoke).mockResolvedValue([mockRepo]);
    const result = await api.repo.list();
    expect(invoke).toHaveBeenCalledWith('list_repos');
    expect(result).toEqual([mockRepo]);
  });

  it('list: returns empty array when no repos', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const result = await api.repo.list();
    expect(result).toEqual([]);
  });

  it('remove: invokes remove_repo with repoId', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.repo.remove('repo_abc123');
    expect(invoke).toHaveBeenCalledWith('remove_repo', {
      repoId: 'repo_abc123',
    });
  });

  it('remove: propagates rejection when repo not found', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Not found'));
    await expect(api.repo.remove('repo_missing')).rejects.toThrow('Not found');
  });

  it('updateGhProfile: invokes update_repo_gh_profile with args', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.repo.updateGhProfile('repo_abc123', 'handokoben');
    expect(invoke).toHaveBeenCalledWith('update_repo_gh_profile', {
      repoId: 'repo_abc123',
      ghProfile: 'handokoben',
    });
  });

  it('updateGhProfile: accepts null to clear gh_profile', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.repo.updateGhProfile('repo_abc123', null);
    expect(invoke).toHaveBeenCalledWith('update_repo_gh_profile', {
      repoId: 'repo_abc123',
      ghProfile: null,
    });
  });
});

describe('api.workspace', () => {
  it('create: invokes create_workspace with args and returns Workspace', async () => {
    vi.mocked(invoke).mockResolvedValue(mockWorkspace);
    const args = {
      repoId: 'repo_abc123',
      title: 'Fix login',
      description: 'Fixing the login bug',
    };
    const result = await api.workspace.create(args);
    expect(invoke).toHaveBeenCalledWith('create_workspace', args);
    expect(result).toEqual(mockWorkspace);
  });

  it('create: forwards optional branchName', async () => {
    vi.mocked(invoke).mockResolvedValue(mockWorkspace);
    const args = {
      repoId: 'repo_abc123',
      title: 'Fix login',
      description: '',
      branchName: 'custom/branch',
    };
    await api.workspace.create(args);
    expect(invoke).toHaveBeenCalledWith('create_workspace', args);
  });

  it('list: invokes list_workspaces with no args when repoId omitted', async () => {
    vi.mocked(invoke).mockResolvedValue([mockWorkspace]);
    const result = await api.workspace.list();
    expect(invoke).toHaveBeenCalledWith('list_workspaces', {
      repoId: undefined,
    });
    expect(result).toEqual([mockWorkspace]);
  });

  it('list: invokes list_workspaces with repoId filter', async () => {
    vi.mocked(invoke).mockResolvedValue([mockWorkspace]);
    await api.workspace.list('repo_abc123');
    expect(invoke).toHaveBeenCalledWith('list_workspaces', {
      repoId: 'repo_abc123',
    });
  });

  it('remove: invokes remove_workspace with workspaceId', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.workspace.remove('ws_abc123');
    expect(invoke).toHaveBeenCalledWith('remove_workspace', {
      workspaceId: 'ws_abc123',
    });
  });

  it('remove: propagates rejection when workspace not found', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Workspace not found'));
    await expect(api.workspace.remove('ws_missing')).rejects.toThrow('Workspace not found');
  });
});

const mockTask: Task = {
  id: 'tk_abc123',
  repo_id: 'repo_abc123',
  workspace_id: null,
  title: 'Fix login bug',
  description: 'Users cannot log in after password reset.',
  column: 'todo',
  order: 0,
  created_at: 1776000000,
  updated_at: 1776000000,
};

describe('api.task', () => {
  it('add: invokes add_task with args and returns Task', async () => {
    vi.mocked(invoke).mockResolvedValue(mockTask);
    const args: CreateTaskArgs = {
      repoId: 'repo_abc123',
      title: 'Fix login bug',
      description: 'Users cannot log in after password reset.',
    };
    const result = await api.task.add(args);
    expect(invoke).toHaveBeenCalledWith('add_task', args);
    expect(result).toEqual(mockTask);
  });

  it('add: forwards optional column', async () => {
    vi.mocked(invoke).mockResolvedValue(mockTask);
    const args: CreateTaskArgs = {
      repoId: 'repo_abc123',
      title: 'Fix login bug',
      description: '',
      column: 'in_progress',
    };
    await api.task.add(args);
    expect(invoke).toHaveBeenCalledWith('add_task', args);
  });

  it('add: propagates rejection from invoke', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Repo not found'));
    await expect(
      api.task.add({ repoId: 'repo_missing', title: 'T', description: '' })
    ).rejects.toThrow('Repo not found');
  });

  it('list: invokes list_tasks with repoId and returns Task[]', async () => {
    vi.mocked(invoke).mockResolvedValue([mockTask]);
    const result = await api.task.list('repo_abc123');
    expect(invoke).toHaveBeenCalledWith('list_tasks', {
      repoId: 'repo_abc123',
    });
    expect(result).toEqual([mockTask]);
  });

  it('list: returns empty array when no tasks', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const result = await api.task.list('repo_abc123');
    expect(result).toEqual([]);
  });

  it('update: invokes update_task with taskId and patch', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const patch: TaskPatch = { title: 'Updated title' };
    await api.task.update('tk_abc123', patch);
    expect(invoke).toHaveBeenCalledWith('update_task', {
      taskId: 'tk_abc123',
      patch,
    });
  });

  it('move: invokes move_task with taskId, column, order and returns updated task', async () => {
    const updated = {
      id: 'tk_abc123',
      repo_id: 'repo_abc',
      workspace_id: 'ws_xyz',
      title: 't',
      description: '',
      column: 'in_progress' as const,
      order: 1,
      created_at: 1,
      updated_at: 2,
    };
    vi.mocked(invoke).mockResolvedValue(updated);
    const result = await api.task.move('tk_abc123', 'in_progress', 1);
    expect(invoke).toHaveBeenCalledWith('move_task', {
      taskId: 'tk_abc123',
      column: 'in_progress',
      order: 1,
    });
    expect(result.workspace_id).toBe('ws_xyz');
  });

  it('move: propagates rejection (e.g. task not found)', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Task not found'));
    await expect(api.task.move('tk_missing', 'in_progress', 0)).rejects.toThrow('Task not found');
  });

  it('remove: invokes remove_task with taskId', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.task.remove('tk_abc123');
    expect(invoke).toHaveBeenCalledWith('remove_task', {
      taskId: 'tk_abc123',
    });
  });

  it('remove: forwards force flag', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.task.remove('tk_abc123', true);
    expect(invoke).toHaveBeenCalledWith('remove_task', {
      taskId: 'tk_abc123',
      force: true,
    });
  });
});
