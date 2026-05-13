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
      refresh: vi.fn(),
    },
  },
}));

vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
}));

import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
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

  it('move: calls api.task.move and replaces local task with backend response (incl workspace_id)', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.move).mockResolvedValue({
      ...task,
      column: 'in_progress',
      order: 2,
      workspace_id: 'ws_auto_created',
    });
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.move('tk_abc123', 'in_progress', 2);
    expect(api.task.move).toHaveBeenCalledWith('tk_abc123', 'in_progress', 2);
    const updated = store.tasks.get('repo_abc123')?.get('tk_abc123');
    expect(updated?.column).toBe('in_progress');
    expect(updated?.order).toBe(2);
    expect(updated?.workspace_id).toBe('ws_auto_created');
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

  it('update: does nothing when task id is not found in any repo map', async () => {
    vi.mocked(api.task.update).mockResolvedValue(undefined);
    const store = new TasksStore();
    // No tasks loaded — update should not throw
    await store.update('tk_missing', { title: 'Noop' });
    expect(api.task.update).toHaveBeenCalledWith('tk_missing', { title: 'Noop' });
  });

  it('move: hydrates the task into the store from backend response even if absent locally', async () => {
    const moved = makeTask({ id: 'tk_remote', column: 'review', order: 0 });
    vi.mocked(api.task.move).mockResolvedValue(moved);
    const store = new TasksStore();
    await store.move('tk_remote', 'review', 0);
    expect(api.task.move).toHaveBeenCalledWith('tk_remote', 'review', 0);
    expect(store.tasks.get(moved.repo_id)?.get('tk_remote')?.column).toBe('review');
  });

  it('remove: clears selectedTaskId when removed task was selected', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.remove).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    store.selectTask('tk_abc123');
    expect(store.selectedTaskId).toBe('tk_abc123');
    await store.remove('tk_abc123');
    expect(store.selectedTaskId).toBeNull();
  });

  it('remove: does nothing when task id is not found in any repo map', async () => {
    vi.mocked(api.task.remove).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.remove('tk_missing');
    expect(api.task.remove).toHaveBeenCalledWith('tk_missing', undefined);
  });

  it('listForRepo: returns empty array when repo has no tasks loaded', () => {
    const store = new TasksStore();
    const result = store.listForRepo('repo_unknown');
    expect(result).toEqual([]);
  });

  it('tasks_move_optimistic_updates_then_reconciles_with_backend_response', async () => {
    const task = makeTask({ id: 'tk_1', repo_id: 'r', column: 'todo', order: 1024 });
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.move).mockResolvedValue({
      ...task,
      column: 'done',
      order: 2048,
      updated_at: 1,
    });
    const store = new TasksStore();
    await store.loadForRepo('r');

    const promise = store.move('tk_1', 'done', 2048);
    // Before awaiting, optimistic write should already be visible
    expect(store.tasks.get('r')?.get('tk_1')?.column).toBe('done');

    await promise;
    // After resolution, reconcile with backend response
    expect(store.tasks.get('r')?.get('tk_1')?.updated_at).toBe(1);
  });

  it('tasks_move_revert_on_error_restores_previous_state_and_toasts', async () => {
    const task = makeTask({ id: 'tk_1', repo_id: 'r', column: 'todo', order: 1024 });
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.move).mockRejectedValue(new Error('Lark API: 91403 Forbidden'));
    const store = new TasksStore();
    await store.loadForRepo('r');

    await store.move('tk_1', 'done', 2048).catch(() => {});

    // Should be reverted to original state
    expect(store.tasks.get('r')?.get('tk_1')?.column).toBe('todo');
    expect(store.tasks.get('r')?.get('tk_1')?.order).toBe(1024);
    // Should have toasted with error
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('Move failed'), 'error');
  });

  it('tasks_refresh_replaces_map_for_selected_repo', async () => {
    const oldTask = makeTask({ id: 'tk_old', repo_id: 'r', column: 'todo', order: 0 });
    vi.mocked(api.task.list).mockResolvedValue([oldTask]);
    const store = new TasksStore();
    await store.loadForRepo('r');

    vi.mocked(api.task.refresh).mockResolvedValue([
      makeTask({ id: 'tk_a', repo_id: 'r', column: 'todo', order: 0 }),
    ]);

    await store.refresh('r');

    expect(api.task.refresh).toHaveBeenCalledWith('r');
    expect(store.tasks.get('r')?.has('tk_old')).toBe(false);
    expect(store.tasks.get('r')?.has('tk_a')).toBe(true);
  });

  it('move: stringifies non-Error rejections in the toast message', async () => {
    const initial = makeTask({ id: 'tk_1', repo_id: 'r', column: 'todo', order: 1024 });
    vi.mocked(api.task.list).mockResolvedValue([initial]);
    const store = new TasksStore();
    await store.loadForRepo('r');

    vi.mocked(api.task.move).mockRejectedValueOnce('plain-string-error');
    await store.move('tk_1', 'done', 2048).catch(() => {});

    expect(store.tasks.get('r')?.get('tk_1')?.column).toBe('todo');
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('plain-string-error'), 'error');
  });

  it('refresh without repoId clears all nested maps and re-populates by repo_id', async () => {
    vi.mocked(api.task.list).mockResolvedValueOnce([makeTask({ id: 'tk_old_a', repo_id: 'r1' })]);
    const store = new TasksStore();
    await store.loadForRepo('r1');
    vi.mocked(api.task.list).mockResolvedValueOnce([makeTask({ id: 'tk_old_b', repo_id: 'r2' })]);
    await store.loadForRepo('r2');

    vi.mocked(api.task.refresh).mockResolvedValue([
      makeTask({ id: 'tk_new_a', repo_id: 'r1' }),
      makeTask({ id: 'tk_new_b', repo_id: 'r2' }),
    ]);

    await store.refresh();

    expect(api.task.refresh).toHaveBeenCalledWith(undefined);
    expect(store.tasks.get('r1')?.has('tk_old_a')).toBe(false);
    expect(store.tasks.get('r2')?.has('tk_old_b')).toBe(false);
    expect(store.tasks.get('r1')?.has('tk_new_a')).toBe(true);
    expect(store.tasks.get('r2')?.has('tk_new_b')).toBe(true);
  });
});
