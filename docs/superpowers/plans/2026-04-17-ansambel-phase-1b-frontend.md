# Ansambel — Phase 1b Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Execute **after** Phase 1b-backend is
> merged.

**Goal:** Build the kanban UI, drag-drop, Plan/Work mode toggle, and baseline
keyboard shortcuts on top of the Phase 1b-backend task layer. Ships a working
kanban where dragging a task from Todo to In Progress auto-creates a git
worktree-backed workspace, tagging the task with the resulting branch name.

**Architecture:** Add a third runes-based store (`TasksStore`) alongside
`ReposStore` + `WorkspacesStore`. Render a 4-column kanban using
`svelte-dnd-action`. Introduce a simple `mode.svelte.ts` state that gates
between Plan mode (kanban) and Work mode (existing workspace placeholder; Phase
1c will put chat here). Add a `keyboard.ts` registry for cross-platform
shortcuts. Use Svelte 5 runes exclusively; public `$state` fields for reactive
store state (lessons from Phase 1a reactivity fixes).

**Tech Stack:** Svelte 5 runes, TypeScript strict, Vitest +
`@testing-library/svelte`, Playwright with existing `TauriDevHarness`, **new
dep: `svelte-dnd-action`** for drag-drop.

**Prerequisite:** Phase 1b-backend merged (task commands + persistence).

---

## Table of Contents

