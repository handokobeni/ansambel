# Ansambel — Phase 1a Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Execute **after** Phase 1a-backend is
> merged.

**Goal:** Build the minimal UI shell — typed IPC wrappers for the new `repo` +
`workspace` commands, Svelte 5 runes stores backed by nested `SvelteMap`s,
`TitleBar.svelte` with folder-picker "Add Repo" button, `Sidebar.svelte` with
per-repo workspace list + "New Workspace" form — integrated into `App.svelte`
via a fixed grid layout. Ships with 2 Playwright E2E flows and tags
`v0.2.0-phase1a`.

**Architecture:** Extend `src/lib/ipc.ts` with `api.repo.*` and
`api.workspace.*` typed wrappers. Create two runes-based stores
(`repos.svelte.ts`, `workspaces.svelte.ts`) that hold `SvelteMap` and drive the
UI reactively. Build three components (`TitleBar`, `Sidebar`, updated `App`)
wired to the stores. Use `@tauri-apps/plugin-dialog` for the folder picker. All
state is fetched from the backend on mount — no local duplication.

**Tech Stack:** Svelte 5 (runes), TypeScript strict, Vitest +
`@testing-library/svelte`, Playwright with the existing `TauriDevHarness`
fixture, `@tauri-apps/plugin-dialog` v2.

**Prerequisite:** Phase 1a-backend (the `repo`/`workspace` Tauri commands +
`tauri-plugin-dialog` dep) must be merged first.

---

## Table of Contents

1. [Task 1](#task-1-expand-srclibtypests--srclibipctsvitest-mocks) — Expand
   `src/lib/types.ts` + `src/lib/ipc.ts` + Vitest mocks (~14 tests)
2. [Task 2](#task-2-srclibbstoresrepossveltets--tests) — `ReposStore` class with
   `SvelteMap`, `selectedRepoId`, full CRUD (~8 tests)
3. [Task 3](#task-3-srclibstoresworkspacessveltetss--tests) — `WorkspacesStore`
   with nested `SvelteMap<repoId, SvelteMap<wsId, Workspace>>` (~8 tests)
4. [Task 4](#task-4-srclibcomponentstitlebarsvelte--tests) — `TitleBar.svelte`
   with folder-picker "Add Repo" button (~3 tests)
5. [Task 5](#task-5-srclibcomponentssidebarsvelte--tests) — `Sidebar.svelte`
   workspace list + "New Workspace" inline form (~5 tests)
6. [Task 6](#task-6-srcappsvelte-integration--tests) — `App.svelte` CSS grid
   layout, on-mount hydration (~3 tests)
7. [Task 7](#task-7-testse2ephase-1aadd-repospects) — E2E: add-repo flow (dialog
   shim + fixture git repo)
8. [Task 8](#task-8-testse2ephase-1aworkspace-lifecyclespects) — E2E: workspace
   lifecycle + final validation, tag `v0.2.0-phase1a`

---

### Task 1: Expand `src/lib/types.ts` + `src/lib/ipc.ts` + Vitest mocks

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`
- Create: `src/lib/ipc.test.ts`

- [ ] **Step 1.1: Write failing tests**

```typescript
// src/lib/ipc.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock @tauri-apps/api/core before importing ipc
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { api } from './ipc';
import type { Repo, Workspace } from './types';

const mockRepo: Repo = {
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
};

const mockWorkspace: Workspace = {
  id: 'ws_abc123',
  repo_id: 'repo_abc123',
  branch: 'feat/task-1',
  base_branch: 'main',
  custom_branch: false,
  title: 'Fix login',
  description: 'Fixing the login bug',
  status: 'not_started',
  column: 'todo',
  created_at: 1776000000,
  updated_at: 1776000000,
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('api.repo', () => {
  it('add: invokes add_repo with path and returns Repo', async () => {
    vi.mocked(invoke).mockResolvedValue(mockRepo);
    const result = await api.repo.add('/home/user/my-project');
    expect(invoke).toHaveBeenCalledWith('add_repo', {
      path: '/home/user/my-project',
    });
    expect(result).toEqual(mockRepo);
  });

  it('add: propagates rejection from invoke', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Not a git repo'));
    await expect(api.repo.add('/tmp/not-git')).rejects.toThrow(
      'Not a git repo'
    );
  });

  it('list: invokes list_repos and returns Repo[]', async () => {
    vi.mocked(invoke).mockResolvedValue([mockRepo]);
    const result = await api.repo.list();
    expect(invoke).toHaveBeenCalledWith('list_repos');
    expect(result).toEqual([mockRepo]);
  });

  it('list: returns empty array when no repos', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const result = await api.repo.list();
    expect(result).toEqual([]);
  });

  it('remove: invokes remove_repo with repoId', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.repo.remove('repo_abc123');
    expect(invoke).toHaveBeenCalledWith('remove_repo', {
      repoId: 'repo_abc123',
    });
  });

  it('remove: propagates rejection when repo not found', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Not found'));
    await expect(api.repo.remove('repo_missing')).rejects.toThrow('Not found');
  });

  it('updateGhProfile: invokes update_repo_gh_profile with args', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.repo.updateGhProfile('repo_abc123', 'handokoben');
    expect(invoke).toHaveBeenCalledWith('update_repo_gh_profile', {
      repoId: 'repo_abc123',
      ghProfile: 'handokoben',
    });
  });

  it('updateGhProfile: accepts null to clear gh_profile', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.repo.updateGhProfile('repo_abc123', null);
    expect(invoke).toHaveBeenCalledWith('update_repo_gh_profile', {
      repoId: 'repo_abc123',
      ghProfile: null,
    });
  });
});

describe('api.workspace', () => {
  it('create: invokes create_workspace with args and returns Workspace', async () => {
    vi.mocked(invoke).mockResolvedValue(mockWorkspace);
    const args = {
      repoId: 'repo_abc123',
      title: 'Fix login',
      description: 'Fixing the login bug',
    };
    const result = await api.workspace.create(args);
    expect(invoke).toHaveBeenCalledWith('create_workspace', args);
    expect(result).toEqual(mockWorkspace);
  });

  it('create: forwards optional branchName', async () => {
    vi.mocked(invoke).mockResolvedValue(mockWorkspace);
    const args = {
      repoId: 'repo_abc123',
      title: 'Fix login',
      description: '',
      branchName: 'custom/branch',
    };
    await api.workspace.create(args);
    expect(invoke).toHaveBeenCalledWith('create_workspace', args);
  });

  it('list: invokes list_workspaces with no args when repoId omitted', async () => {
    vi.mocked(invoke).mockResolvedValue([mockWorkspace]);
    const result = await api.workspace.list();
    expect(invoke).toHaveBeenCalledWith('list_workspaces', {
      repoId: undefined,
    });
    expect(result).toEqual([mockWorkspace]);
  });

  it('list: invokes list_workspaces with repoId filter', async () => {
    vi.mocked(invoke).mockResolvedValue([mockWorkspace]);
    await api.workspace.list('repo_abc123');
    expect(invoke).toHaveBeenCalledWith('list_workspaces', {
      repoId: 'repo_abc123',
    });
  });

  it('remove: invokes remove_workspace with workspaceId', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.workspace.remove('ws_abc123');
    expect(invoke).toHaveBeenCalledWith('remove_workspace', {
      workspaceId: 'ws_abc123',
    });
  });

  it('remove: propagates rejection when workspace not found', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Workspace not found'));
    await expect(api.workspace.remove('ws_missing')).rejects.toThrow(
      'Workspace not found'
    );
  });
});
```

- [ ] **Step 1.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/ipc.test.ts 2>&1 | tail -20
```

