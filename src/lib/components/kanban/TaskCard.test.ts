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

  it('does not truncate description when it is ≤80 chars', () => {
    const short = makeTask({ description: 'Short description.' });
    render(TaskCard, { props: { task: short, onRemove: vi.fn() } });
    const descEl = screen.getByTestId('task-description');
    expect(descEl.textContent).toBe('Short description.');
    expect(descEl.textContent).not.toMatch(/\.\.\.$/);
  });

  it('does not render description element when description is empty', () => {
    const noDesc = makeTask({ description: '' });
    render(TaskCard, { props: { task: noDesc, onRemove: vi.fn() } });
    expect(screen.queryByTestId('task-description')).toBeNull();
  });

  it('shows em-dash placeholder when task has no PIC names', () => {
    render(TaskCard, { props: { task: makeTask(), onRemove: vi.fn() } });
    const pic = screen.getByTestId('task-pic');
    expect(pic.textContent?.trim()).toBe('—');
    expect(pic.getAttribute('title')).toBe('');
  });

  it('shows the single name when task has one PIC', () => {
    const task = makeTask({ pic_names: ['Alice'] });
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    const pic = screen.getByTestId('task-pic');
    expect(pic.textContent?.trim()).toBe('Alice');
    expect(pic.getAttribute('title')).toBe('Alice');
  });

  it('shows first name + "+N" when task has multiple PICs', () => {
    const task = makeTask({ pic_names: ['Alice', 'Bob', 'Carol'] });
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    const pic = screen.getByTestId('task-pic');
    expect(pic.textContent?.trim()).toBe('Alice +2');
    expect(pic.getAttribute('title')).toBe('Alice, Bob, Carol');
  });

  it('treats undefined pic_names (legacy persisted task) as empty', () => {
    const task = makeTask();
    delete (task as { pic_names?: string[] }).pic_names;
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    expect(screen.getByTestId('task-pic').textContent?.trim()).toBe('—');
  });
});