1. [Task 1](#task-1-add-task-types--expand-ipctstask-wrappers) — Add `Task` +
   `TaskPatch` + `CreateTaskArgs` types + expand `ipc.ts` with `api.task.*` (~10
   tests)
2. [Task 2](#task-2-taskssveltetss--tests) — `TasksStore` with nested
   `SvelteMap<repoId, SvelteMap<taskId, Task>>` (~10 tests)
3. [Task 3](#task-3-modesveltetss--tests) — `mode.svelte.ts` — `'plan' | 'work'`
   state + setter (3 tests)
4. [Task 4](#task-4-taskcardsveltes--tests) — `TaskCard.svelte` — title,
   description, branch badge, remove button (4 tests)
5. [Task 5](#task-5-kanbanboardsveltes--tests) — `KanbanBoard.svelte` — 4-column
   layout, drag targets, add-task trigger (5 tests)
6. [Task 6](#task-6-newtaskdialogsveltes--tests) — `NewTaskDialog.svelte` —
   modal with title/description inputs (4 tests)
7. [Task 7](#task-7-install-svelte-dnd-action--wire-drag-behavior) — Install
   `svelte-dnd-action`, wire `dndzone` handlers in `KanbanBoard.svelte` (4
   tests)
8. [Task 8](#task-8-planwork-toggle-in-titlebarsveltes--tests) — Plan/Work
   toggle buttons in `TitleBar.svelte` (3 tests)
9. [Task 9](#task-9-srclibkeyboardtss--tests) — `keyboard.ts` shortcut registry
   (5 tests)
10. [Task 10](#task-10-wire-5-baseline-shortcuts-in-appsveltes--tests) — Wire 5
    baseline shortcuts on mount in `App.svelte` (3 tests)
11. [Task 11](#task-11-appsvelte-plan--work-mode-integration--tests) —
    `App.svelte` conditional Kanban vs Work placeholder rendering (3 tests)
12. [Task 12](#task-12-e2e-testse2ephase-1bkanbanspects) — E2E: full kanban
    flow + `v0.3.0-phase1b` tag

---

### Task 1: Add `Task` types + expand `ipc.ts` task wrappers

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/ipc.test.ts`

- [ ] **Step 1.1: Write failing tests**

```typescript
// Append to src/lib/ipc.test.ts

import type { Task, CreateTaskArgs, TaskPatch } from './types';

const mockTask: Task = {
  id: 'tk_abc123',
  repo_id: 'repo_abc123',
  workspace_id: null,
  title: 'Fix login bug',
  description: 'Users cannot log in after password reset.',
  column: 'todo',
  order: 0,
  created_at: 1776000000,
  updated_at: 1776000000,
};

describe('api.task', () => {
  it('add: invokes add_task with args and returns Task', async () => {
    vi.mocked(invoke).mockResolvedValue(mockTask);
    const args: CreateTaskArgs = {
      repoId: 'repo_abc123',
      title: 'Fix login bug',
      description: 'Users cannot log in after password reset.',
    };
    const result = await api.task.add(args);
    expect(invoke).toHaveBeenCalledWith('add_task', args);
    expect(result).toEqual(mockTask);
  });

  it('add: forwards optional column', async () => {
    vi.mocked(invoke).mockResolvedValue(mockTask);
    const args: CreateTaskArgs = {
      repoId: 'repo_abc123',
      title: 'Fix login bug',
      description: '',
      column: 'in_progress',
    };
    await api.task.add(args);
    expect(invoke).toHaveBeenCalledWith('add_task', args);
  });

  it('add: propagates rejection from invoke', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Repo not found'));
    await expect(
      api.task.add({ repoId: 'repo_missing', title: 'T', description: '' })
    ).rejects.toThrow('Repo not found');
  });

  it('list: invokes list_tasks with repoId and returns Task[]', async () => {
    vi.mocked(invoke).mockResolvedValue([mockTask]);
    const result = await api.task.list('repo_abc123');
    expect(invoke).toHaveBeenCalledWith('list_tasks', {
      repoId: 'repo_abc123',
    });
    expect(result).toEqual([mockTask]);
  });

  it('list: returns empty array when no tasks', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const result = await api.task.list('repo_abc123');
    expect(result).toEqual([]);
  });

  it('update: invokes update_task with taskId and patch', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const patch: TaskPatch = { title: 'Updated title' };
    await api.task.update('tk_abc123', patch);
    expect(invoke).toHaveBeenCalledWith('update_task', {
      taskId: 'tk_abc123',
      patch,
    });
  });

  it('move: invokes move_task with taskId, column, order', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.task.move('tk_abc123', 'in_progress', 1);
    expect(invoke).toHaveBeenCalledWith('move_task', {
      taskId: 'tk_abc123',
      column: 'in_progress',
      order: 1,
    });
  });

  it('move: propagates rejection (e.g. task not found)', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('Task not found'));
    await expect(api.task.move('tk_missing', 'in_progress', 0)).rejects.toThrow(
      'Task not found'
    );
  });

  it('remove: invokes remove_task with taskId', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.task.remove('tk_abc123');
    expect(invoke).toHaveBeenCalledWith('remove_task', {
      taskId: 'tk_abc123',
    });
  });

  it('remove: forwards force flag', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.task.remove('tk_abc123', true);
    expect(invoke).toHaveBeenCalledWith('remove_task', {
      taskId: 'tk_abc123',
      force: true,
    });
  });
});
```

- [ ] **Step 1.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/ipc.test.ts 2>&1 | tail -10
```

Expected: type errors — `Task`, `CreateTaskArgs`, `TaskPatch` not found;
`api.task` not defined.

- [ ] **Step 1.3: Implement**

Append to `src/lib/types.ts`:

```typescript
// --- Task types (Phase 1b) ---

export type Task = {
  id: string; // tk_xxxxxx
  repo_id: string;
  workspace_id: string | null;
  title: string;
  description: string;
  column: KanbanColumn;
  order: number;
  created_at: number;
  updated_at: number;
};

export type CreateTaskArgs = {
  repoId: string;
  title: string;
  description: string;
  column?: KanbanColumn;
};

export type TaskPatch = {
  title?: string;
  description?: string;
  order?: number;
};

export type Mode = 'plan' | 'work';
```

Append to the `api` object in `src/lib/ipc.ts`:

```typescript
  task: {
    add: (args: CreateTaskArgs): Promise<Task> =>
      invoke('add_task', args),

    list: (repoId: string): Promise<Task[]> =>
      invoke('list_tasks', { repoId }),

    update: (taskId: string, patch: TaskPatch): Promise<void> =>
      invoke('update_task', { taskId, patch }),

    move: (taskId: string, column: KanbanColumn, order: number): Promise<void> =>
      invoke('move_task', { taskId, column, order }),

    remove: (taskId: string, force?: boolean): Promise<void> =>
      invoke('remove_task', { taskId, force }),
  },
```

Add the required imports to the top of `src/lib/ipc.ts`:

```typescript
import type {
  Repo,
  Workspace,
  CreateWorkspaceArgs,
  Task,
  CreateTaskArgs,
  TaskPatch,
  KanbanColumn,
} from './types';
```

- [ ] **Step 1.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/ipc.test.ts 2>&1 | tail -10
```

Expected: all `api.task.*` tests pass alongside existing `api.repo.*` /
`api.workspace.*` tests.

- [ ] **Step 1.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/types.ts src/lib/ipc.ts src/lib/ipc.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add Task/TaskPatch/CreateTaskArgs types and api.task IPC wrappers

Extend types.ts with Task, CreateTaskArgs, TaskPatch, Mode. Expand ipc.ts
with api.task.{add, list, update, move, remove}. 10 Vitest tests verify
every invoke signature including force-remove and column forwarding.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `tasks.svelte.ts` + tests

**Files:**

- Create: `src/lib/stores/tasks.svelte.ts`
- Create: `src/lib/stores/tasks.svelte.test.ts`

- [ ] **Step 2.1: Write failing tests**

```typescript
// src/lib/stores/tasks.svelte.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    task: {
      add: vi.fn(),
      list: vi.fn(),
      update: vi.fn(),
      move: vi.fn(),
      remove: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { TasksStore } from './tasks.svelte';
import type { Task } from '$lib/types';

const makeTask = (overrides: Partial<Task> = {}): Task => ({
  id: 'tk_abc123',
  repo_id: 'repo_abc123',
  workspace_id: null,
  title: 'Fix login bug',
  description: 'Users cannot log in.',
  column: 'todo',
  order: 0,
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('TasksStore', () => {
  it('loadForRepo: populates nested map from api.task.list', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    expect(store.tasks.get('repo_abc123')?.get('tk_abc123')).toEqual(task);
  });

  it('loadForRepo: inner map is empty when backend returns []', async () => {
    vi.mocked(api.task.list).mockResolvedValue([]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    expect(store.tasks.get('repo_abc123')?.size).toBe(0);
  });

  it('add: calls api.task.add and inserts returned Task into nested map', async () => {
    const task = makeTask();
    vi.mocked(api.task.add).mockResolvedValue(task);
    const store = new TasksStore();
    const args = {
      repoId: 'repo_abc123',
      title: 'Fix login bug',
      description: '',
    };
    const result = await store.add(args);
    expect(api.task.add).toHaveBeenCalledWith(args);
    expect(result).toEqual(task);
    expect(store.tasks.get('repo_abc123')?.get('tk_abc123')).toEqual(task);
  });

  it('update: calls api.task.update and mutates existing entry in map', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.update).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.update('tk_abc123', { title: 'Updated title' });
    expect(api.task.update).toHaveBeenCalledWith('tk_abc123', {
      title: 'Updated title',
    });
    expect(store.tasks.get('repo_abc123')?.get('tk_abc123')?.title).toBe(
      'Updated title'
    );
  });

  it('move: calls api.task.move and updates column + order in map', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.move).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.move('tk_abc123', 'in_progress', 2);
    expect(api.task.move).toHaveBeenCalledWith('tk_abc123', 'in_progress', 2);
    const updated = store.tasks.get('repo_abc123')?.get('tk_abc123');
    expect(updated?.column).toBe('in_progress');
    expect(updated?.order).toBe(2);
  });

  it('remove: calls api.task.remove and deletes from nested map', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.remove).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.remove('tk_abc123');
    expect(api.task.remove).toHaveBeenCalledWith('tk_abc123', undefined);
    expect(store.tasks.get('repo_abc123')?.has('tk_abc123')).toBe(false);
  });

  it('remove: forwards force=true to api', async () => {
    const task = makeTask();
    vi.mocked(api.task.list).mockResolvedValue([task]);
    vi.mocked(api.task.remove).mockResolvedValue(undefined);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    await store.remove('tk_abc123', true);
    expect(api.task.remove).toHaveBeenCalledWith('tk_abc123', true);
  });

  it('listForRepo: returns all tasks for a repo as array', async () => {
    const t1 = makeTask({ id: 'tk_aaa111', order: 0 });
    const t2 = makeTask({ id: 'tk_bbb222', order: 1 });
    vi.mocked(api.task.list).mockResolvedValue([t1, t2]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    const list = store.listForRepo('repo_abc123');
    expect(list).toHaveLength(2);
  });

  it('listForColumn: returns only tasks matching the given column, sorted by order', async () => {
    const todo1 = makeTask({ id: 'tk_aaa111', column: 'todo', order: 1 });
    const todo0 = makeTask({ id: 'tk_bbb222', column: 'todo', order: 0 });
    const inProg = makeTask({
      id: 'tk_ccc333',
      column: 'in_progress',
      order: 0,
    });
    vi.mocked(api.task.list).mockResolvedValue([todo1, todo0, inProg]);
    const store = new TasksStore();
    await store.loadForRepo('repo_abc123');
    const todos = store.listForColumn('repo_abc123', 'todo');
    expect(todos).toHaveLength(2);
    expect(todos[0].id).toBe('tk_bbb222'); // order 0 first
    expect(todos[1].id).toBe('tk_aaa111'); // order 1 second
  });

  it('selectedTaskId: starts null, can be set', () => {
    const store = new TasksStore();
    expect(store.selectedTaskId).toBeNull();
    store.selectTask('tk_abc123');
    expect(store.selectedTaskId).toBe('tk_abc123');
  });

  it('selectTask: accepts null to deselect', () => {
    const store = new TasksStore();
    store.selectTask('tk_abc123');
    store.selectTask(null);
    expect(store.selectedTaskId).toBeNull();
  });
});
```

- [ ] **Step 2.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/tasks.svelte.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './tasks.svelte'`.

- [ ] **Step 2.3: Implement**

```typescript
// src/lib/stores/tasks.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import type { Task, CreateTaskArgs, TaskPatch, KanbanColumn } from '$lib/types';

export class TasksStore {
  readonly tasks = new SvelteMap<string, SvelteMap<string, Task>>();
  selectedTaskId = $state<string | null>(null);

  private getOrCreate(repoId: string): SvelteMap<string, Task> {
    let map = this.tasks.get(repoId);
    if (!map) {
      map = new SvelteMap<string, Task>();
      this.tasks.set(repoId, map);
    }
    return map;
  }

  async loadForRepo(repoId: string): Promise<void> {
    const list = await api.task.list(repoId);
    const map = this.getOrCreate(repoId);
    map.clear();
    for (const task of list) {
      map.set(task.id, task);
    }
  }

  async add(args: CreateTaskArgs): Promise<Task> {
    const task = await api.task.add(args);
    this.getOrCreate(task.repo_id).set(task.id, task);
    return task;
  }

  async update(taskId: string, patch: TaskPatch): Promise<void> {
    await api.task.update(taskId, patch);
    for (const [, map] of this.tasks) {
      const existing = map.get(taskId);
      if (existing) {
        map.set(taskId, { ...existing, ...patch });
        return;
      }
    }
  }

  async move(
    taskId: string,
    column: KanbanColumn,
    order: number
  ): Promise<void> {
    await api.task.move(taskId, column, order);
    for (const [, map] of this.tasks) {
      const existing = map.get(taskId);
      if (existing) {
        map.set(taskId, { ...existing, column, order });
        return;
      }
    }
  }

  async remove(taskId: string, force?: boolean): Promise<void> {
    await api.task.remove(taskId, force);
    for (const [, map] of this.tasks) {
      if (map.has(taskId)) {
        map.delete(taskId);
        if (this.selectedTaskId === taskId) {
          this.selectedTaskId = null;
        }
        return;
      }
    }
  }

  listForRepo(repoId: string): Task[] {
    const map = this.tasks.get(repoId);
    if (!map) return [];
    return Array.from(map.values());
  }

  listForColumn(repoId: string, column: KanbanColumn): Task[] {
    return this.listForRepo(repoId)
      .filter((t) => t.column === column)
      .sort((a, b) => a.order - b.order);
  }

  selectTask(id: string | null): void {
    this.selectedTaskId = id;
  }
}

export const tasks = new TasksStore();
```

- [ ] **Step 2.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/tasks.svelte.test.ts 2>&1 | tail -10
```

Expected: `10 tests passed`.

- [ ] **Step 2.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/stores/tasks.svelte.ts src/lib/stores/tasks.svelte.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add TasksStore with nested SvelteMap and listForColumn

Nested SvelteMap<repoId, SvelteMap<taskId, Task>> for reactive kanban
state. Public $state fields (selectedTaskId) avoid Phase 1a private-field
reactivity issue. 10 Vitest tests cover CRUD, move, listForRepo,
listForColumn (sorted by order), and selectTask.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `mode.svelte.ts` + tests

**Files:**

- Create: `src/lib/stores/mode.svelte.ts`
- Create: `src/lib/stores/mode.svelte.test.ts`

- [ ] **Step 3.1: Write failing tests**

```typescript
// src/lib/stores/mode.svelte.test.ts
import { describe, it, expect } from 'vitest';
import { ModeStore } from './mode.svelte';

describe('ModeStore', () => {
  it('starts in plan mode', () => {
    const store = new ModeStore();
    expect(store.mode).toBe('plan');
  });

  it('set: switches to work mode', () => {
    const store = new ModeStore();
    store.set('work');
    expect(store.mode).toBe('work');
  });

  it('set: can switch back to plan mode', () => {
    const store = new ModeStore();
    store.set('work');
    store.set('plan');
    expect(store.mode).toBe('plan');
  });
});
```

- [ ] **Step 3.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/mode.svelte.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './mode.svelte'`.

- [ ] **Step 3.3: Implement**

```typescript
// src/lib/stores/mode.svelte.ts
import type { Mode } from '$lib/types';

export class ModeStore {
  mode = $state<Mode>('plan');

  set(next: Mode): void {
    this.mode = next;
  }
}

export const modeStore = new ModeStore();
```

- [ ] **Step 3.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/mode.svelte.test.ts 2>&1 | tail -10
```

Expected: `3 tests passed`.

- [ ] **Step 3.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/stores/mode.svelte.ts src/lib/stores/mode.svelte.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add ModeStore for plan/work mode toggle

Minimal $state-backed store holding 'plan' | 'work' with a set() method.
In-memory only — no persistence needed at this phase. 3 Vitest tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `TaskCard.svelte` + tests

**Files:**

- Create: `src/lib/components/kanban/TaskCard.svelte`
- Create: `src/lib/components/kanban/TaskCard.test.ts`

- [ ] **Step 4.1: Write failing tests**

```typescript
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
```

- [ ] **Step 4.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/TaskCard.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './TaskCard.svelte'`.

- [ ] **Step 4.3: Implement**

```svelte
<!-- src/lib/components/kanban/TaskCard.svelte -->
<script lang="ts">
  import type { Task } from '$lib/types';

  const { task, onRemove }: { task: Task; onRemove: (id: string) => void } =
    $props();

  const truncatedDescription = $derived(
    task.description.length > 80
      ? task.description.slice(0, 80) + '...'
      : task.description
  );

  const relativeDate = $derived(() => {
    const diff = Date.now() / 1000 - task.created_at;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  });
</script>

<div class="task-card" role="listitem" aria-label={task.title}>
  <div class="task-card__header">
    <span class="task-card__title">{task.title}</span>
    <button
      class="task-card__remove"
      aria-label="Remove task"
      onclick={() => onRemove(task.id)}
    >
      ×
    </button>
  </div>

  {#if task.description}
    <p class="task-card__description" data-testid="task-description">
      {truncatedDescription}
    </p>
  {/if}

  <div class="task-card__footer">
    {#if task.workspace_id}
      <span class="task-card__branch-badge" data-testid="branch-badge">
        branch
      </span>
    {/if}
    <span class="task-card__date">{relativeDate()}</span>
  </div>
</div>

<style>
  .task-card {
    background: var(--color-surface-1);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 10px 12px;
    cursor: grab;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .task-card:active {
    cursor: grabbing;
  }

  .task-card__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }

  .task-card__title {
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--color-text-primary);
    line-height: 1.3;
  }

  .task-card__remove {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
    padding: 0 2px;
    flex-shrink: 0;
  }

  .task-card__remove:hover {
    color: var(--color-text-primary);
  }

  .task-card__description {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    margin: 0;
    line-height: 1.4;
  }

  .task-card__footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .task-card__branch-badge {
    font-size: 0.7rem;
    background: var(--color-accent-subtle);
    color: var(--color-accent);
    border-radius: 4px;
    padding: 1px 6px;
    font-family: var(--font-mono, monospace);
  }

  .task-card__date {
    font-size: 0.7rem;
    color: var(--color-text-muted);
    margin-left: auto;
  }
</style>
```

- [ ] **Step 4.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/TaskCard.test.ts 2>&1 | tail -10
```

Expected: `5 tests passed` (4 described + 1 implicit from render).

- [ ] **Step 4.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/kanban/TaskCard.svelte src/lib/components/kanban/TaskCard.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add TaskCard component with truncation and branch badge

Renders task title, description (80-char truncation), branch badge when
workspace_id is set, relative date, and remove button. Svelte 5 runes
only. 5 Vitest/testing-library tests including edge cases.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: `KanbanBoard.svelte` + tests

**Files:**

- Create: `src/lib/components/kanban/KanbanBoard.svelte`
- Create: `src/lib/components/kanban/KanbanBoard.test.ts`

- [ ] **Step 5.1: Write failing tests**

```typescript
// src/lib/components/kanban/KanbanBoard.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
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
    const { container } = render(KanbanBoard, {
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
```

- [ ] **Step 5.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/KanbanBoard.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './KanbanBoard.svelte'`.

- [ ] **Step 5.3: Implement**

```svelte
<!-- src/lib/components/kanban/KanbanBoard.svelte -->
<script lang="ts">
  import type { Task, KanbanColumn } from '$lib/types';
  import TaskCard from './TaskCard.svelte';

  const {
    repoId,
    tasks,
    onMove,
    onAddTask,
    onRemoveTask,
  }: {
    repoId: string;
    tasks: Task[];
    onMove: (taskId: string, column: KanbanColumn, order: number) => void;
    onAddTask: () => void;
    onRemoveTask: (taskId: string) => void;
  } = $props();

  type Column = {
    id: KanbanColumn;
    label: string;
  };

  const COLUMNS: Column[] = [
    { id: 'todo', label: 'Todo' },
    { id: 'in_progress', label: 'In Progress' },
    { id: 'review', label: 'Review' },
    { id: 'done', label: 'Done' },
  ];

  function tasksForColumn(column: KanbanColumn): Task[] {
    return tasks
      .filter((t) => t.column === column)
      .sort((a, b) => a.order - b.order);
  }
</script>

<div class="kanban-board">
  {#each COLUMNS as col (col.id)}
    <div class="kanban-column" data-column={col.id}>
      <div class="kanban-column__header">
        <span class="kanban-column__title">{col.label}</span>
        <span class="kanban-column__count">{tasksForColumn(col.id).length}</span
        >
      </div>

      <div
        class="kanban-column__body"
        role="list"
        aria-label="{col.label} tasks"
      >
        {#each tasksForColumn(col.id) as task (task.id)}
          <TaskCard {task} onRemove={onRemoveTask} />
        {:else}
          <p class="kanban-column__empty">No tasks</p>
        {/each}
      </div>

      {#if col.id === 'todo'}
        <button class="kanban-column__add-btn" onclick={onAddTask}>
          + Add task
        </button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .kanban-board {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    height: 100%;
    overflow-x: auto;
    padding: 16px;
  }

  .kanban-column {
    background: var(--color-surface-0);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    min-height: 200px;
    overflow: hidden;
  }

  .kanban-column__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px 8px;
    border-bottom: 1px solid var(--color-border);
  }

  .kanban-column__title {
    font-size: 0.8125rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
  }

  .kanban-column__count {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    background: var(--color-surface-1);
    border-radius: 10px;
    padding: 1px 7px;
  }

  .kanban-column__body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    overflow-y: auto;
    min-height: 60px;
  }

  .kanban-column__empty {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    text-align: center;
    margin: 12px 0;
  }

  .kanban-column__add-btn {
    width: 100%;
    background: none;
    border: none;
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.8125rem;
    padding: 10px 14px;
    text-align: left;
    transition:
      color 0.15s,
      background 0.15s;
  }

  .kanban-column__add-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-surface-1);
  }
</style>
```

- [ ] **Step 5.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/KanbanBoard.test.ts 2>&1 | tail -10
```

Expected: `5 tests passed`.

- [ ] **Step 5.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/kanban/KanbanBoard.svelte src/lib/components/kanban/KanbanBoard.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add KanbanBoard with 4-column layout and task rendering

4-column kanban (Todo/In Progress/Review/Done) rendering TaskCards sorted
by order, empty-state message, and Add task button in Todo column. Drag
wiring deferred to Task 7. 5 Vitest tests.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: `NewTaskDialog.svelte` + tests

**Files:**

- Create: `src/lib/components/kanban/NewTaskDialog.svelte`
- Create: `src/lib/components/kanban/NewTaskDialog.test.ts`

- [ ] **Step 6.1: Write failing tests**

```typescript
// src/lib/components/kanban/NewTaskDialog.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import NewTaskDialog from './NewTaskDialog.svelte';

describe('NewTaskDialog', () => {
  it('renders title input and description textarea', () => {
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel: vi.fn() },
    });
    expect(screen.getByLabelText(/title/i)).toBeTruthy();
    expect(screen.getByLabelText(/description/i)).toBeTruthy();
  });

  it('submit button is disabled when title is empty', () => {
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel: vi.fn() },
    });
    const submitBtn = screen.getByRole('button', { name: /add task/i });
    expect((submitBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('calls onSubmit with title and description when form submitted', async () => {
    const onSubmit = vi.fn();
    render(NewTaskDialog, {
      props: { open: true, onSubmit, onCancel: vi.fn() },
    });
    const titleInput = screen.getByLabelText(/title/i);
    const descInput = screen.getByLabelText(/description/i);
    await fireEvent.input(titleInput, { target: { value: 'New feature' } });
    await fireEvent.input(descInput, {
      target: { value: 'Add the new feature' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add task/i }));
    expect(onSubmit).toHaveBeenCalledWith({
      title: 'New feature',
      description: 'Add the new feature',
    });
  });

  it('calls onCancel when Cancel button clicked', async () => {
    const onCancel = vi.fn();
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel },
    });
    await fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 6.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/NewTaskDialog.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './NewTaskDialog.svelte'`.

- [ ] **Step 6.3: Implement**

```svelte
<!-- src/lib/components/kanban/NewTaskDialog.svelte -->
<script lang="ts">
  const {
    open,
    onSubmit,
    onCancel,
  }: {
    open: boolean;
    onSubmit: (data: { title: string; description: string }) => void;
    onCancel: () => void;
  } = $props();

  let title = $state('');
  let description = $state('');

  const canSubmit = $derived(title.trim().length > 0);

  function handleSubmit(e: Event) {
    e.preventDefault();
    if (!canSubmit) return;
    onSubmit({ title: title.trim(), description: description.trim() });
    title = '';
    description = '';
  }

  function handleCancel() {
    title = '';
    description = '';
    onCancel();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div class="dialog-backdrop" onclick={handleCancel}>
    <dialog class="new-task-dialog" open onclick={(e) => e.stopPropagation()}>
      <h2 class="new-task-dialog__title">New Task</h2>

      <form onsubmit={handleSubmit}>
        <div class="form-field">
          <label for="task-title">Title</label>
          <input
            id="task-title"
            type="text"
            bind:value={title}
            placeholder="Task title"
            autofocus
            required
          />
        </div>

        <div class="form-field">
          <label for="task-description">Description</label>
          <textarea
            id="task-description"
            bind:value={description}
            placeholder="Optional description"
            rows="3"
          ></textarea>
        </div>

        <div class="new-task-dialog__actions">
          <button type="button" class="btn-secondary" onclick={handleCancel}>
            Cancel
          </button>
          <button type="submit" class="btn-primary" disabled={!canSubmit}>
            Add task
          </button>
        </div>
      </form>
    </dialog>
  </div>
{/if}

<style>
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .new-task-dialog {
    background: var(--color-surface-1);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 24px;
    width: 420px;
    max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .new-task-dialog__title {
    font-size: 1rem;
    font-weight: 600;
    margin: 0 0 20px;
    color: var(--color-text-primary);
  }

  .form-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .form-field label {
    font-size: 0.8125rem;
    font-weight: 500;
    color: var(--color-text-muted);
  }

  .form-field input,
  .form-field textarea {
    background: var(--color-surface-0);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-primary);
    font-size: 0.875rem;
    padding: 8px 10px;
    resize: vertical;
  }

  .form-field input:focus,
  .form-field textarea:focus {
    outline: none;
    border-color: var(--color-accent);
  }

  .new-task-dialog__actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 20px;
  }

  .btn-secondary {
    background: var(--color-surface-0);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-primary);
    cursor: pointer;
    font-size: 0.875rem;
    padding: 7px 16px;
  }

  .btn-primary {
    background: var(--color-accent);
    border: none;
    border-radius: 6px;
    color: #fff;
    cursor: pointer;
    font-size: 0.875rem;
    font-weight: 500;
    padding: 7px 16px;
  }

  .btn-primary:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
```

- [ ] **Step 6.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/NewTaskDialog.test.ts 2>&1 | tail -10
```

Expected: `4 tests passed`.

- [ ] **Step 6.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/kanban/NewTaskDialog.svelte src/lib/components/kanban/NewTaskDialog.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add NewTaskDialog modal with title/description form

Modal dialog with title input (required, submit gated), description
textarea, submit + cancel. Resets fields on dismiss. 4 Vitest tests
including disabled-state and onSubmit payload shape.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Install `svelte-dnd-action` + wire drag behavior

**Files:**

- Modify: `package.json` (via `bun add`)
- Modify: `src/lib/components/kanban/KanbanBoard.svelte`
- Modify: `src/lib/components/kanban/KanbanBoard.test.ts`

- [ ] **Step 7.1: Write failing tests (drag behavior)**

Append to `src/lib/components/kanban/KanbanBoard.test.ts`:

```typescript
// Append to KanbanBoard.test.ts — drag behavior tests
import { SHADOW_ITEM_MARKER_PROPERTY_NAME } from 'svelte-dnd-action';

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
    const inProgressZone = document.querySelector(
      '[data-column="in_progress"]'
    ) as HTMLElement;
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
    const todoZone = document.querySelector(
      '[data-column="todo"]'
    ) as HTMLElement;
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
    const doneZone = document.querySelector(
      '[data-column="done"]'
    ) as HTMLElement;
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
});
```

- [ ] **Step 7.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/KanbanBoard.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module 'svelte-dnd-action'` (package not yet installed).

- [ ] **Step 7.3: Implement**

Install the package:

```bash
cd /home/handokobeni/Work/ai-editor
bun add svelte-dnd-action
```

Update `src/lib/components/kanban/KanbanBoard.svelte` to wire drag-drop:

```svelte
<!-- src/lib/components/kanban/KanbanBoard.svelte -->
<script lang="ts">
  import { dndzone, SHADOW_ITEM_MARKER_PROPERTY_NAME } from 'svelte-dnd-action';
  import type { Task, KanbanColumn } from '$lib/types';
  import TaskCard from './TaskCard.svelte';

  const {
    repoId,
    tasks,
    onMove,
    onAddTask,
    onRemoveTask,
  }: {
    repoId: string;
    tasks: Task[];
    onMove: (taskId: string, column: KanbanColumn, order: number) => void;
    onAddTask: () => void;
    onRemoveTask: (taskId: string) => void;
  } = $props();

  type Column = {
    id: KanbanColumn;
    label: string;
  };

  const COLUMNS: Column[] = [
    { id: 'todo', label: 'Todo' },
    { id: 'in_progress', label: 'In Progress' },
    { id: 'review', label: 'Review' },
    { id: 'done', label: 'Done' },
  ];

  // Local mutable copies per column for dnd-action to manipulate during drag
  let columnItems = $state<Record<KanbanColumn, Task[]>>({
    todo: [],
    in_progress: [],
    review: [],
    done: [],
  });

  // Sync columnItems whenever parent tasks prop changes
  $effect(() => {
    const next: Record<KanbanColumn, Task[]> = {
      todo: [],
      in_progress: [],
      review: [],
      done: [],
    };
    for (const t of tasks) {
      next[t.column].push(t);
    }
    for (const col of COLUMNS) {
      next[col.id].sort((a, b) => a.order - b.order);
    }
    columnItems = next;
  });

  function handleConsider(
    column: KanbanColumn,
    e: CustomEvent<{ items: Task[] }>
  ) {
    columnItems = { ...columnItems, [column]: e.detail.items };
  }

  function handleFinalize(
    column: KanbanColumn,
    e: CustomEvent<{ items: Task[]; info: { id: string } }>
  ) {
    const items = e.detail.items.filter(
      (t) =>
        !(t as Task & Record<string, unknown>)[SHADOW_ITEM_MARKER_PROPERTY_NAME]
    );
    columnItems = { ...columnItems, [column]: items };
    const droppedId = e.detail.info.id;
    const newOrder = items.findIndex((t) => t.id === droppedId);
    if (newOrder !== -1) {
      onMove(droppedId, column, newOrder);
    }
  }
</script>

<div class="kanban-board">
  {#each COLUMNS as col (col.id)}
    <div class="kanban-column" data-column={col.id}>
      <div class="kanban-column__header">
        <span class="kanban-column__title">{col.label}</span>
        <span class="kanban-column__count">{columnItems[col.id].length}</span>
      </div>

      <div
        class="kanban-column__body"
        role="list"
        aria-label="{col.label} tasks"
        use:dndzone={{ items: columnItems[col.id], flipDurationMs: 150 }}
        onconsider={(e) => handleConsider(col.id, e)}
        onfinalize={(e) => handleFinalize(col.id, e)}
      >
        {#each columnItems[col.id] as task (task.id)}
          <TaskCard {task} onRemove={onRemoveTask} />
        {:else}
          <p class="kanban-column__empty">No tasks</p>
        {/each}
      </div>

      {#if col.id === 'todo'}
        <button class="kanban-column__add-btn" onclick={onAddTask}>
          + Add task
        </button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .kanban-board {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    height: 100%;
    overflow-x: auto;
    padding: 16px;
  }

  .kanban-column {
    background: var(--color-surface-0);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    min-height: 200px;
    overflow: hidden;
  }

  .kanban-column__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px 8px;
    border-bottom: 1px solid var(--color-border);
  }

  .kanban-column__title {
    font-size: 0.8125rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
  }

  .kanban-column__count {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    background: var(--color-surface-1);
    border-radius: 10px;
    padding: 1px 7px;
  }

  .kanban-column__body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    overflow-y: auto;
    min-height: 60px;
  }

  .kanban-column__empty {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    text-align: center;
    margin: 12px 0;
  }

  .kanban-column__add-btn {
    width: 100%;
    background: none;
    border: none;
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.8125rem;
    padding: 10px 14px;
    text-align: left;
    transition:
      color 0.15s,
      background 0.15s;
  }

  .kanban-column__add-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-surface-1);
  }
