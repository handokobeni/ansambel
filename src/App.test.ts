// src/App.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import App from './App.svelte';

// Spy on addToast so handleMove toast assertion tests can assert it was called.
vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
  removeToast: vi.fn(),
  getToasts: vi.fn(() => new Map()),
}));

// Use the manual mock for KanbanBoard so its onMove prop can be triggered
// from tests without needing svelte-dnd-action in jsdom.
vi.mock('$lib/components/kanban/KanbanBoard.svelte');

// Stub WorkspaceView so xterm (which requires canvas) never instantiates in
// jsdom. The manual mock at __mocks__/WorkspaceView.svelte renders a sentinel
// div so mount-persistence tests can locate it.
vi.mock('$lib/components/workspace/WorkspaceView.svelte');

// Mock @tauri-apps/api/event so listen() calls in onMount don't fail without
// a real Tauri runtime.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// Mock lark-bindings store so larkBindings.load() resolves without IPC.
vi.mock('$lib/stores/lark-bindings.svelte', () => ({
  larkBindings: {
    load: vi.fn().mockResolvedValue(undefined),
    has: vi.fn().mockReturnValue(false),
    get: vi.fn().mockReturnValue(undefined),
    bindings: new Map(),
  },
}));

// Mock @tauri-apps/api/core so WorkspaceView (rendered in work mode) does not
// break tests that run without a real Tauri runtime.
vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    id = Math.random();
    onmessage?: (ev: unknown) => void;
  }
  return {
    invoke: vi.fn((cmd: string) => {
      if (cmd === 'fetch_team_activity_rows') {
        return Promise.resolve({ kind: 'disabled' });
      }
      return Promise.resolve(undefined);
    }),
    Channel: MockChannel,
  };
});

vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: null as string | null,
    load: vi.fn().mockResolvedValue(undefined),
    select: vi.fn(),
    getSelected: vi.fn(() => null),
    repos: new Map(),
  },
}));

vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    selectedWorkspaceId: null as string | null,
    // Task 18: WorkspaceView reads the privacy flag via byRepo. Surface
    // a real Map so `.get(...)` works (returns undefined → derived falls
    // back to the prop value, which is what the App-level tests want).
    byRepo: new Map(),
    loadForRepo: vi.fn().mockResolvedValue(undefined),
    listForRepo: vi.fn(() => []),
    select: vi.fn(),
    create: vi.fn(),
    remove: vi.fn(),
    getSelected: vi.fn(() => null),
    setTeamActivityPrivate: vi.fn().mockResolvedValue(true),
  },
}));

vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: {
    selectedTaskId: null as string | null,
    loadForRepo: vi.fn().mockResolvedValue(undefined),
    refresh: vi.fn().mockResolvedValue(undefined),
    listForRepo: vi.fn(() => []),
    listForColumn: vi.fn(() => []),
    isLoading: vi.fn(() => false),
    add: vi.fn(),
    update: vi.fn(),
    move: vi.fn().mockResolvedValue(undefined),
    remove: vi.fn(),
  },
}));

vi.mock('$lib/stores/mode.svelte', () => {
  const state = { mode: 'plan' as 'plan' | 'work' };
  return {
    modeStore: {
      get mode() {
        return state.mode;
      },
      set: vi.fn((next: 'plan' | 'work') => {
        state.mode = next;
      }),
    },
  };
});

vi.mock('$lib/keyboard', () => ({
  ShortcutRegistry: vi.fn().mockImplementation(() => ({
    register: vi.fn(),
    destroy: vi.fn(),
  })),
}));

import { repos } from '$lib/stores/repos.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { tasks } from '$lib/stores/tasks.svelte';
import { modeStore } from '$lib/stores/mode.svelte';
import { addToast } from '$lib/stores/toasts.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  vi.mocked(workspaces.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
  (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = null;
  // Wire the select mock to update selectedRepoId so onMount's auto-select
  // branch (which reads selectedRepoId after calling select) sees the change.
  vi.mocked(repos.select).mockImplementation((id: string | null) => {
    (repos as { selectedRepoId: string | null }).selectedRepoId = id;
  });
  modeStore.set('plan');
});