Expected: import errors — `api.repo` and `api.workspace` not yet defined in
`ipc.ts`; type errors from missing `Repo`/`Workspace` in `types.ts`.

- [ ] **Step 1.3: Implement**

Overwrite `src/lib/types.ts`:

```typescript
// src/lib/types.ts

export type WorkspaceStatus =
  | 'not_started'
  | 'running'
  | 'waiting'
  | 'done'
  | 'error';

export type KanbanColumn = 'todo' | 'in_progress' | 'review' | 'done';

export type Repo = {
  id: string;
  name: string;
  path: string;
  gh_profile: string | null;
  default_branch: string;
  created_at: number;
  updated_at: number;
};

export type Workspace = {
  id: string;
  repo_id: string;
  branch: string;
  base_branch: string;
  custom_branch: boolean;
  title: string;
  description: string;
  status: WorkspaceStatus;
  column: KanbanColumn;
  created_at: number;
  updated_at: number;
};

export type AppSettings = {
  schema_version: number;
  theme: string;
  selected_repo_id: string | null;
  selected_workspace_id: string | null;
  recent_repos: string[];
  window_width: number;
  window_height: number;
  onboarding_completed: boolean;
};

export type CreateWorkspaceArgs = {
  repoId: string;
  title: string;
  description: string;
  branchName?: string;
};
```

Overwrite `src/lib/ipc.ts`:

```typescript
// src/lib/ipc.ts
import { invoke } from '@tauri-apps/api/core';
import type { Repo, Workspace, CreateWorkspaceArgs } from './types';

export const api = {
  system: {
    getAppVersion: (): Promise<string> => invoke('get_app_version'),
  },

  repo: {
    add: (path: string): Promise<Repo> => invoke('add_repo', { path }),

    list: (): Promise<Repo[]> => invoke('list_repos'),

    remove: (repoId: string): Promise<void> =>
      invoke('remove_repo', { repoId }),

    updateGhProfile: (
      repoId: string,
      ghProfile: string | null
    ): Promise<void> => invoke('update_repo_gh_profile', { repoId, ghProfile }),
  },

  workspace: {
    create: (args: CreateWorkspaceArgs): Promise<Workspace> =>
      invoke('create_workspace', args),

    list: (repoId?: string): Promise<Workspace[]> =>
      invoke('list_workspaces', { repoId }),

    remove: (workspaceId: string): Promise<void> =>
      invoke('remove_workspace', { workspaceId }),
  },
};
```

- [ ] **Step 1.4: Run tests to verify they pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/ipc.test.ts 2>&1 | tail -20
```

Expected: `14 tests passed` — all `api.repo.*` and `api.workspace.*` calls match
expected invoke signatures.

- [ ] **Step 1.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/types.ts src/lib/ipc.ts src/lib/ipc.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add Repo/Workspace types and api.repo/workspace IPC wrappers

Mirror Rust serde types in types.ts; extend ipc.ts with api.repo.{add,
list, remove, updateGhProfile} and api.workspace.{create, list, remove}.
14 Vitest tests verify every invoke signature.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `src/lib/stores/repos.svelte.ts` + tests

**Files:**

- Create: `src/lib/stores/repos.svelte.ts`
- Create: `src/lib/stores/repos.svelte.test.ts`

- [ ] **Step 2.1: Write failing tests**

```typescript
// src/lib/stores/repos.svelte.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    repo: {
      add: vi.fn(),
      list: vi.fn(),
      remove: vi.fn(),
      updateGhProfile: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { ReposStore } from './repos.svelte';
import type { Repo } from '$lib/types';

const makeRepo = (overrides: Partial<Repo> = {}): Repo => ({
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('ReposStore', () => {
  it('load: populates the map from api.repo.list', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    const store = new ReposStore();
    await store.load();
    expect(store.repos.get('repo_abc123')).toEqual(repo);
  });

  it('load: map is empty when backend returns []', async () => {
    vi.mocked(api.repo.list).mockResolvedValue([]);
    const store = new ReposStore();
    await store.load();
    expect(store.repos.size).toBe(0);
  });

  it('add: calls api.repo.add and inserts returned Repo into map', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.add).mockResolvedValue(repo);
    const store = new ReposStore();
    const result = await store.add('/home/user/my-project');
    expect(api.repo.add).toHaveBeenCalledWith('/home/user/my-project');
    expect(result).toEqual(repo);
    expect(store.repos.get('repo_abc123')).toEqual(repo);
  });

  it('remove: calls api.repo.remove and deletes from map', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    vi.mocked(api.repo.remove).mockResolvedValue(undefined);
    const store = new ReposStore();
    await store.load();
    await store.remove('repo_abc123');
    expect(api.repo.remove).toHaveBeenCalledWith('repo_abc123');
    expect(store.repos.has('repo_abc123')).toBe(false);
  });

  it('updateGhProfile: calls api and updates the in-map entry', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    vi.mocked(api.repo.updateGhProfile).mockResolvedValue(undefined);
    const store = new ReposStore();
    await store.load();
    await store.updateGhProfile('repo_abc123', 'handokoben');
    expect(api.repo.updateGhProfile).toHaveBeenCalledWith(
      'repo_abc123',
      'handokoben'
    );
    expect(store.repos.get('repo_abc123')?.gh_profile).toBe('handokoben');
  });

  it('select: sets selectedRepoId', () => {
    const store = new ReposStore();
    store.select('repo_abc123');
    expect(store.selectedRepoId).toBe('repo_abc123');
  });

  it('select: accepts null to deselect', () => {
    const store = new ReposStore();
    store.select('repo_abc123');
    store.select(null);
    expect(store.selectedRepoId).toBeNull();
  });

  it('getSelected: returns null when nothing selected', () => {
    const store = new ReposStore();
    expect(store.getSelected()).toBeNull();
  });

  it('getSelected: returns the Repo matching selectedRepoId', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    const store = new ReposStore();
    await store.load();
    store.select('repo_abc123');
    expect(store.getSelected()).toEqual(repo);
  });
});
```

- [ ] **Step 2.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/repos.svelte.test.ts 2>&1 | tail -20
```