</style>
```

- [ ] **Step 7.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/kanban/KanbanBoard.test.ts 2>&1 | tail -10
```

Expected: all 9 tests pass (5 from Task 5 + 4 drag behavior).

- [ ] **Step 7.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add package.json bun.lockb src/lib/components/kanban/KanbanBoard.svelte src/lib/components/kanban/KanbanBoard.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): wire svelte-dnd-action drag-drop in KanbanBoard

Install svelte-dnd-action; wire dndzone with consider/finalize handlers.
finalize filters shadow items and calls onMove(taskId, newColumn, newOrder).
consider updates local column state for smooth animation without persisting.
4 new tests covering dnd zone render, finalize->onMove, consider no-op, and
shadow-item filtering.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Plan/Work toggle in `TitleBar.svelte` + tests

**Files:**

- Create: `src/lib/components/TitleBar.svelte`
- Create: `src/lib/components/TitleBar.test.ts`

- [ ] **Step 8.1: Write failing tests**

```typescript
// src/lib/components/TitleBar.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import TitleBar from './TitleBar.svelte';

describe('TitleBar', () => {
  it('renders Plan and Work buttons', () => {
    render(TitleBar, {
      props: { mode: 'plan', onModeChange: vi.fn() },
    });
    expect(screen.getByRole('button', { name: /^plan$/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /^work$/i })).toBeTruthy();
  });

  it('Plan button has active class when mode is plan', () => {
    render(TitleBar, {
      props: { mode: 'plan', onModeChange: vi.fn() },
    });
    const planBtn = screen.getByRole('button', { name: /^plan$/i });
    expect(planBtn.classList.contains('active')).toBe(true);
  });

  it('clicking Work button calls onModeChange with work', async () => {
    const onModeChange = vi.fn();
    render(TitleBar, {
      props: { mode: 'plan', onModeChange },
    });
    await fireEvent.click(screen.getByRole('button', { name: /^work$/i }));
    expect(onModeChange).toHaveBeenCalledWith('work');
  });
});
```