describe('App', () => {
  it('renders TitleBar and Sidebar shells', () => {
    render(App);
    expect(screen.getByRole('button', { name: /add repo/i })).toBeInTheDocument();
    // Sidebar header label "WORKSPACES" — match by exact uppercase to disambiguate
    expect(screen.getByText('Workspaces')).toBeInTheDocument();
  });

  it('plan mode shows "Add a repo to start" when no repo selected', async () => {
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/add a repo to start managing tasks/i)).toBeInTheDocument();
    });
  });

  it('plan mode renders KanbanBoard columns when a repo is selected', async () => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/in progress/i)).toBeInTheDocument();
      expect(screen.getByText(/review/i)).toBeInTheDocument();
    });
  });

  it('work mode shows "Select or create a workspace" when none selected', async () => {
    modeStore.set('work');
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/select or create a workspace/i)).toBeInTheDocument();
    });
  });

  it('work mode shows workspace name when one is selected', async () => {
    modeStore.set('work');
    vi.mocked(workspaces.getSelected).mockReturnValue({
      id: 'ws_abc',
      repo_id: 'repo_abc',
      branch: 'feat/test',
      base_branch: 'main',
      custom_branch: false,
      title: 'Test workspace',
      description: '',
      status: 'running',
      column: 'in_progress',
      created_at: 1776000001,
      updated_at: 1776000001,
      worktree_dir: '/tmp/ws_abc',
      task_id: null,
    });
    render(App);
    await waitFor(() => {
      expect(screen.getByText('Test workspace')).toBeInTheDocument();
    });
  });

  it('hydrates tasks for the selected repo on mount', async () => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_xyz',
      name: 'xyz',
      path: '/x/y/z',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    (repos as { selectedRepoId: string | null }).selectedRepoId = 'repo_xyz';
    render(App);
    await waitFor(() => {
      expect(tasks.loadForRepo).toHaveBeenCalledWith('repo_xyz');
      expect(workspaces.loadForRepo).toHaveBeenCalledWith('repo_xyz');
    });
  });

  it('auto-selects the first repo on cold start so tasks/workspaces load without re-Add', async () => {
    // Cold start: selectedRepoId is null, but the persisted repos list has
    // entries — load() populates the SvelteMap. App should pick the first
    // and trigger both loaders so the kanban does not appear empty after
    // restart.
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    const repoMap = new Map<string, unknown>();
    repoMap.set('repo_kelola', {
      id: 'repo_kelola',
      name: 'kelola-app',
      path: '/x/kelola',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    (repos as unknown as { repos: Map<string, unknown> }).repos = repoMap;
    // load() is what the app awaits; simulate it setting selectedRepoId by
    // letting our auto-select logic do it. The mock just resolves.
    vi.mocked(repos.load).mockResolvedValue(undefined);
    render(App);
    await waitFor(() => {
      expect(repos.select).toHaveBeenCalledWith('repo_kelola');
      expect(tasks.loadForRepo).toHaveBeenCalledWith('repo_kelola');
      expect(workspaces.loadForRepo).toHaveBeenCalledWith('repo_kelola');
    });
  });

  it('does not auto-select when repos list is empty (first-run experience preserved)', async () => {
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    (repos as unknown as { repos: Map<string, unknown> }).repos = new Map();
    vi.mocked(repos.load).mockResolvedValue(undefined);
    render(App);
    // Wait for onMount to settle; nothing should be selected.
    await new Promise((r) => setTimeout(r, 10));
    expect(repos.select).not.toHaveBeenCalled();
    expect(tasks.loadForRepo).not.toHaveBeenCalled();
    expect(workspaces.loadForRepo).not.toHaveBeenCalled();
  });

  it('does not override an already-set selectedRepoId during auto-select', async () => {
    // If a future change persists selectedRepoId, the auto-select branch
    // must not clobber it — only fall back when nothing is selected.
    (repos as { selectedRepoId: string | null }).selectedRepoId = 'repo_existing';
    const repoMap = new Map<string, unknown>();
    repoMap.set('repo_first', {
      id: 'repo_first',
      name: 'first',
      path: '/x',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    repoMap.set('repo_existing', {
      id: 'repo_existing',
      name: 'existing',
      path: '/y',
      gh_profile: null,
      default_branch: 'main',
      created_at: 2,
      updated_at: 2,
    });
    (repos as unknown as { repos: Map<string, unknown> }).repos = repoMap;
    vi.mocked(repos.load).mockResolvedValue(undefined);
    render(App);
    await waitFor(() => {
      expect(tasks.loadForRepo).toHaveBeenCalledWith('repo_existing');
    });
    expect(repos.select).not.toHaveBeenCalled();
  });

  it('clicking Plan/Work toggle switches mode store', async () => {
    render(App);
    const workBtn = await screen.findByRole('button', { name: /^work$/i });
    await fireEvent.click(workBtn);
    expect(modeStore.set).toHaveBeenCalledWith('work');
  });
});

describe('WorkspaceView mount-persistence across mode toggle', () => {
  // Regression test for: toggling Plan↔Work must NOT unmount WorkspaceView.
  // The fix wraps WorkspaceView in a `class:hidden` div so it stays mounted
  // in the DOM even when plan mode is active.
  const WS_FIXTURE = {
    id: 'ws_persist',
    repo_id: 'repo_persist',
    title: 'Persist WS',
    description: '',
    branch: 'feat/persist',
    base_branch: 'main',
    custom_branch: false,
    status: 'running' as const,
    column: 'in_progress' as const,
    created_at: 0,
    updated_at: 0,
    worktree_dir: '/tmp/ws_persist',
    task_id: null,
  };

  it('WorkspaceView stub stays in DOM (hidden wrapper) when mode switches to plan', async () => {
    // Start in plan mode (beforeEach default) with a workspace already selected.
    // With the old {:else if} structure the stub would NOT be in the DOM at all
    // in plan mode — this test asserts it IS present (just hidden).
    vi.mocked(workspaces.getSelected).mockReturnValue(WS_FIXTURE);
    render(App);

    // The workspace-view-stub must be in the DOM even in plan mode.
    const stub = screen.getByTestId('workspace-view-stub');
    expect(stub).toBeInTheDocument();

    // Its host wrapper must carry the `hidden` class so it is not visible.
    const hostWrapper = stub.parentElement!;
    expect(hostWrapper).toHaveClass('hidden');
  });
});

describe('App work mode', () => {
  it('renders WorkspaceView when work mode + selected workspace', async () => {
    vi.mocked(workspaces.getSelected).mockReturnValue({
      id: 'ws_a',
      repo_id: 'repo_a',
      title: 'Fix login',
      description: '',
      branch: 'feat/x',
      base_branch: 'main',
      custom_branch: false,
      status: 'running',
      column: 'in_progress',
      created_at: 0,
      updated_at: 0,
      worktree_dir: '/tmp/ws_a',
      task_id: null,
    });
    modeStore.set('work');
    const { getByText } = render(App);
    await waitFor(() => expect(getByText('Fix login')).toBeTruthy());
  });

  it('falls back to "Select or create" when work mode but no workspace', async () => {
    vi.mocked(workspaces.getSelected).mockReturnValue(null);
    modeStore.set('work');
    const { getByText } = render(App);
    expect(getByText(/select or create/i)).toBeTruthy();
  });

  it('keeps Plan mode rendering KanbanBoard', async () => {
    modeStore.set('plan');
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_a',
      name: 'Demo',
      path: '/x',
      gh_profile: null,
      default_branch: 'main',
      created_at: 0,
      updated_at: 0,
    });
    const { getByText } = render(App);
    await waitFor(() => expect(getByText(/Todo/)).toBeTruthy());
  });
});