Expected: `Cannot find module './repos.svelte'` — file does not exist yet.

- [ ] **Step 2.3: Implement**

```typescript
// src/lib/stores/repos.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import type { Repo } from '$lib/types';

export class ReposStore {
  readonly repos = new SvelteMap<string, Repo>();
  #selectedRepoId = $state<string | null>(null);

  get selectedRepoId(): string | null {
    return this.#selectedRepoId;
  }

  async load(): Promise<void> {
    const list = await api.repo.list();
    this.repos.clear();
    for (const repo of list) {
      this.repos.set(repo.id, repo);
    }
  }

  async add(path: string): Promise<Repo> {
    const repo = await api.repo.add(path);
    this.repos.set(repo.id, repo);
    return repo;
  }

  async remove(id: string): Promise<void> {
    await api.repo.remove(id);
    this.repos.delete(id);
    if (this.#selectedRepoId === id) {
      this.#selectedRepoId = null;
    }
  }

  async updateGhProfile(id: string, profile: string | null): Promise<void> {
    await api.repo.updateGhProfile(id, profile);
    const existing = this.repos.get(id);
    if (existing) {
      this.repos.set(id, { ...existing, gh_profile: profile });
    }
  }

  select(id: string | null): void {
    this.#selectedRepoId = id;
  }

  getSelected(): Repo | null {
    if (this.#selectedRepoId === null) return null;
    return this.repos.get(this.#selectedRepoId) ?? null;
  }
}

export const repos = new ReposStore();
```

- [ ] **Step 2.4: Run tests to verify they pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/repos.svelte.test.ts 2>&1 | tail -20
```

Expected: `8 tests passed` — all ReposStore operations verified.

- [ ] **Step 2.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/stores/repos.svelte.ts src/lib/stores/repos.svelte.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add ReposStore with SvelteMap and selectedRepoId rune

Class-based store wrapping api.repo.* — SvelteMap<id, Repo> for reactive
collection, #selectedRepoId $state rune for selection. 8 Vitest tests
covering load, add, remove, updateGhProfile, select, getSelected.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `src/lib/stores/workspaces.svelte.ts` + tests

**Files:**

- Create: `src/lib/stores/workspaces.svelte.ts`
- Create: `src/lib/stores/workspaces.svelte.test.ts`

- [ ] **Step 3.1: Write failing tests**

```typescript
// src/lib/stores/workspaces.svelte.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    workspace: {
      create: vi.fn(),
      list: vi.fn(),
      remove: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { WorkspacesStore } from './workspaces.svelte';
import type { Workspace } from '$lib/types';

const makeWorkspace = (overrides: Partial<Workspace> = {}): Workspace => ({
  id: 'ws_abc123',
  repo_id: 'repo_abc123',
  branch: 'feat/task-1',
  base_branch: 'main',
  custom_branch: false,
  title: 'Fix login',
  description: 'Fixing the login bug',
  status: 'not_started',
  column: 'todo',
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('WorkspacesStore', () => {
  it('loadForRepo: populates nested map for a repoId', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    expect(api.workspace.list).toHaveBeenCalledWith('repo_abc123');
    expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')).toEqual(ws);
  });

  it('loadForRepo: empty inner map when no workspaces returned', async () => {
    vi.mocked(api.workspace.list).mockResolvedValue([]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    expect(store.byRepo.get('repo_abc123')?.size).toBe(0);
  });

  it('create: calls api and inserts into nested map', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.create).mockResolvedValue(ws);
    const store = new WorkspacesStore();
    const result = await store.create({
      repoId: 'repo_abc123',
      title: 'Fix login',
      description: 'Fixing the login bug',
    });
    expect(result).toEqual(ws);
    expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')).toEqual(ws);
  });

  it('remove: calls api and deletes from nested map', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    vi.mocked(api.workspace.remove).mockResolvedValue(undefined);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    await store.remove('ws_abc123', 'repo_abc123');
    expect(api.workspace.remove).toHaveBeenCalledWith('ws_abc123');
    expect(store.byRepo.get('repo_abc123')?.has('ws_abc123')).toBe(false);
  });

  it('listForRepo: returns workspaces array for a repoId', async () => {
    const ws1 = makeWorkspace({ id: 'ws_111111' });
    const ws2 = makeWorkspace({ id: 'ws_222222' });
    vi.mocked(api.workspace.list).mockResolvedValue([ws1, ws2]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    const list = store.listForRepo('repo_abc123');
    expect(list).toHaveLength(2);
    expect(list.map((w) => w.id)).toContain('ws_111111');
    expect(list.map((w) => w.id)).toContain('ws_222222');
  });

  it('listForRepo: returns [] for unknown repoId', () => {
    const store = new WorkspacesStore();
    expect(store.listForRepo('repo_unknown')).toEqual([]);
  });

  it('select: sets selectedWorkspaceId', () => {
    const store = new WorkspacesStore();
    store.select('ws_abc123');
    expect(store.selectedWorkspaceId).toBe('ws_abc123');
  });

  it('getSelected: returns null when nothing selected', () => {
    const store = new WorkspacesStore();
    expect(store.getSelected()).toBeNull();
  });

  it('getSelected: returns the Workspace matching selectedWorkspaceId', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    store.select('ws_abc123');
    expect(store.getSelected()).toEqual(ws);
  });
});
```

- [ ] **Step 3.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/workspaces.svelte.test.ts 2>&1 | tail -20
```

Expected: `Cannot find module './workspaces.svelte'` — file does not exist yet.

- [ ] **Step 3.3: Implement**