- [ ] **Step 8.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/TitleBar.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './TitleBar.svelte'`.

- [ ] **Step 8.3: Implement**

```svelte
<!-- src/lib/components/TitleBar.svelte -->
<script lang="ts">
  import type { Mode } from '$lib/types';

  const {
    mode,
    onModeChange,
  }: {
    mode: Mode;
    onModeChange: (next: Mode) => void;
  } = $props();
</script>

<header class="titlebar">
  <div class="titlebar__drag-region" data-tauri-drag-region></div>

  <div class="titlebar__mode-toggle" role="group" aria-label="View mode">
    <button
      class="mode-btn"
      class:active={mode === 'plan'}
      onclick={() => onModeChange('plan')}
      aria-pressed={mode === 'plan'}
    >
      Plan
    </button>
    <button
      class="mode-btn"
      class:active={mode === 'work'}
      onclick={() => onModeChange('work')}
      aria-pressed={mode === 'work'}
    >
      Work
    </button>
  </div>

  <div class="titlebar__spacer"></div>
</header>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    height: 38px;
    background: var(--color-titlebar-bg, var(--color-surface-0));
    border-bottom: 1px solid var(--color-border);
    padding: 0 12px;
    gap: 8px;
    position: relative;
    z-index: 10;
  }

  .titlebar__drag-region {
    flex: 1;
    height: 100%;
    position: absolute;
    inset: 0;
  }

  .titlebar__mode-toggle {
    display: flex;
    background: var(--color-surface-1);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    overflow: hidden;
    position: relative;
    z-index: 1;
  }

  .mode-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.8125rem;
    font-weight: 500;
    padding: 4px 14px;
    transition:
      background 0.15s,
      color 0.15s;
  }

  .mode-btn.active {
    background: var(--color-accent);
    color: #fff;
  }

  .mode-btn:not(.active):hover {
    background: var(--color-surface-2);
    color: var(--color-text-primary);
  }

  .titlebar__spacer {
    position: relative;
    z-index: 1;
    flex: 1;
  }
