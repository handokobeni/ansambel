// src/lib/components/kanban/TaskCard.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import TaskCard from './TaskCard.svelte';
import type { Task } from '$lib/types';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { modeStore } from '$lib/stores/mode.svelte';

vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    byId: vi.fn(() => undefined),
    select: vi.fn(),
    getSelected: vi.fn(() => null),
  },
}));

vi.mock('$lib/stores/mode.svelte', () => ({
  modeStore: {
    mode: 'plan' as 'plan' | 'work',
    set: vi.fn(),
  },
}));

vi.mock('$lib/actions', () => ({
  tooltip: () => ({ destroy: () => {} }),
}));

vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: {
    link: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue({ kind: 'unlinked' }),
  },
}));

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
  beforeEach(() => {
    vi.mocked(workspaces.byId).mockReturnValue(undefined);
    vi.mocked(workspaces.select).mockClear();
    vi.mocked(modeStore.set).mockClear();
  });

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

  it('renders a workspace chip when the task is linked', () => {
    vi.mocked(workspaces.byId).mockReturnValue({
      id: 'ws_a',
      repo_id: 'repo_abc123',
      title: 'payment-refactor',
      task_ids: ['tk_abc123'],
      team_activity_private: false,
    } as never);
    const task = makeTask({ workspace_id: 'ws_a' });
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    const chip = screen.getByTestId('task-workspace-chip');
    expect(chip.textContent).toMatch(/payment-refactor/);
  });

  it('does NOT render the chip when task.workspace_id is null', () => {
    render(TaskCard, { props: { task: makeTask(), onRemove: vi.fn() } });
    expect(screen.queryByTestId('task-workspace-chip')).toBeNull();
  });

  it('menu "Link to workspace…" opens the picker', async () => {
    // Provide one workspace in the repo so the picker renders a row.
    const wsModule = await import('$lib/stores/workspaces.svelte');
    const wsMock = wsModule.workspaces as unknown as {
      listForRepo?: ReturnType<typeof vi.fn>;
    };
    wsMock.listForRepo = vi.fn(() => [
      {
        id: 'ws_pick',
        repo_id: 'repo_abc123',
        branch: 'feat/pick',
        title: 'Pick me',
        task_ids: [],
        updated_at: 1,
      },
    ]) as never;

    const task = makeTask({ workspace_id: null });
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    await fireEvent.click(screen.getByTestId('task-menu-trigger'));
    await fireEvent.click(screen.getByTestId('task-menu-link-workspace'));
    expect(screen.getByTestId('link-picker-row')).toBeTruthy();
  });

  it('clicking the chip selects the workspace and switches to work mode', async () => {
    vi.mocked(workspaces.byId).mockReturnValue({
      id: 'ws_a',
      repo_id: 'repo_abc123',
      title: 'payment-refactor',
      task_ids: ['tk_abc123'],
      team_activity_private: false,
    } as never);
    const task = makeTask({ workspace_id: 'ws_a' });
    render(TaskCard, { props: { task, onRemove: vi.fn() } });
    const chip = screen.getByTestId('task-workspace-chip');
    await fireEvent.click(chip);
    expect(vi.mocked(workspaces.select)).toHaveBeenCalledWith('ws_a');
    expect(vi.mocked(modeStore.set)).toHaveBeenCalledWith('work');
  });
});