```typescript
// src/lib/stores/workspaces.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import type { Workspace, CreateWorkspaceArgs } from '$lib/types';

export class WorkspacesStore {
  readonly byRepo = new SvelteMap<string, SvelteMap<string, Workspace>>();
  #selectedWorkspaceId = $state<string | null>(null);

  get selectedWorkspaceId(): string | null {
    return this.#selectedWorkspaceId;
  }

  private getOrCreateInner(repoId: string): SvelteMap<string, Workspace> {
    let inner = this.byRepo.get(repoId);
    if (!inner) {
      inner = new SvelteMap<string, Workspace>();
      this.byRepo.set(repoId, inner);
    }
    return inner;
  }

  async loadForRepo(repoId: string): Promise<void> {
    const list = await api.workspace.list(repoId);
    const inner = this.getOrCreateInner(repoId);
    inner.clear();
    for (const ws of list) {
      inner.set(ws.id, ws);
    }
  }

  async create(args: CreateWorkspaceArgs): Promise<Workspace> {
    const ws = await api.workspace.create(args);
    const inner = this.getOrCreateInner(ws.repo_id);
    inner.set(ws.id, ws);
    return ws;
  }

  async remove(id: string, repoId: string): Promise<void> {
    await api.workspace.remove(id);
    this.byRepo.get(repoId)?.delete(id);
    if (this.#selectedWorkspaceId === id) {
      this.#selectedWorkspaceId = null;
    }
  }

  listForRepo(repoId: string): Workspace[] {
    const inner = this.byRepo.get(repoId);
    if (!inner) return [];
    return [...inner.values()];
  }

  select(id: string | null): void {
    this.#selectedWorkspaceId = id;
  }

  getSelected(): Workspace | null {
    if (this.#selectedWorkspaceId === null) return null;
    for (const inner of this.byRepo.values()) {
      const ws = inner.get(this.#selectedWorkspaceId);
      if (ws) return ws;
    }
    return null;
  }
}

export const workspaces = new WorkspacesStore();
```

- [ ] **Step 3.4: Run tests to verify they pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/workspaces.svelte.test.ts 2>&1 | tail -20
```

Expected: `9 tests passed` — nested SvelteMap operations all verified.

- [ ] **Step 3.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/stores/workspaces.svelte.ts src/lib/stores/workspaces.svelte.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add WorkspacesStore with nested SvelteMap<repoId, SvelteMap<wsId>>

Nested reactive map: outer keyed by repoId, inner keyed by wsId.
Methods: loadForRepo, create, remove, listForRepo, select, getSelected.
9 Vitest tests covering all operations including deselection on remove.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `src/lib/components/TitleBar.svelte` + tests

**Files:**

- Create: `src/lib/components/TitleBar.svelte`
- Create: `src/lib/components/TitleBar.test.ts`

- [ ] **Step 4.1: Write failing tests**

```typescript
// src/lib/components/TitleBar.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import TitleBar from './TitleBar.svelte';

// Mock @tauri-apps/plugin-dialog
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

// Mock the repos store
vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: null as string | null,
    repos: new Map(),
    add: vi.fn(),
    getSelected: vi.fn(() => null),
  },
}));

import { open } from '@tauri-apps/plugin-dialog';
import { repos } from '$lib/stores/repos.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
});

describe('TitleBar', () => {
  it('renders "No repo selected" when no repo is selected', () => {
    render(TitleBar);
    expect(screen.getByText('No repo selected')).toBeInTheDocument();
  });

  it('shows selected repo name when a repo is selected', () => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    render(TitleBar);
    expect(screen.getByText('my-project')).toBeInTheDocument();
  });

  it('clicking "Add Repo" opens folder dialog and calls repos.add with returned path', async () => {
    vi.mocked(open).mockResolvedValue('/home/user/new-project');
    vi.mocked(repos.add).mockResolvedValue({
      id: 'repo_new111',
      name: 'new-project',
      path: '/home/user/new-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000001,
      updated_at: 1776000001,
    });
    render(TitleBar);
    const addBtn = screen.getByRole('button', { name: /add repo/i });
    await fireEvent.click(addBtn);
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false });
    expect(repos.add).toHaveBeenCalledWith('/home/user/new-project');
  });
});
```

- [ ] **Step 4.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/TitleBar.test.ts 2>&1 | tail -20
```

Expected: `Cannot find module './TitleBar.svelte'` — component not created yet.

- [ ] **Step 4.3: Implement**

First install `@tauri-apps/plugin-dialog` if not already present:

```bash
cd /home/handokobeni/Work/ai-editor
bun add @tauri-apps/plugin-dialog
```

```svelte
<!-- src/lib/components/TitleBar.svelte -->
<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { repos } from '$lib/stores/repos.svelte';

  let adding = $state(false);

  const selectedRepo = $derived(repos.getSelected());

  async function handleAddRepo() {
    if (adding) return;
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== 'string' || !selected) return;
    adding = true;
    try {
      const repo = await repos.add(selected);
      repos.select(repo.id);
    } catch (err) {
      console.error('Failed to add repo:', err);
    } finally {
      adding = false;
    }
  }
</script>

<header
  class="flex items-center justify-between h-10 px-3 bg-[var(--bg-titlebar)] border-b border-[var(--border)] flex-shrink-0 select-none"
>
  <div class="flex items-center gap-2">
    <span
      class="text-sm font-semibold text-[var(--text-primary)] max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap"
    >
      {#if selectedRepo}
        {selectedRepo.name}
      {:else}
        <span class="text-[var(--text-muted)]">No repo selected</span>
      {/if}
    </span>
  </div>

  <div class="flex items-center gap-2">
    <button
      class="flex items-center gap-1 px-2 py-1 text-xs font-semibold rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
      onclick={handleAddRepo}
      disabled={adding}
      aria-label="Add Repo"
    >
      {adding ? 'Adding…' : 'Add Repo'}
    </button>
  </div>
</header>
```

- [ ] **Step 4.4: Run tests to verify they pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/TitleBar.test.ts 2>&1 | tail -20
```

Expected: `3 tests passed` — renders no-repo state, shows repo name, dialog flow
verified.

- [ ] **Step 4.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/TitleBar.svelte src/lib/components/TitleBar.test.ts package.json bun.lockb
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add TitleBar.svelte with repo name display and Add Repo button

Shows selected repo name (or "No repo selected"). Add Repo button opens
@tauri-apps/plugin-dialog folder picker, calls repos.add(path), and
selects the new repo. 3 Vitest tests with mocked dialog + store.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: `src/lib/components/Sidebar.svelte` + tests

**Files:**

- Create: `src/lib/components/Sidebar.svelte`
- Create: `src/lib/components/Sidebar.test.ts`

- [ ] **Step 5.1: Write failing tests**

```typescript
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