</style>
```

- [ ] **Step 8.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/TitleBar.test.ts 2>&1 | tail -10
```

Expected: `3 tests passed`.

- [ ] **Step 8.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/TitleBar.svelte src/lib/components/TitleBar.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add TitleBar with Plan/Work mode toggle buttons

Two-button toggle group emitting onModeChange; active button highlighted
via .active class. Includes Tauri drag region overlay. 3 Vitest tests
covering render, active state, and click callback.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: `src/lib/keyboard.ts` + tests

**Files:**

- Create: `src/lib/keyboard.ts`
- Create: `src/lib/keyboard.test.ts`

- [ ] **Step 9.1: Write failing tests**

```typescript
// src/lib/keyboard.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ShortcutRegistry } from './keyboard';

describe('ShortcutRegistry', () => {
  let registry: ShortcutRegistry;
  let listeners: Array<(e: KeyboardEvent) => void> = [];

  beforeEach(() => {
    listeners = [];
    vi.spyOn(window, 'addEventListener').mockImplementation(
      (type: string, listener: EventListenerOrEventListenerObject) => {
        if (type === 'keydown') {
          listeners.push(listener as (e: KeyboardEvent) => void);
        }
      }
    );
    vi.spyOn(window, 'removeEventListener').mockImplementation(() => {});
    registry = new ShortcutRegistry();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function fire(
    key: string,
    opts: { ctrlKey?: boolean; metaKey?: boolean } = {}
  ) {
    const event = new KeyboardEvent('keydown', {
      key,
      ctrlKey: opts.ctrlKey ?? false,
      metaKey: opts.metaKey ?? false,
      bubbles: true,
    });
    listeners.forEach((l) => l(event));
  }

  it('registers a handler that fires on Ctrl+key', () => {
    const handler = vi.fn();
    registry.register('ctrl+1', handler);
    fire('1', { ctrlKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('registers a handler that fires on Meta+key (macOS Cmd)', () => {
    const handler = vi.fn();
    registry.register('ctrl+1', handler);
    fire('1', { metaKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does not fire when modifier is absent', () => {
    const handler = vi.fn();
    registry.register('ctrl+n', handler);
    fire('n');
    expect(handler).not.toHaveBeenCalled();
  });

  it('unregister stops the handler from firing', () => {
    const handler = vi.fn();
    const unregister = registry.register('ctrl+,', handler);
    unregister();
    fire(',', { ctrlKey: true });
    expect(handler).not.toHaveBeenCalled();
  });

  it('multiple shortcuts coexist independently', () => {
    const h1 = vi.fn();
    const h2 = vi.fn();
    registry.register('ctrl+1', h1);
    registry.register('ctrl+2', h2);
    fire('1', { ctrlKey: true });
    expect(h1).toHaveBeenCalledTimes(1);
    expect(h2).not.toHaveBeenCalled();
    fire('2', { ctrlKey: true });
    expect(h2).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 9.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/keyboard.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './keyboard'`.

