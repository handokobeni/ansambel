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

// Module-scope handle for the persisted repo id the mocked `get_selected_repo`
// command should resolve with. Each test sets this in its setup phase; the
// shared beforeEach below resets it back to null so untouched tests preserve
// the pre-existing "no persisted selection" baseline.
let persistedRepoId: string | null = null;

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
      if (cmd === 'get_selected_repo') {
        return Promise.resolve(persistedRepoId);
      }
      if (cmd === 'set_selected_repo') {
        return Promise.resolve(undefined);
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
    byId: vi.fn(() => undefined),
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
    link: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue({ kind: 'unlinked' }),
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
  persistedRepoId = null;
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
      task_ids: [],
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

  it('on cold start restores the persisted selected_repo_id when it is present in the repos list', async () => {
    // Two repos in deterministic insertion order: top first, kelola second.
    // The persisted id is the second one — without restore logic the app
    // would always pick the first.
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    const repoMap = new Map<string, unknown>();
    repoMap.set('repo_top', {
      id: 'repo_top',
      name: 'top-assessment',
      path: '/top',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    repoMap.set('repo_kelola', {
      id: 'repo_kelola',
      name: 'kelola-app',
      path: '/kelola',
      gh_profile: null,
      default_branch: 'main',
      created_at: 2,
      updated_at: 2,
    });
    (repos as unknown as { repos: Map<string, unknown> }).repos = repoMap;
    persistedRepoId = 'repo_kelola';
    vi.mocked(repos.load).mockResolvedValue(undefined);
    render(App);
    await waitFor(() => {
      expect(repos.select).toHaveBeenCalledWith('repo_kelola');
    });
    // And the fallback path was NOT taken (no extra call with repo_top).
    expect(repos.select).not.toHaveBeenCalledWith('repo_top');
  });

  it('on cold start falls back to the first repo when no selection is persisted', async () => {
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    const repoMap = new Map<string, unknown>();
    repoMap.set('repo_top', {
      id: 'repo_top',
      name: 'top-assessment',
      path: '/top',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    repoMap.set('repo_kelola', {
      id: 'repo_kelola',
      name: 'kelola-app',
      path: '/kelola',
      gh_profile: null,
      default_branch: 'main',
      created_at: 2,
      updated_at: 2,
    });
    (repos as unknown as { repos: Map<string, unknown> }).repos = repoMap;
    persistedRepoId = null;
    vi.mocked(repos.load).mockResolvedValue(undefined);
    render(App);
    await waitFor(() => {
      expect(repos.select).toHaveBeenCalledWith('repo_top');
    });
  });

  it('on cold start falls back to the first repo when the persisted id no longer exists', async () => {
    // Stale persisted id: the repo was removed between sessions. The
    // validity check (`repos.repos.has(persisted)`) keeps us from picking
    // a ghost and the fallback fires.
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    const repoMap = new Map<string, unknown>();
    repoMap.set('repo_top', {
      id: 'repo_top',
      name: 'top-assessment',
      path: '/top',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    repoMap.set('repo_kelola', {
      id: 'repo_kelola',
      name: 'kelola-app',
      path: '/kelola',
      gh_profile: null,
      default_branch: 'main',
      created_at: 2,
      updated_at: 2,
    });
    (repos as unknown as { repos: Map<string, unknown> }).repos = repoMap;
    persistedRepoId = 'repo_gone';
    vi.mocked(repos.load).mockResolvedValue(undefined);
    render(App);
    await waitFor(() => {
      expect(repos.select).toHaveBeenCalledWith('repo_top');
    });
    expect(repos.select).not.toHaveBeenCalledWith('repo_gone');
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
    task_ids: [],
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
      task_ids: [],
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

// Task 12: auto-create undo toast on Todo→InProgress that creates a workspace.
describe('App handleMove — auto-create undo toast', () => {
  // Same setup as the other handleMove tests: plan mode + a selected repo so
  // the mock KanbanBoard renders the trigger buttons.
  beforeEach(() => {
    modeStore.set('plan');
    vi.mocked(repos.getSelected).mockReturnValue(REPO_A);
    (repos as { selectedRepoId: string | null }).selectedRepoId = REPO_A.id;
  });

  // A task that starts with NO workspace link — moving it to in_progress should
  // trigger auto-create and the new undo toast.
  const TASK_NO_WS = {
    ...TASK_WITH_WS,
    workspace_id: null,
    column: 'todo' as const,
  };

  it('fires a 10s toast with [Link to existing instead] and [Undo create] when a workspace is auto-created', async () => {
    vi.mocked(tasks.listForRepo).mockReturnValue([TASK_NO_WS]);
    // Backend move returns the task with a fresh workspace_id (auto-created).
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_NO_WS,
      workspace_id: 'ws_new',
      column: 'in_progress',
    });
    vi.mocked(workspaces.byId).mockReturnValue({
      id: 'ws_new',
      repo_id: REPO_A.id,
      title: 'Shiny new ws',
      branch: 'feat/auto',
      base_branch: 'main',
      custom_branch: false,
      description: '',
      status: 'Waiting',
      column: 'in_progress',
      created_at: 0,
      updated_at: 0,
      team_activity_private: false,
      task_ids: ['task_trigger'],
      worktree_dir: '/tmp/ws_new',
    } as never);

    render(App);

    const btn = await screen.findByTestId('trigger-move-in-progress');
    await fireEvent.click(btn);

    await waitFor(() => {
      expect(addToast).toHaveBeenCalled();
    });
    // The undo toast must include the workspace title in its message + the
    // two named actions in the documented order.
    const calls = vi.mocked(addToast).mock.calls;
    const undoCall = calls.find((c) => {
      const opts = c[2] as { actions?: { label: string }[] } | undefined;
      return opts?.actions?.some((a) => a.label === 'Undo create');
    });
    expect(undoCall).toBeDefined();
    expect(undoCall![0]).toMatch(/Shiny new ws/);
    expect(undoCall![1]).toBe('info');
    const opts = undoCall![2] as {
      actions: { label: string }[];
      timeoutMs: number;
    };
    expect(opts.timeoutMs).toBe(10_000);
    expect(opts.actions.map((a) => a.label)).toEqual(['Link to existing instead', 'Undo create']);
  });

  it('Undo create unlinks the new workspace (force=true) and moves the card back to todo', async () => {
    vi.mocked(tasks.listForRepo).mockReturnValue([TASK_NO_WS]);
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_NO_WS,
      workspace_id: 'ws_new',
      column: 'in_progress',
    });

    render(App);

    const btn = await screen.findByTestId('trigger-move-in-progress');
    await fireEvent.click(btn);

    await waitFor(() => expect(addToast).toHaveBeenCalled());
    const undoCall = vi.mocked(addToast).mock.calls.find((c) => {
      const opts = c[2] as { actions?: { label: string }[] } | undefined;
      return opts?.actions?.some((a) => a.label === 'Undo create');
    });
    expect(undoCall).toBeDefined();

    const undoAction = (
      undoCall![2] as {
        actions: { label: string; onClick: () => Promise<void> | void }[];
      }
    ).actions.find((a) => a.label === 'Undo create');
    expect(undoAction).toBeDefined();

    // Reset move so we can detect the new call cleanly.
    vi.mocked(tasks.move).mockClear();
    vi.mocked(tasks.unlink).mockClear();
    vi.mocked(tasks.unlink).mockResolvedValue({ kind: 'unlinked' });
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_NO_WS,
      workspace_id: null,
      column: 'todo',
    });

    await undoAction!.onClick();

    expect(tasks.unlink).toHaveBeenCalledWith('task_trigger', true, REPO_A.id);
    expect(tasks.move).toHaveBeenCalledWith('task_trigger', 'todo', 0);
  });

  it('Link to existing instead opens the picker with cleanupWorkspaceOnPick set to the new ws id', async () => {
    vi.mocked(tasks.listForRepo).mockReturnValue([TASK_NO_WS]);
    vi.mocked(tasks.move).mockResolvedValue({
      ...TASK_NO_WS,
      workspace_id: 'ws_new',
      column: 'in_progress',
    });
    // Picker reads listForRepo on the workspaces store; return one row so
    // the picker dialog body renders something testable.
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_existing',
        repo_id: REPO_A.id,
        title: 'Existing',
        branch: 'feat/old',
        base_branch: 'main',
        custom_branch: false,
        description: '',
        status: 'Waiting',
        column: 'in_progress',
        created_at: 0,
        updated_at: 0,
        team_activity_private: false,
        task_ids: [],
        worktree_dir: '/tmp/ws_existing',
      } as never,
    ]);

    render(App);

    const btn = await screen.findByTestId('trigger-move-in-progress');
    await fireEvent.click(btn);

    await waitFor(() => expect(addToast).toHaveBeenCalled());

    const linkCall = vi.mocked(addToast).mock.calls.find((c) => {
      const opts = c[2] as { actions?: { label: string }[] } | undefined;
      return opts?.actions?.some((a) => a.label === 'Link to existing instead');
    });
    expect(linkCall).toBeDefined();

    const linkAction = (
      linkCall![2] as {
        actions: { label: string; onClick: () => Promise<void> | void }[];
      }
    ).actions.find((a) => a.label === 'Link to existing instead');
    expect(linkAction).toBeDefined();

    await linkAction!.onClick();

    // Picker dialog should now be in the DOM with the existing-ws row visible.
    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Link to workspace/i })).toBeInTheDocument();
    });
    // Picking a row triggers tasks.unlink (force=true, cleanup of the
    // just-auto-created workspace) followed by tasks.link to the chosen one.
    const row = screen.getByTestId('link-picker-row');
    await fireEvent.click(row);
    await waitFor(() => {
      expect(tasks.unlink).toHaveBeenCalledWith('task_trigger', true, REPO_A.id);
    });
    expect(tasks.link).toHaveBeenCalledWith('task_trigger', 'ws_existing', REPO_A.id);
  });
});
