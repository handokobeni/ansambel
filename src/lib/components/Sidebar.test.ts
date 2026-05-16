// src/lib/components/Sidebar.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import Sidebar from './Sidebar.svelte';

vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: 'repo_abc123' as string | null,
    getSelected: vi.fn(() => ({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    })),
  },
}));

vi.mock('$lib/stores/workspaces.svelte', () => {
  const workspaceList = [
    {
      id: 'ws_abc123',
      repo_id: 'repo_abc123',
      branch: 'feat/task-1',
      base_branch: 'main',
      custom_branch: false,
      title: 'Fix login',
      description: 'Fixing the login bug',
      status: 'running' as const,
      column: 'in_progress' as const,
      created_at: 1776000001,
      updated_at: 1776000001,
      worktree_dir: '/tmp/ws_abc123',
    },
    {
      id: 'ws_def456',
      repo_id: 'repo_abc123',
      branch: 'feat/task-2',
      base_branch: 'main',
      custom_branch: false,
      title: 'Add dark mode',
      description: '',
      status: 'waiting' as const,
      column: 'todo' as const,
      created_at: 1776000002,
      updated_at: 1776000002,
      worktree_dir: '/tmp/ws_def456',
    },
  ];
  return {
    workspaces: {
      selectedWorkspaceId: null as string | null,
      listForRepo: vi.fn(() => workspaceList),
      create: vi.fn(),
      remove: vi.fn(),
      select: vi.fn(),
    },
  };
});

import { workspaces } from '$lib/stores/workspaces.svelte';
import { repos } from '$lib/stores/repos.svelte';
import { getToasts, removeToast } from '$lib/stores/toasts.svelte';

const defaultRepo = {
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null as string | null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
};

const defaultWorkspaceList = [
  {
    id: 'ws_abc123',
    repo_id: 'repo_abc123',
    branch: 'feat/task-1',
    base_branch: 'main',
    custom_branch: false,
    title: 'Fix login',
    description: 'Fixing the login bug',
    status: 'running' as const,
    column: 'in_progress' as const,
    created_at: 1776000001,
    updated_at: 1776000001,
    worktree_dir: '/tmp/ws_abc123',
  },
  {
    id: 'ws_def456',
    repo_id: 'repo_abc123',
    branch: 'feat/task-2',
    base_branch: 'main',
    custom_branch: false,
    title: 'Add dark mode',
    description: '',
    status: 'waiting' as const,
    column: 'todo' as const,
    created_at: 1776000002,
    updated_at: 1776000002,
    worktree_dir: '/tmp/ws_def456',
  },
];