// Helper repo fixture reused across handleMove tests.
const REPO_A = {
  id: 'repo_a',
  name: 'test-repo',
  path: '/x',
  gh_profile: null,
  default_branch: 'main',
  created_at: 0,
  updated_at: 0,
};

// Helper task fixture with a workspace link.
const TASK_WITH_WS = {
  id: 'task_trigger',
  repo_id: 'repo_a',
  workspace_id: 'ws_x',
  title: 'Some task',
  description: '',
  column: 'in_progress' as const,
  order: 0,
  created_at: 0,
  updated_at: 0,
};

describe('App handleMove — empty-workspace removal toast', () => {
  // Shared setup: plan mode with a selected repo so the mock KanbanBoard renders.
  beforeEach(() => {
    modeStore.set('plan');
    vi.mocked(repos.getSelected).mockReturnValue(REPO_A);
    (repos as { selectedRepoId: string | null }).selectedRepoId = REPO_A.id;
  });

  it('fires addToast when task had a workspace, moves to todo, and backend cleared workspace_id', async () => {
    // Seed: the task currently has a workspace link.
    vi.mocked(tasks.listForRepo).mockReturnValue([TASK_WITH_WS]);
    // Backend move returns the task without a workspace link (empty WS removed).
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_WITH_WS,
      workspace_id: null,
      column: 'todo',
    });

    render(App);

    // Wait for the mock board's trigger button to appear then click it.
    const btn = await screen.findByTestId('trigger-move-todo');
    await fireEvent.click(btn);

    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith('Removed empty workspace', 'info');
    });
  });

  it('does NOT fire the toast when moving to in_progress (not todo)', async () => {
    vi.mocked(tasks.listForRepo).mockReturnValue([TASK_WITH_WS]);
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_WITH_WS,
      workspace_id: null,
      column: 'in_progress',
    });

    render(App);

    const btn = await screen.findByTestId('trigger-move-in-progress');
    await fireEvent.click(btn);

    await waitFor(() => {
      expect(tasks.move).toHaveBeenCalled();
    });
    expect(addToast).not.toHaveBeenCalledWith('Removed empty workspace', 'info');
  });

  it('does NOT fire the toast when the task had no workspace to begin with', async () => {
    // Task starts with no workspace.
    vi.mocked(tasks.listForRepo).mockReturnValue([{ ...TASK_WITH_WS, workspace_id: null }]);
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_WITH_WS,
      workspace_id: null,
      column: 'todo',
    });

    render(App);

    const btn = await screen.findByTestId('trigger-move-todo');
    await fireEvent.click(btn);

    await waitFor(() => {
      expect(tasks.move).toHaveBeenCalled();
    });
    expect(addToast).not.toHaveBeenCalledWith('Removed empty workspace', 'info');
  });

  it('does NOT fire the toast when the backend kept workspace_id non-null', async () => {
    vi.mocked(tasks.listForRepo).mockReturnValue([TASK_WITH_WS]);
    // Backend kept the workspace link (e.g. workspace was not empty).
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_WITH_WS,
      workspace_id: 'ws_x',
      column: 'todo',
    });

    render(App);

    const btn = await screen.findByTestId('trigger-move-todo');
    await fireEvent.click(btn);

    await waitFor(() => {
      expect(tasks.move).toHaveBeenCalled();
    });
    expect(addToast).not.toHaveBeenCalledWith('Removed empty workspace', 'info');
  });
});
