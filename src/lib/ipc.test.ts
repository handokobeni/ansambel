import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock @tauri-apps/api/core before importing ipc
vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    id = Math.random();
    onmessage?: (ev: unknown) => void;
  }

  return {
    invoke: vi.fn(),
    Channel: MockChannel,
  };
});

import { invoke } from '@tauri-apps/api/core';
import { api, agentChannel } from './ipc';
import type { Repo, WorkspaceInfo } from './types';
import type { Task, CreateTaskArgs, TaskPatch, BitableBinding } from './types';

const mockRepo: Repo = {
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
};

const mockWorkspace: WorkspaceInfo = {
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
  worktree_dir: '/tmp/ws_abc123',
  task_ids: [],
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

  // ── Task 18: setTeamActivityPrivate ─────────────────────────────────
  it('setTeamActivityPrivate: forwards camelCase args to backend', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.workspace.setTeamActivityPrivate('ws_abc123', true);
    expect(invoke).toHaveBeenCalledWith('set_workspace_team_activity_private', {
      workspaceId: 'ws_abc123',
      isPrivate: true,
    });
  });

  it('setTeamActivityPrivate: also accepts isPrivate=false', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.workspace.setTeamActivityPrivate('ws_abc123', false);
    expect(invoke).toHaveBeenCalledWith('set_workspace_team_activity_private', {
      workspaceId: 'ws_abc123',
      isPrivate: false,
    });
  });

  it('setTeamActivityPrivate: propagates rejection from backend', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('workspace not found'));
    await expect(api.workspace.setTeamActivityPrivate('ws_missing', true)).rejects.toThrow(
      'workspace not found'
    );
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

describe('api.task new wrappers', () => {
  it('refresh: invokes refresh_tasks with repoId', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await api.task.refresh('repo_a');
    expect(invoke).toHaveBeenCalledWith('refresh_tasks', { repoId: 'repo_a' });
  });

  it('refresh: invokes with undefined repoId when not provided', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await api.task.refresh();
    expect(invoke).toHaveBeenCalledWith('refresh_tasks', { repoId: undefined });
  });
});

describe('api.agent', () => {
  it('spawn passes workspaceId and channel to invoke', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const ch = agentChannel();
    await api.agent.spawn('ws_a', ch);
    expect(invoke).toHaveBeenCalledWith('spawn_agent', {
      workspaceId: 'ws_a',
      onEvent: ch,
    });
  });

  it('send passes workspaceId and text (no attachments → null)', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.agent.send('ws_a', 'Hello world');
    expect(invoke).toHaveBeenCalledWith('send_message', {
      workspaceId: 'ws_a',
      text: 'Hello world',
      attachments: null,
    });
  });

  it('send passes attachments mapped to camelCase backend shape', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.agent.send('ws_a', 'check', [
      { sourcePath: '/u/p.png', mediaType: 'image/png', filename: 'p.png' },
    ]);
    expect(invoke).toHaveBeenCalledWith('send_message', {
      workspaceId: 'ws_a',
      text: 'check',
      attachments: [{ sourcePath: '/u/p.png', mediaType: 'image/png', filename: 'p.png' }],
    });
  });

  it('send sends attachments=null when an empty array is provided', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.agent.send('ws_a', 'plain', []);
    expect(invoke).toHaveBeenCalledWith('send_message', {
      workspaceId: 'ws_a',
      text: 'plain',
      attachments: null,
    });
  });

  it('stop passes workspaceId only', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.agent.stop('ws_a');
    expect(invoke).toHaveBeenCalledWith('stop_agent', {
      workspaceId: 'ws_a',
    });
  });

  it('spawn rejects when invoke rejects', async () => {
    vi.mocked(invoke).mockRejectedValue('spawn failed');
    const ch = agentChannel();
    await expect(api.agent.spawn('ws_a', ch)).rejects.toBe('spawn failed');
  });

  it('send rejects when invoke rejects', async () => {
    vi.mocked(invoke).mockRejectedValue('no agent');
    await expect(api.agent.send('ws_a', 'hi')).rejects.toBe('no agent');
  });

  it('stop rejects when invoke rejects', async () => {
    vi.mocked(invoke).mockRejectedValue('agent already stopped');
    await expect(api.agent.stop('ws_a')).rejects.toBe('agent already stopped');
  });
});