beforeEach(() => {
  vi.clearAllMocks();
});

describe('Sidebar', () => {
  it('renders workspace titles from the store for the selected repo', () => {
    render(Sidebar);
    expect(screen.getByText('Fix login')).toBeInTheDocument();
    expect(screen.getByText('Add dark mode')).toBeInTheDocument();
  });

  it('shows amber status dot for running workspace and olive for waiting', () => {
    const { container } = render(Sidebar);
    const dots = container.querySelectorAll('[data-status-dot]');
    expect(dots[0]).toHaveAttribute('data-status', 'running');
    expect(dots[1]).toHaveAttribute('data-status', 'waiting');
  });

  it('clicking a workspace row calls workspaces.select with its id', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByText('Fix login'));
    expect(workspaces.select).toHaveBeenCalledWith('ws_abc123');
  });

  it('"New Workspace" button reveals the inline form', async () => {
    render(Sidebar);
    expect(
      screen.queryByPlaceholderText(/workspace title/i)
    ).not.toBeInTheDocument();
    await fireEvent.click(
      screen.getByRole('button', { name: /new workspace/i })
    );
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
    });
    render(Sidebar);
    await fireEvent.click(
      screen.getByRole('button', { name: /new workspace/i })
    );
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
});
```

- [ ] **Step 5.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/Sidebar.test.ts 2>&1 | tail -20
```

Expected: `Cannot find module './Sidebar.svelte'` — component not created yet.

- [ ] **Step 5.3: Implement**

```svelte
<!-- src/lib/components/Sidebar.svelte -->
<script lang="ts">
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import type { WorkspaceStatus } from '$lib/types';

  const selectedRepo = $derived(repos.getSelected());
  const workspaceList = $derived(
    selectedRepo ? workspaces.listForRepo(selectedRepo.id) : []
  );

  let showForm = $state(false);
  let formTitle = $state('');
  let formDescription = $state('');
  let formBranch = $state('');
  let formSubmitting = $state(false);

  function statusDotClass(status: WorkspaceStatus): string {
    if (status === 'running') return 'bg-amber-400';
    if (status === 'waiting') return 'bg-[var(--status-ok)]';
    return 'bg-[var(--text-muted)]';
  }

  function handleSelectWorkspace(id: string) {
    workspaces.select(id);
  }

  async function handleRemoveWorkspace(
    e: MouseEvent,
    wsId: string,
    repoId: string
  ) {
    e.stopPropagation();
    if (
      !window.confirm(
        'Remove this workspace? The git worktree will be deleted.'
      )
    )
      return;
    try {
      await workspaces.remove(wsId, repoId);
    } catch (err) {
      console.error('Failed to remove workspace:', err);
    }
  }

  async function handleCreateSubmit(e: Event) {
    e.preventDefault();
    if (!selectedRepo || !formTitle.trim()) return;
    formSubmitting = true;
    try {
      await workspaces.create({
        repoId: selectedRepo.id,
        title: formTitle.trim(),
        description: formDescription.trim(),
        branchName: formBranch.trim() || undefined,
      });
      formTitle = '';
      formDescription = '';
      formBranch = '';
      showForm = false;
    } catch (err) {
      console.error('Failed to create workspace:', err);
    } finally {
      formSubmitting = false;
    }
  }

  function handleCancelForm() {
    showForm = false;
    formTitle = '';
    formDescription = '';
    formBranch = '';
  }
</script>

<aside
  class="flex flex-col h-full w-full bg-[var(--bg-sidebar)] border-r border-[var(--border)] overflow-hidden"
>
  <div
    class="flex items-center justify-between px-3 py-2 border-b border-[var(--border)]"
  >
    <span
      class="text-xs font-semibold uppercase tracking-wider text-[var(--text-muted)]"
    >
      Workspaces
    </span>
    <button
      class="text-xs font-semibold px-2 py-0.5 rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
      onclick={() => {
        showForm = !showForm;
      }}
      aria-label="New Workspace"
    >
      + New Workspace
    </button>
  </div>

  {#if showForm}
    <!-- Inline new workspace form -->
    <form
      class="flex flex-col gap-2 px-3 py-2 border-b border-[var(--border)] bg-[var(--bg-card)]"
      onsubmit={handleCreateSubmit}
    >
      <input
        class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        type="text"
        placeholder="Workspace title"
        bind:value={formTitle}
        required
      />
      <textarea
        class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none"
        placeholder="Description (optional)"
        rows={2}
        bind:value={formDescription}
      ></textarea>
      <input
        class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        type="text"
        placeholder="Branch name (optional)"
        bind:value={formBranch}
      />
      <div class="flex gap-2">
        <button
          type="submit"
          class="flex-1 py-1 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 cursor-pointer"
          disabled={formSubmitting || !formTitle.trim()}
          aria-label="Create"
        >
          {formSubmitting ? 'Creating…' : 'Create'}
        </button>
        <button
          type="button"
          class="py-1 px-2 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          onclick={handleCancelForm}
        >
          Cancel
        </button>
      </div>
    </form>
  {/if}

  <ul class="flex-1 overflow-y-auto py-1">
    {#each workspaceList as ws (ws.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <li
        class="group flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
        class:bg-[var(--bg-active)]={workspaces.selectedWorkspaceId === ws.id}
        onclick={() => handleSelectWorkspace(ws.id)}
      >
        <!-- Status dot -->
        <span
          class="w-2 h-2 rounded-full flex-shrink-0 {statusDotClass(ws.status)}"
          data-status-dot
          data-status={ws.status}
          aria-label="Status: {ws.status}"
        ></span>

        <!-- Title -->
        <span
          class="flex-1 text-xs text-[var(--text-secondary)] overflow-hidden text-ellipsis whitespace-nowrap group-hover:text-[var(--text-primary)] transition-colors"
        >
          {ws.title}
        </span>

        <!-- Remove button -->
        <button
          class="opacity-0 group-hover:opacity-100 flex items-center justify-center w-4 h-4 rounded text-[var(--text-muted)] hover:text-[var(--error)] hover:bg-[var(--error-bg)] transition-all cursor-pointer"
          onclick={(e) => handleRemoveWorkspace(e, ws.id, ws.repo_id)}
          aria-label="Remove workspace"
          title="Remove workspace"
        >
          ×
        </button>
      </li>
    {:else}
      <li class="px-3 py-3 text-xs text-[var(--text-muted)] text-center">
        {#if selectedRepo}
          No workspaces yet
        {:else}
          Select a repo to see workspaces
        {/if}
      </li>
    {/each}
  </ul>
</aside>
```