- [ ] **Step 9.3: Implement**

```typescript
// src/lib/keyboard.ts

/**
 * Cross-platform keyboard shortcut registry.
 *
 * Combo format: "ctrl+<key>" — matches both Ctrl (Windows/Linux) and
 * Meta/Cmd (macOS) so a single registration covers all platforms.
 *
 * Usage:
 *   const registry = new ShortcutRegistry();
 *   const unregister = registry.register('ctrl+n', () => openNewTask());
 *   // On cleanup:
 *   unregister();
 */

type ShortcutHandler = (event: KeyboardEvent) => void;

interface Shortcut {
  key: string; // lowercase single key character, e.g. 'n', ',', '1'
  ctrl: boolean;
  handler: ShortcutHandler;
}

export class ShortcutRegistry {
  private shortcuts: Map<string, Shortcut> = new Map();
  private listener: (e: KeyboardEvent) => void;

  constructor() {
    this.listener = (e: KeyboardEvent) => this.handleKeydown(e);
    window.addEventListener('keydown', this.listener);
  }

  /**
   * Register a shortcut.
   *
   * @param combo - Format: "ctrl+<key>", e.g. "ctrl+n", "ctrl+1", "ctrl+,"
   * @param handler - Called when the shortcut fires.
   * @returns Unregister function — call on component unmount.
   */
  register(combo: string, handler: ShortcutHandler): () => void {
    const parsed = this.parse(combo);
    if (!parsed) {
      throw new Error(`Invalid shortcut combo: "${combo}"`);
    }
    const id = combo.toLowerCase();
    this.shortcuts.set(id, { ...parsed, handler });
    return () => this.shortcuts.delete(id);
  }

  /** Remove all registered shortcuts and detach the global listener. */
  destroy(): void {
    this.shortcuts.clear();
    window.removeEventListener('keydown', this.listener);
  }

  private parse(combo: string): { key: string; ctrl: boolean } | null {
    const parts = combo.toLowerCase().split('+');
    if (parts.length < 2) return null;
    const modifiers = parts.slice(0, -1);
    const key = parts[parts.length - 1];
    const ctrl = modifiers.includes('ctrl') || modifiers.includes('cmd');
    return { key, ctrl };
  }

  private handleKeydown(e: KeyboardEvent): void {
    const hasModifier = e.ctrlKey || e.metaKey;
    if (!hasModifier) return;
    for (const shortcut of this.shortcuts.values()) {
      if (shortcut.ctrl && e.key.toLowerCase() === shortcut.key) {
        shortcut.handler(e);
      }
    }
  }
}
```

- [ ] **Step 9.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/keyboard.test.ts 2>&1 | tail -10
```

Expected: `5 tests passed`.

- [ ] **Step 9.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/keyboard.ts src/lib/keyboard.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add ShortcutRegistry — cross-platform keyboard shortcut system

ctrl+<key> combos match both Ctrl (Win/Linux) and Meta/Cmd (macOS) via a
single registration. register() returns an unregister fn for cleanup.
destroy() tears down the global listener. 5 Vitest tests covering
Ctrl, Meta, no-modifier, unregister, and independent coexistence.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: Wire 5 baseline shortcuts in `App.svelte` + tests

**Files:**

- Create: `src/lib/components/App.svelte` (or modify if already exists)
- Create: `src/lib/components/App.test.ts` (new file — shortcuts section)

Note: `App.svelte` is introduced fully in Task 11. This task only covers the
shortcuts wiring. Write the shortcuts test to be importable independently of the
full App render.

- [ ] **Step 10.1: Write failing tests**

```typescript
// src/lib/components/App.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock stores so we can observe their side effects
vi.mock('$lib/stores/mode.svelte', () => ({
  modeStore: { mode: 'plan', set: vi.fn() },
}));
vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: { selectedTaskId: null, selectTask: vi.fn() },
}));
vi.mock('$lib/ipc', () => ({
  api: {
    repo: { list: vi.fn().mockResolvedValue([]) },
    workspace: { list: vi.fn().mockResolvedValue([]) },
    task: { list: vi.fn().mockResolvedValue([]) },
    system: { getAppVersion: vi.fn().mockResolvedValue('0.3.0') },
  },
}));

import { render } from '@testing-library/svelte';
import { modeStore } from '$lib/stores/mode.svelte';
import App from './App.svelte';

function fire(
  key: string,
  opts: { ctrlKey?: boolean; metaKey?: boolean } = {}
) {
  window.dispatchEvent(
    new KeyboardEvent('keydown', {
      key,
      ctrlKey: opts.ctrlKey ?? false,
      metaKey: opts.metaKey ?? false,
      bubbles: true,
    })
  );
}

describe('App shortcuts', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('Ctrl+1 switches to plan mode', async () => {
    render(App);
    fire('1', { ctrlKey: true });
    expect(modeStore.set).toHaveBeenCalledWith('plan');
  });

  it('Ctrl+2 switches to work mode', async () => {
    render(App);
    fire('2', { ctrlKey: true });
    expect(modeStore.set).toHaveBeenCalledWith('work');
  });

  it('Meta+1 (macOS) switches to plan mode', async () => {
    render(App);
    fire('1', { metaKey: true });
    expect(modeStore.set).toHaveBeenCalledWith('plan');
  });
});
```

- [ ] **Step 10.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/App.test.ts 2>&1 | tail -10
```

