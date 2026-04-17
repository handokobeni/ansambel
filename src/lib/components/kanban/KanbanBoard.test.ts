// src/lib/components/kanban/KanbanBoard.test.ts
import { describe, it, expect, vi } from 'vitest';
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