- [ ] **Step 5.4: Run tests to verify they pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/Sidebar.test.ts 2>&1 | tail -20
```

Expected: `5 tests passed` — list renders, status dots, select row, form reveal,
create submit.

- [ ] **Step 5.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/Sidebar.svelte src/lib/components/Sidebar.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): add Sidebar.svelte with workspace list and New Workspace form

Lists workspaces for selected repo with status dots (amber=running,
olive=waiting, gray=others). New Workspace button reveals inline form.
Remove X calls workspaces.remove after native confirm. 5 Vitest tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: `src/App.svelte` integration + tests

**Files:**

- Modify: `src/App.svelte`
- Create: `src/App.test.ts`

- [ ] **Step 6.1: Write failing tests**

```typescript
// src/App.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import App from './App.svelte';

vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: null as string | null,
    load: vi.fn().mockResolvedValue(undefined),
    getSelected: vi.fn(() => null),
    repos: new Map(),
  },
}));

vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    selectedWorkspaceId: null as string | null,
    loadForRepo: vi.fn().mockResolvedValue(undefined),
    listForRepo: vi.fn(() => []),
    select: vi.fn(),
    create: vi.fn(),
    remove: vi.fn(),
    getSelected: vi.fn(() => null),
  },
}));

import { repos } from '$lib/stores/repos.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  vi.mocked(workspaces.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
  (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId =
    null;
});

describe('App', () => {
  it('renders TitleBar and Sidebar shell without errors', () => {
    render(App);
    // TitleBar renders
    expect(screen.getByText('No repo selected')).toBeInTheDocument();
    // Sidebar renders empty state
    expect(screen.getByText(/Select a repo/i)).toBeInTheDocument();
  });

  it('shows "Select or create a workspace" placeholder in main area when none selected', () => {
    render(App);
    expect(
      screen.getByText(/select or create a workspace/i)
    ).toBeInTheDocument();
  });

  it('shows selected workspace title in main area when a workspace is selected', () => {
    vi.mocked(workspaces.getSelected).mockReturnValue({
      id: 'ws_abc123',
      repo_id: 'repo_abc123',
      branch: 'feat/task-1',
      base_branch: 'main',
      custom_branch: false,
      title: 'Fix login',
      description: '',
      status: 'not_started',
      column: 'todo',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId =
      'ws_abc123';
    render(App);
    expect(screen.getByText('Workspace: Fix login')).toBeInTheDocument();
  });
});
```

- [ ] **Step 6.2: Run tests to verify they fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/App.test.ts 2>&1 | tail -20
```

Expected: failures — current `App.svelte` renders a version string only, not
TitleBar/Sidebar grid.

- [ ] **Step 6.3: Implement**

```svelte
<!-- src/App.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';

  const selectedWorkspace = $derived(workspaces.getSelected());

  onMount(async () => {
    await repos.load();
    const selected = repos.getSelected();
    if (selected) {
      await workspaces.loadForRepo(selected.id);
    }
  });
</script>

<div
  class="app-shell"
  style="
    display: grid;
    grid-template-rows: auto 1fr;
    grid-template-columns: 260px 1fr;
    height: 100vh;
    overflow: hidden;
  "
>
  <!-- TitleBar: spans both columns -->
  <div style="grid-column: 1 / -1; grid-row: 1;">
    <TitleBar />
  </div>

  <!-- Sidebar: bottom-left -->
  <div style="grid-column: 1; grid-row: 2; overflow: hidden;">
    <Sidebar />
  </div>

  <!-- Main: bottom-right -->
  <main
    class="bg-[var(--bg-base)] overflow-auto flex items-center justify-center"
    style="grid-column: 2; grid-row: 2;"
  >
    {#if selectedWorkspace}
      <section
        class="flex flex-col items-center gap-2 text-[var(--text-secondary)]"
      >
        <p class="text-base font-semibold text-[var(--text-primary)]">
          Workspace: {selectedWorkspace.title}
        </p>
        <p class="text-xs text-[var(--text-muted)]">
          Branch: {selectedWorkspace.branch}
        </p>
        <!-- Phase 1b/c will replace this placeholder -->
      </section>
    {:else}
      <p class="text-sm text-[var(--text-muted)]">
        Select or create a workspace
      </p>
    {/if}
  </main>
</div>
```

- [ ] **Step 6.4: Run tests to verify they pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/App.test.ts 2>&1 | tail -20
```

Expected: `3 tests passed` — shell renders, placeholder shown, workspace title
shown on selection.

- [ ] **Step 6.5: Run full unit test suite + type-check**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test:coverage 2>&1 | tail -30
bun run check 2>&1 | tail -10
```

Expected: all unit tests pass, coverage ≥ 95% on changed files, `svelte-check` 0
errors.

- [ ] **Step 6.6: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/App.svelte src/App.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1a): wire App.svelte to CSS grid layout with TitleBar + Sidebar

Grid: auto titlebar (spans 2 cols) + 260px sidebar + 1fr main. On mount
loads repos then workspace list for selected repo. Main shows workspace
title or "Select or create a workspace" placeholder. 3 Vitest tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: `tests/e2e/phase-1a/add-repo.spec.ts`

**Files:**

- Create: `tests/e2e/phase-1a/add-repo.spec.ts`
- Create: `tests/e2e/fixtures/mock-repo/.gitkeep` (directory placeholder; actual
  repo created at test runtime)

- [ ] **Step 7.1: Write the E2E spec**

```typescript
// tests/e2e/phase-1a/add-repo.spec.ts
import { test, expect } from '@playwright/test';
import { execSync, execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

// Path to fixture repo — created in beforeAll
let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  // Create a real git repo in a temp dir so the backend can canonicalize + detect default branch
  const tmpDir = fs.mkdtempSync(
    path.join(os.tmpdir(), 'ansambel-e2e-mock-repo-')
  );
  FIXTURE_REPO_PATH = tmpDir;

  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 'test@example.com'], {
    cwd: tmpDir,
  });
  execFileSync('git', ['config', 'user.name', 'Test'], { cwd: tmpDir });
  // Need at least one commit for a valid git repo with a default branch
  execFileSync('git', ['commit', '--allow-empty', '-m', 'initial'], {
    cwd: tmpDir,
  });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

test.beforeEach(async ({ page }) => {
  // Shim the Tauri dialog plugin so open() returns our fixture path without
  // showing a native dialog (which can't be driven in a headless Playwright session)
  await page.addInitScript((fixturePath: string) => {
    // Override the module before app scripts load
    Object.defineProperty(window, '__TAURI_DIALOG_OPEN_MOCK__', {
      value: fixturePath,
      writable: false,
    });
    // Intercept the Tauri invoke for file dialog at the IPC layer
    const origInvoke = (
      window as unknown as {
        __TAURI_INTERNALS__?: { invoke?: (...args: unknown[]) => unknown };
      }
    ).__TAURI_INTERNALS__?.invoke;
    if (!origInvoke) return;
    const internals = (
      window as unknown as {
        __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => unknown };
      }
    ).__TAURI_INTERNALS__;
    internals.invoke = (cmd: string, ...args: unknown[]) => {
      if (cmd === 'plugin:dialog|open') {
        return Promise.resolve(fixturePath);
      }
      return origInvoke(cmd, ...args);
    };
  }, FIXTURE_REPO_PATH);
});

test('Add Repo: clicking Add Repo opens dialog, backend adds repo, TitleBar shows repo name', async ({
  page,
}) => {
  // Wait for app to be ready
  await page.waitForSelector('header', { timeout: 10000 });

  // Initially shows "No repo selected"
  await expect(page.getByText('No repo selected')).toBeVisible();

  // Click the Add Repo button
  const addBtn = page.getByRole('button', { name: /add repo/i });
  await expect(addBtn).toBeVisible();
  await addBtn.click();

  // After dialog mock resolves and backend processes, TitleBar should show repo name
  // The repo name is derived from the folder name of the fixture path
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 5000 });

  // "No repo selected" should be gone
  await expect(page.getByText('No repo selected')).not.toBeVisible();
});
```

- [ ] **Step 7.2: Create the fixture placeholder directory**

```bash
mkdir -p /home/handokobeni/Work/ai-editor/tests/e2e/fixtures/mock-repo
touch /home/handokobeni/Work/ai-editor/tests/e2e/fixtures/mock-repo/.gitkeep
```

- [ ] **Step 7.3: Verify the spec runs (with Tauri dev harness)**

The Playwright config must be set up to launch `bun tauri dev` as the app under
test. If `playwright.config.ts` already exists from Phase 0, ensure it
references the Tauri webdriver port. Run:

```bash
cd /home/handokobeni/Work/ai-editor
bun run e2e tests/e2e/phase-1a/add-repo.spec.ts 2>&1 | tail -30
```

Expected: `1 passed` — TitleBar updates to show the fixture repo name after Add
Repo click.

- [ ] **Step 7.4: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add tests/e2e/phase-1a/add-repo.spec.ts tests/e2e/fixtures/mock-repo/.gitkeep
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1a): E2E add-repo spec with dialog shim and fixture git repo