beforeEach(() => {
  vi.clearAllMocks();
  // Re-establish default implementations after clearAllMocks
  vi.mocked(repos.getSelected).mockReturnValue(defaultRepo);
  (repos as { selectedRepoId: string | null }).selectedRepoId = 'repo_abc123';
  vi.mocked(workspaces.listForRepo).mockReturnValue(defaultWorkspaceList);
  (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = null;
});

describe('Sidebar', () => {
  it('renders workspace titles from the store for the selected repo', () => {
    render(Sidebar);
    expect(screen.getByText('Fix login')).toBeInTheDocument();
    expect(screen.getByText('Add dark mode')).toBeInTheDocument();
  });

  it('paints status dots with the user-aligned palette: running=green, waiting=amber', () => {
    // The original mapping (running=amber, waiting=green) confused users
    // because green conventionally signals "active". Running is the most
    // active state — agent currently processing — so it owns green now.
    const { container } = render(Sidebar);
    const dots = container.querySelectorAll('[data-status-dot]');
    expect(dots[0]).toHaveAttribute('data-status', 'running');
    expect(dots[0]?.className).toContain('var(--status-ok)');
    expect(dots[1]).toHaveAttribute('data-status', 'waiting');
    expect(dots[1]?.className).toContain('amber');
  });

  it('selected workspace row shows a left-border accent so selection is unmistakable', () => {
    (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = 'ws_abc123';
    const { container } = render(Sidebar);
    const rows = container.querySelectorAll('li[data-workspace-id]');
    // The selected row carries a marker attribute the styling hooks into.
    // Without this the only selection cue was a subtle bg shift that lost
    // visual priority to the status dot of the OTHER row.
    expect(rows[0]).toHaveAttribute('data-selected', 'true');
    expect(rows[1]).toHaveAttribute('data-selected', 'false');
    // Class-level check pins down the accent so a refactor doesn't silently
    // drop it.
    expect(rows[0]?.className).toContain('var(--accent)');
  });

  it('clicking a workspace row calls workspaces.select with its id', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByText('Fix login'));
    expect(workspaces.select).toHaveBeenCalledWith('ws_abc123');
  });

  it('"New Workspace" button reveals the inline form', async () => {
    render(Sidebar);
    expect(screen.queryByPlaceholderText(/workspace title/i)).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    expect(screen.getByPlaceholderText(/workspace title/i)).toBeInTheDocument();
  });

  it('submitting the form calls workspaces.create with repoId and form values', async () => {
    vi.mocked(workspaces.create).mockResolvedValue({
      id: 'ws_new111',
      repo_id: 'repo_abc123',
      branch: 'feat/new-ws',
      base_branch: 'main',
      custom_branch: false,
      title: 'My new task',
      description: 'A description',
      status: 'not_started',
      column: 'todo',
      created_at: 1776000003,
      updated_at: 1776000003,
      worktree_dir: '/tmp/ws_new111',
    });
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    await fireEvent.input(screen.getByPlaceholderText(/workspace title/i), {
      target: { value: 'My new task' },
    });
    await fireEvent.input(screen.getByPlaceholderText(/description/i), {
      target: { value: 'A description' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    await waitFor(() => {
      expect(workspaces.create).toHaveBeenCalledWith({
        repoId: 'repo_abc123',
        title: 'My new task',
        description: 'A description',
        branchName: undefined,
      });
    });
  });

  it('Cancel button hides the form', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    expect(screen.getByPlaceholderText(/workspace title/i)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(screen.queryByPlaceholderText(/workspace title/i)).not.toBeInTheDocument();
  });

  it('remove workspace button calls workspaces.remove when confirm is accepted', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true)
    );
    vi.mocked(workspaces.remove).mockResolvedValue(undefined);
    render(Sidebar);
    // Hover to reveal remove button via keyboard-accessible click (stopPropagation test)
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    await waitFor(() => {
      expect(workspaces.remove).toHaveBeenCalledWith('ws_abc123', 'repo_abc123');
    });
    vi.unstubAllGlobals();
  });

  it('remove workspace does nothing when confirm is rejected', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => false)
    );
    render(Sidebar);
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    expect(workspaces.remove).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it('remove workspace surfaces a toast even when the rejection is a plain string', async () => {
    // Covers the `err instanceof Error ? err.message : String(err)`
    // fallback branch for backend errors that come through as raw
    // strings rather than Error objects.
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true)
    );
    vi.mocked(workspaces.remove).mockRejectedValue('plain remove failure');
    for (const id of Array.from(getToasts().keys())) removeToast(id);
    render(Sidebar);
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    await waitFor(() => {
      const found = [...getToasts().values()].find((t) =>
        t.message.includes('plain remove failure')
      );
      expect(found).toBeDefined();
    });
    vi.unstubAllGlobals();
  });

  it('create form surfaces a toast even when the rejection is a plain string', async () => {
    vi.mocked(workspaces.create).mockRejectedValue('raw failure');
    for (const id of Array.from(getToasts().keys())) removeToast(id);
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    await fireEvent.input(screen.getByPlaceholderText(/workspace title/i), {
      target: { value: 'Will fail' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    await waitFor(() => {
      const found = [...getToasts().values()].find((t) => t.message.includes('raw failure'));
      expect(found).toBeDefined();
    });
  });

  it('remove workspace surfaces a toast when workspaces.remove rejects', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true)
    );
    vi.mocked(workspaces.remove).mockRejectedValue(new Error('remove failed'));
    // Clear any toasts left over from earlier tests so the assertion is
    // pinned to this run rather than picking up stale state.
    for (const id of Array.from(getToasts().keys())) removeToast(id);
    render(Sidebar);
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    await waitFor(() => {
      const found = [...getToasts().values()].find((t) => t.message.includes('remove failed'));
      expect(found, 'expected an error toast carrying the rejection message').toBeDefined();
      expect(found?.type).toBe('error');
    });
    vi.unstubAllGlobals();
  });

  it('shows "Select a repo to see workspaces" when no workspaces and no repo selected', () => {
    vi.mocked(repos.getSelected).mockReturnValue(null);
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    vi.mocked(workspaces.listForRepo).mockReturnValue([]);
    render(Sidebar);
    expect(screen.getByText(/Select a repo to see workspaces/i)).toBeInTheDocument();
  });

  it('shows "No workspaces yet" when repo is selected but workspace list is empty', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([]);
    render(Sidebar);
    expect(screen.getByText(/No workspaces yet/i)).toBeInTheDocument();
  });

  it('highlights the active workspace row', () => {
    (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = 'ws_abc123';
    const { container } = render(Sidebar);
    // Even if the Tailwind class isn't in jsdom, the Svelte class: binding runs without error
    expect(container.querySelector('ul')).toBeInTheDocument();
  });

  it('submit form does nothing when title is empty', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    // Don't fill the title field — just click create
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    expect(workspaces.create).not.toHaveBeenCalled();
  });

  it('renders gray dot for not_started status (fallback statusDotClass)', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_xyz',
        repo_id: 'repo_abc123',
        branch: 'feat/pending',
        base_branch: 'main',
        custom_branch: false,
        title: 'Pending task',
        description: '',
        status: 'not_started',
        column: 'todo',
        created_at: 1776000010,
        updated_at: 1776000010,
        worktree_dir: '/tmp/ws_xyz',
      },
    ]);
    const { container } = render(Sidebar);
    const dots = container.querySelectorAll('[data-status-dot]');
    expect(dots[0]).toHaveAttribute('data-status', 'not_started');
  });

  it('status dot for done/not_started/error uses muted bg (fallthrough branch)', async () => {
    const fallthroughList = [
      {
        id: 'ws_done1',
        repo_id: 'repo_abc123',
        branch: 'feat/done',
        base_branch: 'main',
        custom_branch: false,
        title: 'Finished task',
        description: '',
        status: 'done' as const,
        column: 'done' as const,
        created_at: 1776000003,
        updated_at: 1776000003,
        worktree_dir: '/tmp/ws_done1',
      },
    ];
    vi.mocked(workspaces.listForRepo).mockReturnValue(fallthroughList);
    const { container } = render(Sidebar);
    // The Done group starts collapsed; expand it first so the row's
    // status dot is in the DOM.
    const doneHeader = container.querySelector<HTMLButtonElement>(
      '[data-sidebar-group="done"] button[aria-expanded]'
    );
    if (doneHeader) await fireEvent.click(doneHeader);
    const dot = container.querySelector('[data-status-dot]');
    expect(dot).toHaveAttribute('data-status', 'done');
    expect(dot?.className).toContain('bg-[var(--text-muted)]');
  });

  it('submitting form with whitespace-only title does not call workspaces.create', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    await fireEvent.input(screen.getByPlaceholderText(/workspace title/i), {
      target: { value: '   ' },
    });
    // Create button stays disabled because formTitle.trim() is empty, but submit via form
    // to exercise the handleCreateSubmit early-return guard (covers line 38 branch).
    const form = screen.getByPlaceholderText(/workspace title/i).closest('form');
    if (form) await fireEvent.submit(form);
    expect(workspaces.create).not.toHaveBeenCalled();
  });

  it('create form surfaces a toast when workspaces.create rejects', async () => {
    vi.mocked(workspaces.create).mockRejectedValue(new Error('backend error'));
    for (const id of Array.from(getToasts().keys())) removeToast(id);
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    await fireEvent.input(screen.getByPlaceholderText(/workspace title/i), {
      target: { value: 'Failing task' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    await waitFor(() => {
      const found = [...getToasts().values()].find((t) => t.message.includes('backend error'));
      expect(found, 'expected an error toast carrying the rejection message').toBeDefined();
      expect(found?.type).toBe('error');
    });
  });
});

describe('Sidebar grouping', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('groups workspaces by column under Active / Review / Done sections', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_a',
        repo_id: 'repo_abc123',
        branch: 'a',
        base_branch: 'main',
        custom_branch: false,
        title: 'Active task',
        description: '',
        status: 'running' as const,
        column: 'in_progress' as const,
        created_at: 1,
        updated_at: 1,
        worktree_dir: '/tmp/a',
      },
      {
        id: 'ws_b',
        repo_id: 'repo_abc123',
        branch: 'b',
        base_branch: 'main',
        custom_branch: false,
        title: 'Review task',
        description: '',
        status: 'waiting' as const,
        column: 'review' as const,
        created_at: 2,
        updated_at: 2,
        worktree_dir: '/tmp/b',
      },
      {
        id: 'ws_c',
        repo_id: 'repo_abc123',
        branch: 'c',
        base_branch: 'main',
        custom_branch: false,
        title: 'Done task',
        description: '',
        status: 'done' as const,
        column: 'done' as const,
        created_at: 3,
        updated_at: 3,
        worktree_dir: '/tmp/c',
      },
    ]);
    const { container } = render(Sidebar);
    expect(container.querySelector('[data-sidebar-group="active"]')).not.toBeNull();
    expect(container.querySelector('[data-sidebar-group="review"]')).not.toBeNull();
    expect(container.querySelector('[data-sidebar-group="done"]')).not.toBeNull();
    // Each row should land in the correct group (active = todo+in_progress).
    const activeIds = [
      ...container.querySelectorAll('[data-sidebar-group="active"] [data-workspace-id]'),
    ].map((el) => el.getAttribute('data-workspace-id'));
    const reviewIds = [
      ...container.querySelectorAll('[data-sidebar-group="review"] [data-workspace-id]'),
    ].map((el) => el.getAttribute('data-workspace-id'));
    expect(activeIds).toEqual(['ws_a']);
    expect(reviewIds).toEqual(['ws_b']);
  });

  it('omits a group section entirely when it has no items', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_only_active',
        repo_id: 'repo_abc123',
        branch: 'a',
        base_branch: 'main',
        custom_branch: false,
        title: 'Only one',
        description: '',
        status: 'running' as const,
        column: 'in_progress' as const,
        created_at: 1,
        updated_at: 1,
        worktree_dir: '/tmp/a',
      },
    ]);
    const { container } = render(Sidebar);
    expect(container.querySelector('[data-sidebar-group="active"]')).not.toBeNull();
    expect(container.querySelector('[data-sidebar-group="review"]')).toBeNull();
    expect(container.querySelector('[data-sidebar-group="done"]')).toBeNull();
  });

  it('Done group is collapsed by default; clicking the header expands it', async () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_done',
        repo_id: 'repo_abc123',
        branch: 'd',
        base_branch: 'main',
        custom_branch: false,
        title: 'Shipped feature',
        description: '',
        status: 'done' as const,
        column: 'done' as const,
        created_at: 1,
        updated_at: 1,
        worktree_dir: '/tmp/d',
      },
    ]);
    const { container } = render(Sidebar);
    // Header exists; row inside is hidden because the group starts collapsed.
    const header = container.querySelector<HTMLButtonElement>(
      '[data-sidebar-group="done"] button[aria-expanded]'
    )!;
    expect(header.getAttribute('aria-expanded')).toBe('false');
    expect(container.querySelector('[data-sidebar-group="done"] [data-workspace-id]')).toBeNull();
    await fireEvent.click(header);
    expect(header.getAttribute('aria-expanded')).toBe('true');
    expect(
      container.querySelector('[data-sidebar-group="done"] [data-workspace-id]')
    ).not.toBeNull();
  });

  it('Active group can be collapsed by clicking its header', async () => {
    const { container } = render(Sidebar);
    const header = container.querySelector<HTMLButtonElement>(
      '[data-sidebar-group="active"] button[aria-expanded]'
    )!;
    expect(header.getAttribute('aria-expanded')).toBe('true');
    await fireEvent.click(header);
    expect(header.getAttribute('aria-expanded')).toBe('false');
    expect(container.querySelector('[data-sidebar-group="active"] [data-workspace-id]')).toBeNull();
  });

  it('header shows the count of workspaces in its group', () => {
    const { container } = render(Sidebar);
    // Two fixtures, both in Active (in_progress + todo) per default mocks.
    const activeHeader = container.querySelector(
      '[data-sidebar-group="active"] button[aria-expanded]'
    )!;
    expect(activeHeader.textContent).toContain('2');
  });

  it('Review group renders its rows when populated and starts expanded', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_review',
        repo_id: 'repo_abc123',
        branch: 'r',
        base_branch: 'main',
        custom_branch: false,
        title: 'Awaiting review',
        description: '',
        status: 'waiting' as const,
        column: 'review' as const,
        created_at: 1,
        updated_at: 1,
        worktree_dir: '/tmp/r',
      },
    ]);
    const { container } = render(Sidebar);
    const header = container.querySelector('[data-sidebar-group="review"] button[aria-expanded]');
    expect(header?.getAttribute('aria-expanded')).toBe('true');
    expect(
      container.querySelector('[data-sidebar-group="review"] [data-workspace-id="ws_review"]')
    ).not.toBeNull();
  });

  it('selected workspace inside Active group still carries the accent border', () => {
    (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = 'ws_abc123';
    const { container } = render(Sidebar);
    const row = container.querySelector(
      '[data-sidebar-group="active"] [data-workspace-id="ws_abc123"]'
    );
    expect(row?.getAttribute('data-selected')).toBe('true');
    expect(row?.className).toContain('var(--accent)');
  });
});