Expected: `Cannot find module './App.svelte'` (component not yet created).

- [ ] **Step 10.3: Implement**

Shortcut wiring will be included in the full `App.svelte` created in Task 11.
For now, create a minimal stub that makes these tests pass:

```svelte
<!-- src/lib/components/App.svelte — stub for Task 10; extended in Task 11 -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { ShortcutRegistry } from '$lib/keyboard';
  import { modeStore } from '$lib/stores/mode.svelte';

  let registry: ShortcutRegistry;

  onMount(() => {
    registry = new ShortcutRegistry();
    registry.register('ctrl+1', () => modeStore.set('plan'));
    registry.register('ctrl+2', () => modeStore.set('work'));
    // ctrl+n, ctrl+, and ctrl+e are no-ops at this phase; registered for
    // future phases to attach handlers without structural changes.
    registry.register('ctrl+n', () => {});
    registry.register('ctrl+,', () => {});
    registry.register('ctrl+e', () => {});
  });

  onDestroy(() => {
    registry?.destroy();
  });
</script>

<div class="app-stub">
  <!-- Full layout wired in Task 11 -->
</div>
```

- [ ] **Step 10.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/App.test.ts 2>&1 | tail -10
```

Expected: `3 tests passed`.

- [ ] **Step 10.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/App.svelte src/lib/components/App.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): wire 5 baseline shortcuts in App.svelte via ShortcutRegistry

Ctrl/Cmd+1 → plan mode, Ctrl/Cmd+2 → work mode. Ctrl+N/,/E stubs
registered for future phases. Registry created on mount, destroyed on
unmount to avoid listener leaks. 3 Vitest tests covering Ctrl+1/2 and
Meta+1 (macOS Cmd).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: `App.svelte` Plan/Work mode integration + tests

**Files:**

- Modify: `src/lib/components/App.svelte`
- Modify: `src/lib/components/App.test.ts`

- [ ] **Step 11.1: Write failing tests**

Append to `src/lib/components/App.test.ts`:

```typescript
// Append to App.test.ts — integration rendering tests

describe('App mode rendering', () => {
  it('renders KanbanBoard when mode is plan', async () => {
    // Override modeStore.mode to 'plan' for this test
    vi.mocked(modeStore).mode = 'plan';
    const { container } = render(App);
    // KanbanBoard renders a div.kanban-board
    expect(container.querySelector('.kanban-board')).toBeTruthy();
  });

  it('renders Work mode placeholder when mode is work', async () => {
    vi.mocked(modeStore).mode = 'work';
    const { container } = render(App);
    expect(container.querySelector('.work-placeholder')).toBeTruthy();
    expect(container.querySelector('.kanban-board')).toBeNull();
  });

  it('TitleBar is always rendered regardless of mode', async () => {
    const { container } = render(App);
    expect(container.querySelector('.titlebar')).toBeTruthy();
  });
});
```

- [ ] **Step 11.2: Run test — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/App.test.ts 2>&1 | tail -10
```

Expected: `.kanban-board` not found — App stub has no real layout.

- [ ] **Step 11.3: Implement**

Replace `src/lib/components/App.svelte` with the full integration:

```svelte
<!-- src/lib/components/App.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { ShortcutRegistry } from '$lib/keyboard';
  import { modeStore } from '$lib/stores/mode.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import { repos } from '$lib/stores/repos.svelte';
  import TitleBar from './TitleBar.svelte';
  import KanbanBoard from './kanban/KanbanBoard.svelte';
  import NewTaskDialog from './kanban/NewTaskDialog.svelte';
  import type { KanbanColumn } from '$lib/types';

  let registry: ShortcutRegistry;
  let showNewTask = $state(false);

  const selectedRepo = $derived(repos.getSelected());

  const boardTasks = $derived(
    selectedRepo ? tasks.listForRepo(selectedRepo.id) : []
  );

  onMount(async () => {
    registry = new ShortcutRegistry();
    registry.register('ctrl+1', () => modeStore.set('plan'));
    registry.register('ctrl+2', () => modeStore.set('work'));
    registry.register('ctrl+n', () => {
      if (modeStore.mode === 'plan') showNewTask = true;
    });
    registry.register('ctrl+,', () => {
      // Settings — no-op until Phase 2
    });
    registry.register('ctrl+e', () => {
      // Focus repo dropdown — no-op until Phase 2
    });

    await repos.load();
    if (repos.selectedRepoId) {
      await tasks.loadForRepo(repos.selectedRepoId);
    }
  });

  onDestroy(() => {
    registry?.destroy();
  });

  async function handleMove(
    taskId: string,
    column: KanbanColumn,
    order: number
  ) {
    await tasks.move(taskId, column, order);
  }

  async function handleAddTask(data: { title: string; description: string }) {
    if (!selectedRepo) return;
    await tasks.add({
      repoId: selectedRepo.id,
      title: data.title,
      description: data.description,
      column: 'todo',
    });
    showNewTask = false;
  }

  async function handleRemoveTask(taskId: string) {
    await tasks.remove(taskId);
  }
</script>

<div class="app">
  <TitleBar
    mode={modeStore.mode}
    onModeChange={(next) => modeStore.set(next)}
  />

  <main class="app__main">
    {#if modeStore.mode === 'plan'}
      {#if selectedRepo}
        <KanbanBoard
          repoId={selectedRepo.id}
          tasks={boardTasks}
          onMove={handleMove}
          onAddTask={() => (showNewTask = true)}
          onRemoveTask={handleRemoveTask}
        />
      {:else}
        <div class="app__empty">
          <p>No repository selected. Add a repo to get started.</p>
        </div>
      {/if}
    {:else}
      <section class="work-placeholder">
        <p>Work mode — chat panel coming in Phase 1c.</p>
      </section>
    {/if}
  </main>

  <NewTaskDialog
    open={showNewTask}
    onSubmit={handleAddTask}
    onCancel={() => (showNewTask = false)}
  />
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    background: var(--color-surface-0);
  }

  .app__main {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .app__empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-text-muted);
    font-size: 0.875rem;
  }

  .work-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-text-muted);
    font-size: 0.875rem;
  }
</style>
```

- [ ] **Step 11.4: Run test — verify pass**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/App.test.ts 2>&1 | tail -10
```

Expected: all 6 tests pass (3 shortcuts + 3 mode rendering).

- [ ] **Step 11.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/App.svelte src/lib/components/App.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): integrate App.svelte with Plan/Work mode and KanbanBoard

App renders KanbanBoard in plan mode, Work placeholder in work mode,
TitleBar always visible. Ctrl+N opens NewTaskDialog in plan mode. Hydrates
repos + tasks on mount. 3 new rendering tests (kanban visible in plan,
placeholder in work, titlebar always present).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: E2E `tests/e2e/phase-1b/kanban.spec.ts`

**Files:**

- Create: `tests/e2e/phase-1b/kanban.spec.ts`

- [ ] **Step 12.1: Write the E2E spec**

```typescript
// tests/e2e/phase-1b/kanban.spec.ts
import { test, expect, type Page } from '@playwright/test';
import { TauriDevHarness } from '../helpers/tauri-driver';
import { setupFixtureRepo, teardownFixtureRepo } from '../helpers/fixtures';
import type { Repo, Task } from '$lib/types';

/**
 * Phase 1b golden-path E2E:
 *   1. Add a repo programmatically via invoke
 *   2. Switch to Plan mode (already default)
 *   3. Add a task via UI ("+ Add task" → NewTaskDialog → submit)
 *   4. Verify card appears in Todo column
 *   5. Drag card from Todo → In Progress via dispatchEvent (simulated DnD)
 *   6. Assert backend created a workspace (auto-created by move_task)
 *   7. Verify task card shows branch badge after workspace creation
 *   8. Verify Sidebar (if visible) shows the new workspace
 */

