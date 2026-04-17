// src/lib/stores/tasks.svelte.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    task: {
      add: vi.fn(),
      list: vi.fn(),
      update: vi.fn(),
      move: vi.fn(),
      remove: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { TasksStore } from './tasks.svelte';
import type { Task } from '$lib/types';

const makeTask = (overrides: Partial<Task> = {}): Task => ({
  id: 'tk_abc123',
  repo_id: 'repo_abc123',
  workspace_id: null,
  title: 'Fix login bug',
  description: 'Users cannot log in.',
  column: 'todo',
  order: 0,
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('TasksStore', () => {
  it('loadForRepo: populates nested map from api.task.list', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    expect(store.tasks.get('repo_abc123')?.get('tk_abc123')).toEqual(task);
  });

  it('loadForRepo: inner map is empty when backend returns []', async () => {
    vi.mocked(api.task.list).mockResolvedValue([]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    expect(store.tasks.get('repo_abc123')?.size).toBe(0);
  });

  it('add: calls api.task.add and inserts returned Task into nested map', async () => {
    const task = makeTask();
    vi.mocked(api.task.add).mockResolvedValue(task);
    const store = new TasksStore();
    const args = {
      repoId: 'repo_abc123',
      title: 'Fix login bug',
      description: '',
    };
    const result = await store.add(args);
    expect(api.task.add).toHaveBeenCalledWith(args);
    expect(result).toEqual(task);
    expect(store.tasks.get('repo_abc123')?.get('tk_abc123')).toEqual(task);
  });

  it('update: calls api.task.update and mutates existing entry in map', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.update).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.update('tk_abc123', { title: 'Updated title' });
    expect(api.task.update).toHaveBeenCalledWith('tk_abc123', {
      title: 'Updated title',
    });
    expect(store.tasks.get('repo_abc123')?.get('tk_abc123')?.title).toBe('Updated title');
  });

  it('move: calls api.task.move and updates column + order in map', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.move).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.move('tk_abc123', 'in_progress', 2);
    expect(api.task.move).toHaveBeenCalledWith('tk_abc123', 'in_progress', 2);
    const updated = store.tasks.get('repo_abc123')?.get('tk_abc123');
    expect(updated?.column).toBe('in_progress');
    expect(updated?.order).toBe(2);
  });

  it('remove: calls api.task.remove and deletes from nested map', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.remove).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.remove('tk_abc123');
    expect(api.task.remove).toHaveBeenCalledWith('tk_abc123', undefined);
    expect(store.tasks.get('repo_abc123')?.has('tk_abc123')).toBe(false);
  });

  it('remove: forwards force=true to api', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.remove).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.remove('tk_abc123', true);
    expect(api.task.remove).toHaveBeenCalledWith('tk_abc123', true);
  });

  it('listForRepo: returns all tasks for a repo as array', async () => {
    const t1 = makeTask({ id: 'tk_aaa111', order: 0 });
    const t2 = makeTask({ id: 'tk_bbb222', order: 1 });
    vi.mocked(api.task.list).mockResolvedValue([t1, t2]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    const list = store.listForRepo('repo_abc123');
    expect(list).toHaveLength(2);
  });

  it('listForColumn: returns only tasks matching the given column, sorted by order', async () => {
    const todo1 = makeTask({ id: 'tk_aaa111', column: 'todo', order: 1 });
    const todo0 = makeTask({ id: 'tk_bbb222', column: 'todo', order: 0 });
    const inProg = makeTask({
      id: 'tk_ccc333',
      column: 'in_progress',
      order: 0,
    });
    vi.mocked(api.task.list).mockResolvedValue([todo1, todo0, inProg]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    const todos = store.listForColumn('repo_abc123', 'todo');
    expect(todos).toHaveLength(2);
    expect(todos[0].id).toBe('tk_bbb222'); // order 0 first
    expect(todos[1].id).toBe('tk_aaa111'); // order 1 second
  });

  it('selectedTaskId: starts null, can be set', () => {
    const store = new TasksStore();
    expect(store.selectedTaskId).toBeNull();
    store.selectTask('tk_abc123');
    expect(store.selectedTaskId).toBe('tk_abc123');
  });

  it('selectTask: accepts null to deselect', () => {
    const store = new TasksStore();
    store.selectTask('tk_abc123');
    store.selectTask(null);
    expect(store.selectedTaskId).toBeNull();
  });
});