Creates a real git repo in tmpdir at test runtime via child_process git
init + empty commit. Shims Tauri dialog IPC to return the fixture path.
Asserts TitleBar shows repo name after Add Repo flow completes.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: `tests/e2e/phase-1a/workspace-lifecycle.spec.ts` + final validation + `v0.2.0-phase1a` tag

**Files:**

- Create: `tests/e2e/phase-1a/workspace-lifecycle.spec.ts`

- [ ] **Step 8.1: Write the E2E spec**

```typescript
// tests/e2e/phase-1a/workspace-lifecycle.spec.ts
import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

// ---------------------------------------------------------------------------
// Fixture setup (shared across tests in this file)
// ---------------------------------------------------------------------------

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(
    path.join(os.tmpdir(), 'ansambel-e2e-ws-lifecycle-')
  );
  FIXTURE_REPO_PATH = tmpDir;
  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 'test@example.com'], {
    cwd: tmpDir,
  });
  execFileSync('git', ['config', 'user.name', 'Test'], { cwd: tmpDir });
  execFileSync('git', ['commit', '--allow-empty', '-m', 'initial'], {
    cwd: tmpDir,
  });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

// Shim Tauri dialog on every page load
test.beforeEach(async ({ page }) => {
  await page.addInitScript((fixturePath: string) => {
    const internals = (
      window as unknown as {
        __TAURI_INTERNALS__?: { invoke?: (...a: unknown[]) => unknown };
      }
    ).__TAURI_INTERNALS__;
    if (!internals?.invoke) return;
    const orig = internals.invoke.bind(internals);
    internals.invoke = (cmd: string, ...args: unknown[]) => {
      if (cmd === 'plugin:dialog|open') return Promise.resolve(fixturePath);
      return orig(cmd, ...args);
    };
  }, FIXTURE_REPO_PATH);
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function addRepo(page: import('@playwright/test').Page) {
  await page.waitForSelector('header', { timeout: 10000 });
  await page.getByRole('button', { name: /add repo/i }).click();
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test('Workspace lifecycle: create then remove', async ({ page }) => {
  // Step 1: Add repo (reuses the dialog shim from beforeEach)
  await addRepo(page);

  // Step 2: Click "New Workspace" to reveal inline form
  const newWsBtn = page.getByRole('button', { name: /new workspace/i });
  await expect(newWsBtn).toBeVisible();
  await newWsBtn.click();

  // Step 3: Fill the form
  await page.getByPlaceholder(/workspace title/i).fill('E2E test task');
  await page.getByPlaceholder(/description/i).fill('Created by Playwright');

  // Step 4: Submit
  await page.getByRole('button', { name: /^create$/i }).click();

  // Step 5: Assert workspace row appears in Sidebar
  await expect(page.getByText('E2E test task')).toBeVisible({ timeout: 8000 });

  // Step 6: Remove workspace — mock window.confirm to return true
  await page.evaluate(() => {
    window.confirm = () => true;
  });

  // Hover to reveal the remove button (opacity-0 → group-hover:opacity-100)
  const wsRow = page.locator('li').filter({ hasText: 'E2E test task' });
  await wsRow.hover();
  const removeBtn = wsRow.getByRole('button', { name: /remove workspace/i });
  await removeBtn.click();

  // Step 7: Assert row is gone
  await expect(page.getByText('E2E test task')).not.toBeVisible({
    timeout: 5000,
  });
});
```

