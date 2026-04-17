// src/lib/components/kanban/KanbanBoard.test.ts
import { describe, it, expect, vi } from 'vitest';
import { SHADOW_ITEM_MARKER_PROPERTY_NAME } from 'svelte-dnd-action';
import { render, screen } from '@testing-library/svelte';
import KanbanBoard from './KanbanBoard.svelte';
import type { Task } from '$lib/types';

const COLUMNS = ['Todo', 'In Progress', 'Review', 'Done'];

const makeTask = (overrides: Partial<Task> = {}): Task => ({
  id: 'tk_abc123',
  repo_id: 'repo_abc123',
  workspace_id: null,
  title: 'Fix login bug',
  description: '',
  column: 'todo',
  order: 0,
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

describe('KanbanBoard', () => {
  it('renders all 4 column headers', () => {
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [],
        onMove: vi.fn(),
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    for (const col of COLUMNS) {
      expect(screen.getByText(col)).toBeTruthy();
    }
  });

  it('renders task cards in their respective columns', () => {
    const todo = makeTask({
      id: 'tk_todo',
      title: 'Todo task',
      column: 'todo',
    });
    const inProg = makeTask({
      id: 'tk_prog',
      title: 'In Progress task',
      column: 'in_progress',
    });
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [todo, inProg],
        onMove: vi.fn(),
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    expect(screen.getByText('Todo task')).toBeTruthy();
    expect(screen.getByText('In Progress task')).toBeTruthy();
  });

  it('shows Add task button in Todo column', () => {
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [],
        onMove: vi.fn(),
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    expect(screen.getByRole('button', { name: /add task/i })).toBeTruthy();
  });

  it('calls onAddTask when Add task button is clicked', async () => {
    const onAddTask = vi.fn();
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [],
        onMove: vi.fn(),
        onAddTask,
        onRemoveTask: vi.fn(),
      },
    });
    const btn = screen.getByRole('button', { name: /add task/i });
    await btn.click();
    expect(onAddTask).toHaveBeenCalled();
  });

  it('shows empty column message when no tasks in a column', () => {
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [],
        onMove: vi.fn(),
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    const emptyMessages = screen.getAllByText(/no tasks/i);
    expect(emptyMessages.length).toBeGreaterThan(0);
  });
});

describe('KanbanBoard drag behavior', () => {
  it('dnd zones are rendered for each column', () => {
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [],
        onMove: vi.fn(),
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    const zones = document.querySelectorAll('[data-column]');
    expect(zones.length).toBe(4);
  });

  it('calls onMove when a finalize event fires with new column', async () => {
    const onMove = vi.fn();
    const task = makeTask({ id: 'tk_abc123', column: 'todo' });
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [task],
        onMove,
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    const inProgressZone = document.querySelector('[data-column="in_progress"]') as HTMLElement;
    const movedTask = { ...task, column: 'in_progress' as const };
    const event = new CustomEvent('finalize', {
      detail: { items: [movedTask], info: { id: 'tk_abc123' } },
    });
    inProgressZone.dispatchEvent(event);
    expect(onMove).toHaveBeenCalledWith('tk_abc123', 'in_progress', 0);
  });

  it('does not call onMove for consider events (intermediate hover)', async () => {
    const onMove = vi.fn();
    const task = makeTask();
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [task],
        onMove,
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    const todoZone = document.querySelector('[data-column="todo"]') as HTMLElement;
    const event = new CustomEvent('consider', {
      detail: { items: [task], info: { id: 'tk_abc123' } },
    });
    todoZone.dispatchEvent(event);
    expect(onMove).not.toHaveBeenCalled();
  });

  it('shadow item (in-flight drag placeholder) is filtered from final list', async () => {
    const onMove = vi.fn();
    const task = makeTask({ id: 'tk_abc123', column: 'todo' });
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [task],
        onMove,
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    const doneZone = document.querySelector('[data-column="done"]') as HTMLElement;
    const shadowTask = {
      ...task,
      column: 'done' as const,
      [SHADOW_ITEM_MARKER_PROPERTY_NAME]: true,
    };
    const event = new CustomEvent('finalize', {
      detail: { items: [shadowTask], info: { id: 'tk_abc123' } },
    });
    doneZone.dispatchEvent(event);
    // shadow item has marker — onMove should be called with the real id
    expect(onMove).toHaveBeenCalledWith('tk_abc123', 'done', 0);
  });

  it('does not fire onMove when finalize fires on a source zone (item absent from items)', async () => {
    // svelte-dnd-action fires finalize on BOTH the source and destination zones
    // when an item is dragged between zones. Only the destination should trigger
    // onMove — the source zone finalize (where the dropped id is not in items)
    // must be a no-op to avoid double-calling the backend.
    const onMove = vi.fn();
    const task = makeTask({ id: 'tk_abc123', column: 'todo' });
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [task],
        onMove,
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    const todoZone = document.querySelector('[data-column="todo"]') as HTMLElement;
    // Source-zone finalize: task has moved away, items[] no longer contains it.
    const event = new CustomEvent('finalize', {
      detail: { items: [], info: { id: 'tk_abc123' } },
    });
    todoZone.dispatchEvent(event);
    expect(onMove).not.toHaveBeenCalled();
  });

  it('does not show Add task button in non-todo columns', () => {
    render(KanbanBoard, {
      props: {
        repoId: 'repo_abc123',
        tasks: [],
        onMove: vi.fn(),
        onAddTask: vi.fn(),
        onRemoveTask: vi.fn(),
      },
    });
    // Only 1 Add task button should exist (Todo column only)
    const addBtns = screen.getAllByRole('button', { name: /add task/i });
    expect(addBtns.length).toBe(1);
  });
});
