// src/lib/components/kanban/LinkWorkspacePicker.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import LinkWorkspacePicker from './LinkWorkspacePicker.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { tasks } from '$lib/stores/tasks.svelte';
import type { WorkspaceInfo } from '$lib/types';

vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    listForRepo: vi.fn(() => [] as WorkspaceInfo[]),
  },
}));

vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: {
    link: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue({ kind: 'unlinked' }),
  },
}));

const makeWorkspace = (overrides: Partial<WorkspaceInfo> = {}): WorkspaceInfo =>
  ({
    id: 'ws_default',
    repo_id: 'repo_a',
    branch: 'feat/default',
    base_branch: 'main',
    custom_branch: false,
    title: 'Default',
    description: '',
    status: 'Waiting',
    column: 'in_progress',
    created_at: 0,
    updated_at: 0,
    team_activity_private: false,
    task_ids: [],
    worktree_dir: '/tmp/ws_default',
    ...overrides,
  }) as WorkspaceInfo;

describe('LinkWorkspacePicker', () => {
  beforeEach(() => {
    vi.mocked(workspaces.listForRepo).mockReset();
    vi.mocked(workspaces.listForRepo).mockReturnValue([]);
    vi.mocked(tasks.link).mockReset();
    vi.mocked(tasks.link).mockResolvedValue(undefined);
    vi.mocked(tasks.unlink).mockReset();
    vi.mocked(tasks.unlink).mockResolvedValue({ kind: 'unlinked' });
  });

  it('lists workspaces for the card repo, sorted by updated_at desc', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      makeWorkspace({
        id: 'old',
        title: 'Older',
        branch: 'b1',
        task_ids: ['x'],
        updated_at: 1,
      }),
      makeWorkspace({
        id: 'new',
        title: 'Newer',
        branch: 'b2',
        task_ids: ['y', 'z'],
        updated_at: 5,
      }),
    ]);
    const { getAllByTestId } = render(LinkWorkspacePicker, {
      props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose: vi.fn() },
    });
    const rows = getAllByTestId('link-picker-row');
    expect(rows[0].textContent).toMatch(/Newer/);
    expect(rows[1].textContent).toMatch(/Older/);
  });

  it('renders branch + card count + last-modified for each row', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      makeWorkspace({
        id: 'ws_a',
        title: 'Pay',
        branch: 'feat/pay',
        task_ids: ['x', 'y'],
        updated_at: Math.floor(Date.now() / 1000) - 60 * 30, // 30 min ago
      }),
    ]);
    const { getByTestId } = render(LinkWorkspacePicker, {
      props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose: vi.fn() },
    });
    const row = getByTestId('link-picker-row');
    expect(row.textContent).toMatch(/Pay/);
    expect(row.textContent).toMatch(/feat\/pay/);
    expect(row.textContent).toMatch(/2 cards/);
    // relative time helper output for 30m ago should be "30m ago" — use a
    // loose match because the exact whole-minute value may drift by one.
    expect(row.textContent).toMatch(/\d+m ago/);
  });

  it('selecting a workspace calls tasks.link and closes the picker', async () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      makeWorkspace({
        id: 'ws_a',
        title: 'Pay',
        branch: 'feat/pay',
        task_ids: [],
        updated_at: 1,
      }),
    ]);
    const onClose = vi.fn();
    const { getByTestId } = render(LinkWorkspacePicker, {
      props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose },
    });
    await fireEvent.click(getByTestId('link-picker-row'));
    await waitFor(() => expect(tasks.link).toHaveBeenCalledWith('tk_a', 'ws_a', 'repo_a'));
    expect(onClose).toHaveBeenCalled();
  });

  it('renders empty-state when the repo has zero workspaces', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([]);
    const { getByTestId } = render(LinkWorkspacePicker, {
      props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose: vi.fn() },
    });
    expect(getByTestId('link-picker-empty').textContent).toMatch(/No workspaces/);
  });
});