describe('api.lark', () => {
  const mockStatus = {
    configured: true,
    app_id: 'cli_test',
    base_url: 'https://open.larksuite.com',
    has_secret: true,
  };

  it('setCredentials: forwards camelCase args and returns LarkStatus', async () => {
    vi.mocked(invoke).mockResolvedValue(mockStatus);
    const out = await api.lark.setCredentials({
      appId: 'cli_test',
      appSecret: 'shh',
    });
    expect(invoke).toHaveBeenCalledWith('set_lark_credentials', {
      appId: 'cli_test',
      appSecret: 'shh',
      baseUrl: null,
    });
    expect(out).toEqual(mockStatus);
  });

  it('setCredentials: forwards baseUrl when provided', async () => {
    vi.mocked(invoke).mockResolvedValue(mockStatus);
    await api.lark.setCredentials({
      appId: 'cli',
      appSecret: 's',
      baseUrl: 'https://open.feishu.cn',
    });
    expect(invoke).toHaveBeenCalledWith('set_lark_credentials', {
      appId: 'cli',
      appSecret: 's',
      baseUrl: 'https://open.feishu.cn',
    });
  });

  it('setCredentials: propagates rejection on validation error', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Lark API: app_id must not be empty'));
    await expect(api.lark.setCredentials({ appId: '', appSecret: 's' })).rejects.toThrow(
      'app_id must not be empty'
    );
  });

  it('getStatus: invokes get_lark_status with no args and returns LarkStatus', async () => {
    vi.mocked(invoke).mockResolvedValue(mockStatus);
    const out = await api.lark.getStatus();
    expect(invoke).toHaveBeenCalledWith('get_lark_status');
    expect(out).toEqual(mockStatus);
  });

  it('testConnection: invokes test_lark_connection with creds', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.lark.testConnection('bascn', 'tbl');
    expect(invoke).toHaveBeenCalledWith('test_lark_connection', {
      appToken: 'bascn',
      tableId: 'tbl',
    });
  });

  it('testConnection: rejects with Lark API error', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Lark API: tenant_access_token code 99991663'));
    await expect(api.lark.testConnection('bascn', 'tbl')).rejects.toThrow('99991663');
  });

  it('clear: invokes clear_lark_credentials with no args', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.lark.clear();
    expect(invoke).toHaveBeenCalledWith('clear_lark_credentials');
  });

  it('api.lark.getRepoBinding invokes get_lark_repo_binding with repoId', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await api.lark.getRepoBinding('repo_x');
    expect(invoke).toHaveBeenCalledWith('get_lark_repo_binding', {
      repoId: 'repo_x',
    });
  });

  it('api.lark.setRepoBinding passes binding payload', async () => {
    const binding: BitableBinding = {
      app_token: 'bascn',
      table_id: 'tbl',
      filters: { conjunction: 'and', conditions: [] },
      field_mapping: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
        pic: null,
      },
      status_value_mapping: { entries: {}, default_column: 'todo' },
      created_at: 0,
      updated_at: 0,
    };
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.lark.setRepoBinding('repo_x', binding);
    expect(invoke).toHaveBeenCalledWith('set_lark_repo_binding', {
      repoId: 'repo_x',
      binding,
    });
  });

  it('api.lark.deleteRepoBinding invokes delete_lark_repo_binding', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.lark.deleteRepoBinding('repo_x');
    expect(invoke).toHaveBeenCalledWith('delete_lark_repo_binding', {
      repoId: 'repo_x',
    });
  });

  it('api.lark.listRepoBindings invokes list_lark_repo_bindings', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await api.lark.listRepoBindings();
    expect(invoke).toHaveBeenCalledWith('list_lark_repo_bindings');
  });

  it('api.lark.detectSchema invokes detect_lark_schema with creds', async () => {
    vi.mocked(invoke).mockResolvedValue({
      fields: [],
      suggested: {
        title: { field_id: '', field_name: '' },
        description: null,
        status: null,
        order: null,
        pic: null,
      },
      status_options: null,
      suggested_status_values: { entries: {}, default_column: 'todo' },
    });
    await api.lark.detectSchema('bascn', 'tbl');
    expect(invoke).toHaveBeenCalledWith('detect_lark_schema', {
      appToken: 'bascn',
      tableId: 'tbl',
    });
  });
});