- [ ] **Step 8.2: Run the E2E spec**

```bash
cd /home/handokobeni/Work/ai-editor
bun run e2e tests/e2e/phase-1a/workspace-lifecycle.spec.ts 2>&1 | tail -30
```

Expected: `1 passed` — workspace created and removed through the UI.

- [ ] **Step 8.3: Run full validation suite**

```bash
cd /home/handokobeni/Work/ai-editor

# 1. Rust tests
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -5

# 2. Rust fmt + clippy
cargo fmt --check 2>&1
cargo clippy -- -D warnings 2>&1 | tail -10

# 3. Frontend lint + type-check
bun run lint 2>&1 | tail -5
bun run check 2>&1 | tail -5

# 4. Vitest with coverage gate
bun run test:coverage \
  --coverage.thresholds.lines=95 \
  --coverage.thresholds.branches=95 2>&1 | tail -20

# 5. Both E2E specs
bun run e2e tests/e2e/phase-1a/ 2>&1 | tail -20
```

Expected output shape:

```
# cargo test
test result: ok. ~80 passed; 0 failed

# clippy
Finished — no warnings

# bun run check
svelte-check: 0 errors, 0 warnings

# vitest coverage
✓ 34+ tests passed
Coverage: lines 95%+, branches 95%+

# playwright e2e
2 passed (add-repo.spec.ts, workspace-lifecycle.spec.ts)
```

- [ ] **Step 8.4: Commit the E2E spec**

```bash
cd /home/handokobeni/Work/ai-editor
git add tests/e2e/phase-1a/workspace-lifecycle.spec.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1a): E2E workspace lifecycle — create form → sidebar row → remove

Full Playwright flow: add repo (dialog shim), click New Workspace, fill
title + description, submit, assert row in sidebar, hover, click remove,
assert row gone. window.confirm mocked to skip native dialog.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 8.5: Tag `v0.2.0-phase1a`**

```bash
cd /home/handokobeni/Work/ai-editor
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" \
  tag -a v0.2.0-phase1a \
  -m "Phase 1a complete: typed IPC wrappers, ReposStore, WorkspacesStore, TitleBar, Sidebar, App grid, 2 E2E specs"
```

- [ ] **Step 8.6: Verify tag + recent commit log**

```bash
cd /home/handokobeni/Work/ai-editor
git log --oneline -30
```

Expected output shape (most-recent-first, newest at top):

```
<sha>  (HEAD -> main, tag: v0.2.0-phase1a) test(phase-1a): E2E workspace lifecycle — create form → sidebar row → remove
<sha>  test(phase-1a): E2E add-repo spec with dialog shim and fixture git repo
<sha>  feat(phase-1a): wire App.svelte to CSS grid layout with TitleBar + Sidebar
<sha>  feat(phase-1a): add Sidebar.svelte with workspace list and New Workspace form
<sha>  feat(phase-1a): add TitleBar.svelte with repo name display and Add Repo button
<sha>  feat(phase-1a): add WorkspacesStore with nested SvelteMap<repoId, SvelteMap<wsId>>
<sha>  feat(phase-1a): add ReposStore with SvelteMap and selectedRepoId rune
<sha>  feat(phase-1a): add Repo/Workspace types and api.repo/workspace IPC wrappers
<sha>  … (Phase 1a-backend commits)
<sha>  … (Phase 0 commits)
```

---

## Phase 1a shipping criteria (full — both backend and frontend)

- [ ] **Backend:** all commands registered (`add_repo`, `list_repos`,
      `remove_repo`, `update_repo_gh_profile`, `create_workspace`,
      `list_workspaces`, `remove_workspace`), `cargo test --lib` ~80 tests pass,
      coverage gate 95%, `cargo fmt --check` + `cargo clippy -- -D warnings`
      clean
- [ ] **Frontend:** `bun run lint` clean, `bun run check` 0 errors,
      `bun run test:coverage` all pass with 95% lines + branches gate
- [ ] **E2E:** both specs pass (`add-repo.spec.ts`,
      `workspace-lifecycle.spec.ts`) on Linux CI
- [ ] **Manual smoke:** `bun tauri dev` → click Add Repo → native dialog opens →
      select a git repo → TitleBar shows repo name → Sidebar shows "No
      workspaces yet" → click New Workspace → inline form appears → fill title +
      description → Submit → sidebar row appears with status dot → hover row →
      click × → row gone
- [ ] **Git tag `v0.2.0-phase1a`** on the final commit, pushed to origin

---

## Known deferrals to Phase 1b

- Plan/Work mode toggle (TitleBar keyboard shortcuts Ctrl+1/Ctrl+2) — Phase 1b
- Kanban board itself (Todo / In Progress / Review / Done drag-drop) — Phase 1b
- `Ctrl+Enter` / custom keyboard shortcuts — Phase 1b
- Breadcrumb navigation (korlap-style Plan→Work flow) — Phase 1b

## Known deferrals to Phase 1c

- Chat panel and message input — Phase 1c
- Agent spawn + `stream-json` NDJSON parser — Phase 1c
- Messages persistence (`messages/<wsId>.json`) — Phase 1c
- `SvelteMap<wsId, SvelteMap<msgId, Message>>` messages store — Phase 1c

---

## Self-review

1. **Placeholder scan** — no `TODO`, `...`, or stub bodies remain; all step
   blocks contain runnable code.

2. **Type consistency** — `Repo`, `Workspace`, `WorkspaceStatus`,
   `KanbanColumn`, `AppSettings`, `CreateWorkspaceArgs` defined once in
   `types.ts`, imported by name in all consumers. Field names (`repo_id`,
   `gh_profile`, `base_branch`, `custom_branch`, `created_at`, `updated_at`) use
   snake_case matching Rust serde throughout.

3. **Spec coverage** — every scope bullet has a task:
   - Types + IPC → Task 1
   - `ReposStore` → Task 2
   - `WorkspacesStore` → Task 3
   - `TitleBar` → Task 4
   - `Sidebar` → Task 5
   - `App` grid layout + on-mount → Task 6
   - E2E add-repo → Task 7
   - E2E workspace lifecycle + tag → Task 8

4. **TS strictness** — all `$state<T>()` calls have explicit type parameters
   (`$state<string | null>(null)`), `SvelteMap` generics explicit, no `any` in
   implementation code, `noUnusedLocals`/`noUnusedParameters` compliant.
