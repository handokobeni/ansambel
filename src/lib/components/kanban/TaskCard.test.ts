// src/lib/components/kanban/TaskCard.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import TaskCard from './TaskCard.svelte';
import type { Task } from '$lib/types';

const makeTask = (overrides: Partial<Task> = {}): Task => ({
  id: 'tk_abc123',
  repo_id: 'repo_abc123',
  workspace_id: null,
  title: 'Fix login bug',
  description:
    'Users cannot log in after password reset. This is a longer description that exceeds 80 characters.',
  column: 'todo',
  order: 0,
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

describe('TaskCard', () => {
  it('renders task title', () => {
    render(TaskCard, { props: { task: makeTask(), onRemove: vi.fn() } });
    expect(screen.getByText('Fix login bug')).toBeTruthy();
  });

  it('truncates description to 80 chars with ellipsis', () => {
    render(TaskCard, { props: { task: makeTask(), onRemove: vi.fn() } });
    const descEl = screen.getByTestId('task-description');
    expect(descEl.textContent?.length).toBeLessThanOrEqual(83); // 80 + '...'
    expect(descEl.textContent).toMatch(/\.\.\.$/);
  });

  it('shows branch badge when workspace_id is set', () => {
    const task = makeTask({ workspace_id: 'ws_xyz999' });
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    expect(screen.getByTestId('branch-badge')).toBeTruthy();
  });

  it('omits branch badge when workspace_id is null', () => {
    render(TaskCard, { props: { task: makeTask(), onRemove: vi.fn() } });
    expect(screen.queryByTestId('branch-badge')).toBeNull();
  });

  it('calls onRemove with task id when remove button clicked', async () => {
    const onRemove = vi.fn();
    render(TaskCard, { props: { task: makeTask(), onRemove } });
    await fireEvent.click(screen.getByRole('button', { name: /remove/i }));
    expect(onRemove).toHaveBeenCalledWith('tk_abc123');
  });
});