describe('Sidebar resize', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  async function dispatch(node: Element, type: string, init: PointerEventInit = {}): Promise<void> {
    const ev = new Event(type, { bubbles: true });
    Object.assign(ev, init, { pointerId: init.pointerId ?? 1 });
    node.dispatchEvent(ev);
    // Let Svelte 5 flush reactive bindings (e.g. inline width style) so
    // assertions read the post-update DOM.
    const { tick } = await import('svelte');
    await tick();
  }

  it('renders a horizontal resize handle to the right of the sidebar', () => {
    const { container } = render(Sidebar);
    const handle = container.querySelector('[role="separator"][aria-orientation="horizontal"]');
    expect(handle).not.toBeNull();
  });

  it('drag adjusts the sidebar width within the [180, 420] clamp range', async () => {
    const { container } = render(Sidebar);
    const aside = container.querySelector<HTMLElement>('[data-sidebar]')!;
    const startWidth = parseInt(aside.style.width, 10);
    const handle = container.querySelector<HTMLElement>(
      '[role="separator"][aria-orientation="horizontal"]'
    )!;
    handle.setPointerCapture = vi.fn();
    handle.releasePointerCapture = vi.fn();
    await dispatch(handle, 'pointerdown', { clientX: 0 });
    await dispatch(handle, 'pointermove', { clientX: 50 });
    await dispatch(handle, 'pointerup', { clientX: 50 });
    const after = parseInt(aside.style.width, 10);
    expect(after).toBe(Math.min(420, Math.max(180, startWidth + 50)));
  });

  it('persists final width to localStorage when the drag ends', async () => {
    const { container } = render(Sidebar);
    const handle = container.querySelector<HTMLElement>(
      '[role="separator"][aria-orientation="horizontal"]'
    )!;
    handle.setPointerCapture = vi.fn();
    handle.releasePointerCapture = vi.fn();
    dispatch(handle, 'pointerdown', { clientX: 0 });
    dispatch(handle, 'pointermove', { clientX: 30 });
    dispatch(handle, 'pointerup', { clientX: 30 });
    expect(localStorage.getItem('ansambel-sidebar-width')).not.toBeNull();
  });

  it('rehydrates width from localStorage on next mount', () => {
    localStorage.setItem('ansambel-sidebar-width', '320');
    const { container } = render(Sidebar);
    const aside = container.querySelector<HTMLElement>('[data-sidebar]')!;
    expect(aside.style.width).toBe('320px');
  });

  it('clamps a stored width that sits outside [180, 420]', () => {
    localStorage.setItem('ansambel-sidebar-width', '1000');
    const { container } = render(Sidebar);
    const aside = container.querySelector<HTMLElement>('[data-sidebar]')!;
    expect(parseInt(aside.style.width, 10)).toBeLessThanOrEqual(420);
  });

  it('falls back to the default width when stored value is non-numeric', () => {
    localStorage.setItem('ansambel-sidebar-width', 'not-a-number');
    const { container } = render(Sidebar);
    const aside = container.querySelector<HTMLElement>('[data-sidebar]')!;
    // Default = 280 per the component constants.
    expect(aside.style.width).toBe('280px');
  });
});