describe('api.lark.listFields', () => {
  it('invokes list_lark_fields with camelCase params and returns BitableField[]', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { field_id: 'fld1', field_name: 'Title', type: 1, is_primary: true, property: null },
    ]);
    const result = await api.lark.listFields('appA', 'tblA');
    expect(invoke).toHaveBeenCalledWith('list_lark_fields', {
      appToken: 'appA',
      tableId: 'tblA',
    });
    expect(result).toHaveLength(1);
    expect(result[0].field_name).toBe('Title');
  });
});

describe('api.teamActivity', () => {
  const mockCfg = {
    app_token: 'bascn_team',
    table_id: 'tbl_team',
    machine_label: 'handoko@laptop-1',
  };

  it('get: invokes get_team_activity_config and returns config', async () => {
    vi.mocked(invoke).mockResolvedValue(mockCfg);
    const out = await api.teamActivity.get();
    expect(invoke).toHaveBeenCalledWith('get_team_activity_config');
    expect(out).toEqual(mockCfg);
  });

  it('get: returns null when no config persisted', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    const out = await api.teamActivity.get();
    expect(out).toBeNull();
  });

  it('get: propagates rejection from backend', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('read fail'));
    await expect(api.teamActivity.get()).rejects.toThrow('read fail');
  });

  it('set: forwards cfg payload under the cfg key', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.teamActivity.set(mockCfg);
    expect(invoke).toHaveBeenCalledWith('set_team_activity_config', {
      cfg: mockCfg,
    });
  });

  it('set: propagates rejection on disk error', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('write fail'));
    await expect(api.teamActivity.set(mockCfg)).rejects.toThrow('write fail');
  });

  it('setupTable: forwards camelCase params and returns created column names', async () => {
    vi.mocked(invoke).mockResolvedValue(['workspace_id', 'ansambel_status']);
    const out = await api.teamActivity.setupTable('appA', 'tblA');
    expect(invoke).toHaveBeenCalledWith('setup_team_activity_table', {
      appToken: 'appA',
      tableId: 'tblA',
    });
    expect(out).toEqual(['workspace_id', 'ansambel_status']);
  });

  it('setupTable: returns empty list on idempotent run', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const out = await api.teamActivity.setupTable('appA', 'tblA');
    expect(out).toEqual([]);
  });

  it('setupTable: propagates Lark API error', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Lark API: code 99991663'));
    await expect(api.teamActivity.setupTable('appA', 'tblA')).rejects.toThrow('99991663');
  });

  describe('api.teamActivity.fetchRows', () => {
    it('invokes fetch_team_activity_rows with no arguments', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
      const result = await api.teamActivity.fetchRows();
      expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
      expect(result).toEqual({ kind: 'rows', rows: [] });
    });

    it('passes through the disabled FetchResult variant unchanged', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ kind: 'disabled' });
      const result = await api.teamActivity.fetchRows();
      expect(result).toEqual({ kind: 'disabled' });
    });
  });
});

describe('agentChannel', () => {
  it('returns a Tauri Channel-shaped object with onmessage setter', () => {
    const ch = agentChannel();
    expect(typeof ch).toBe('object');
    ch.onmessage = (_ev) => undefined;
    expect(typeof ch.onmessage).toBe('function');
  });

  it('two channels are independent instances', () => {
    const a = agentChannel();
    const b = agentChannel();
    expect(a).not.toBe(b);
  });

  it('Channel id is a number (Tauri internal)', () => {
    const ch = agentChannel() as unknown as { id: number };
    expect(typeof ch.id).toBe('number');
  });
});