let harness: TauriDevHarness;
let fixtureRepoPath: string;

test.beforeAll(async () => {
  fixtureRepoPath = await setupFixtureRepo();
  harness = new TauriDevHarness();
  await harness.launch();
});

test.afterAll(async () => {
  await harness.close();
  await teardownFixtureRepo(fixtureRepoPath);
});

async function addRepoViaInvoke(page: Page, path: string): Promise<Repo> {
  return page.evaluate(async (repoPath) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<Repo>('add_repo', { path: repoPath });
  }, path);
}

async function openNewTaskDialog(page: Page): Promise<void> {
  const addBtn = page.getByRole('button', { name: /add task/i });
  await addBtn.click();
  await expect(page.getByRole('heading', { name: /new task/i })).toBeVisible();
}

async function fillAndSubmitTask(
  page: Page,
  title: string,
  description: string
): Promise<void> {
  await page.getByLabel(/title/i).fill(title);
  await page.getByLabel(/description/i).fill(description);
  await page
    .getByRole('button', { name: /add task/i })
    .last()
    .click();
}

async function simulateDragToColumn(
  page: Page,
  taskTitle: string,
  targetColumn: 'todo' | 'in_progress' | 'review' | 'done'
): Promise<void> {
  // Get the task's id from the DOM data attribute, then fire a synthetic
  // finalize event on the target column's dnd zone.
  await page.evaluate(
    async ({ title, column }) => {
      const cards = document.querySelectorAll('[role="listitem"]');
      let taskId: string | null = null;
      for (const card of cards) {
        if (card.textContent?.includes(title)) {
          taskId = card.getAttribute('data-task-id');
          break;
        }
      }
      if (!taskId) throw new Error(`Task card not found: ${title}`);
      const zone = document.querySelector(`[data-column="${column}"]`);
      if (!zone) throw new Error(`Column zone not found: ${column}`);
      // Dispatch a synthetic finalize event that svelte-dnd-action listens to
      zone.dispatchEvent(
        new CustomEvent('finalize', {
          detail: {
            items: [{ id: taskId, column }],
            info: { id: taskId },
          },
          bubbles: true,
        })
      );
    },
    { title: taskTitle, column: targetColumn }
  );
}

test('kanban golden path: add task → drag Todo→InProgress → workspace auto-created → branch badge visible', async () => {
  const page = harness.page;

  // 1. Add a repo programmatically
  const repo = await addRepoViaInvoke(page, fixtureRepoPath);
  expect(repo.id).toMatch(/^repo_/);

  // Reload stores (navigate or reload to pick up the new repo)
  await page.reload();
  await page.waitForSelector('.kanban-board');

  // 2. Verify Plan mode is active by default
  const planBtn = page.getByRole('button', { name: /^plan$/i });
  await expect(planBtn).toHaveClass(/active/);

  // 3. Open NewTaskDialog and add a task
  await openNewTaskDialog(page);
  await fillAndSubmitTask(
    page,
    'Implement login flow',
    'Build the full authentication flow'
  );

  // 4. Task card appears in Todo column
  const todoColumn = page.locator('[data-column="todo"]');
  await expect(todoColumn.getByText('Implement login flow')).toBeVisible();

  // 5. Drag Todo → In Progress
  await simulateDragToColumn(page, 'Implement login flow', 'in_progress');

  // 6. Backend should have auto-created a workspace when the task moved to in_progress.
  //    Poll for the workspace appearing in the backend state.
  await page.waitForFunction(
    async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      const workspaces = await invoke<{ id: string; task_id?: string }[]>(
        'list_workspaces',
        { repoId: undefined }
      );
      return workspaces.some((w) => w.task_id != null);
    },
    { timeout: 5000 }
  );

  // 7. Task card in In Progress column now shows the branch badge
  const inProgressColumn = page.locator('[data-column="in_progress"]');
  await expect(
    inProgressColumn.getByText('Implement login flow')
  ).toBeVisible();
  await expect(
    inProgressColumn.locator('[data-testid="branch-badge"]')
  ).toBeVisible({ timeout: 3000 });

  // 8. Verify Sidebar reflects the new workspace (if Sidebar renders workspace list)
  //    The Sidebar is added in Phase 1a — check it shows at least one workspace item.
  const sidebarWorkspace = page
    .locator('.sidebar')
    .getByRole('listitem')
    .first();
  await expect(sidebarWorkspace).toBeVisible({ timeout: 3000 });
});

test('Plan/Work toggle shortcut switches mode', async () => {
  const page = harness.page;

  // Start in plan mode
  await expect(page.getByRole('button', { name: /^plan$/i })).toHaveClass(
    /active/
  );

  // Ctrl+2 → work mode
  await page.keyboard.press('Control+2');
  await expect(page.getByRole('button', { name: /^work$/i })).toHaveClass(
    /active/
  );
  await expect(page.locator('.work-placeholder')).toBeVisible();
  await expect(page.locator('.kanban-board')).not.toBeVisible();

  // Ctrl+1 → plan mode
  await page.keyboard.press('Control+1');
  await expect(page.getByRole('button', { name: /^plan$/i })).toHaveClass(
    /active/
  );
  await expect(page.locator('.kanban-board')).toBeVisible();
});

test('Ctrl+N opens NewTaskDialog in plan mode', async () => {
  const page = harness.page;

  // Ensure plan mode
  await page.keyboard.press('Control+1');
  await expect(page.locator('.kanban-board')).toBeVisible();

  await page.keyboard.press('Control+n');
  await expect(page.getByRole('heading', { name: /new task/i })).toBeVisible();

  // Dismiss
  await page.keyboard.press('Escape');
});
```

- [ ] **Step 12.2: Run E2E — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bunx playwright test tests/e2e/phase-1b/kanban.spec.ts 2>&1 | tail -20
```

Expected: tests fail because the Phase 1b-backend commands (`add_task`,
`move_task`, workspace auto-creation) + UI components are not yet integrated in
a running app.

- [ ] **Step 12.3: Integrate and run full app**

Ensure the Tauri app builds with all components:

```bash
cd /home/handokobeni/Work/ai-editor
bun run check 2>&1 | tail -20
```

Fix any type errors revealed by the type checker, then run the E2E suite:

```bash
cd /home/handokobeni/Work/ai-editor
bunx playwright test tests/e2e/phase-1b/ 2>&1 | tail -30
```

- [ ] **Step 12.4: Run full unit test suite + coverage check**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test --coverage 2>&1 | tail -20
```

Expected: coverage threshold met (95% lines + branches on changed files).

- [ ] **Step 12.5: Commit and tag**

```bash
cd /home/handokobeni/Work/ai-editor
git add tests/e2e/phase-1b/kanban.spec.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1b): add E2E kanban golden-path spec

3 Playwright scenarios: (1) add task → drag Todo→InProgress → assert
workspace auto-created + branch badge visible + sidebar reflects workspace;
(2) Plan/Work toggle via Ctrl+1/2; (3) Ctrl+N opens NewTaskDialog.
Uses TauriDevHarness fixture with programmatic repo injection via invoke.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" tag -a v0.3.0-phase1b -m "Phase 1b: kanban UI, drag-drop, Plan/Work mode, baseline shortcuts"
```

---

## Phase 1b shipping criteria (backend + frontend combined)

- [ ] All unit + E2E tests pass, coverage gate 95%
- [ ] Manual smoke: `bun tauri dev` → add repo → Plan mode → add task in Todo →
      drag to In Progress → workspace auto-appears in sidebar + task card shows
      branch badge
- [ ] Plan/Work toggle via shortcut (Cmd/Ctrl+1 / +2) and click
- [ ] Git tag `v0.3.0-phase1b`

---

## Known deferrals to Phase 1c

- Chat panel (replaces Work mode placeholder)
- Claude CLI spawn + stream-json parser
- Messages persistence
- Sidebar workspace list from Phase 1a wired to auto-created workspaces
- Repo picker dropdown in TitleBar (Phase 2 settings UI)
- Settings modal (Phase 2)
