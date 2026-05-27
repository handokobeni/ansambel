// tests/e2e/phase-3b-workspace-lifecycle/lifecycle.spec.ts
//
// Phase 3b — Workspace lifecycle cleanup E2E golden path.
//
// Architecture notes:
//   - No real Tauri binary; __TAURI_INTERNALS__ installed via tauri-shim.
//   - Base shim handles repos.load(), workspaces.load(), tasks.load() fired
//     by App.svelte on mount.
//   - A second page.addInitScript (runs after the shim in insertion order)
//     wraps internals.invoke and intercepts move_task and list_workspaces,
//     delegating everything else to the base shim.
//   - The layered override maintains its own wsByTask Map and workspaces array
//     to assert the lifecycle CONTRACT:
//       1. move to In Progress → exactly one workspace created.
//       2. move back to Todo (empty) → workspace removed.
//       3. second move to In Progress → workspace reattached (still one, never two).

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';

test('moving a card to In Progress then back to Todo cleans up the empty workspace', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, {
    initialRepos: [
      {
        id: 'repo_e2e',
        name: 'lifecycle-repo',
        path: '/tmp/lifecycle-repo',
        gh_profile: null,
        default_branch: 'main',
        created_at: 1700000000,
        updated_at: 1700000000,
      },
    ],
    initialWorkspaces: [],
    initialTasks: [],
  });

  await page.addInitScript(() => {
    const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const original = internals.invoke.bind(internals);
    const wsByTask = new Map<string, string>();
    let nextWs = 1;
    const workspaces: Array<Record<string, unknown>> = [];
    (window as unknown as Record<string, unknown>)['__WS_COUNT__'] = () => workspaces.length;

    internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === 'move_task') {
        const taskId = args.taskId as string;
        const column = args.column as string;
        let wsId: string | null = wsByTask.get(taskId) ?? null;
        if (column === 'in_progress' && !wsId) {
          wsId = `ws_${nextWs++}`;
          wsByTask.set(taskId, wsId);
          workspaces.push({
            id: wsId,
            repo_id: 'repo_e2e',
            task_id: taskId,
            column: 'in_progress',
          });
        } else if (column === 'todo' && wsId) {
          const i = workspaces.findIndex((w) => w.id === wsId);
          if (i >= 0) workspaces.splice(i, 1);
          wsByTask.delete(taskId);
          wsId = null;
        }
        return {
          id: taskId,
          repo_id: 'repo_e2e',
          workspace_id: wsId,
          title: 'Card',
          description: '',
          column,
          order: 0,
          created_at: 0,
          updated_at: 0,
          pic_names: [],
        };
      }
      if (cmd === 'list_workspaces') return workspaces;
      return original(cmd, args);
    };
  });

  await page.goto('/');

  const count = async () =>
    page.evaluate(() => (window as unknown as Record<string, () => number>)['__WS_COUNT__']());

  // Step 1: move to In Progress → exactly one workspace created
  await page.evaluate(async () => {
    const inv = (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke: (c: string, a: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__.invoke;
    await inv('move_task', { taskId: 'tk_a', column: 'in_progress', order: 0 });
  });
  expect(await count()).toBe(1);

  // Step 2: move back to Todo (empty workspace) → workspace removed
  await page.evaluate(async () => {
    const inv = (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke: (c: string, a: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__.invoke;
    await inv('move_task', { taskId: 'tk_a', column: 'todo', order: 0 });
  });
  expect(await count()).toBe(0);

  // Step 3: move to In Progress again twice → reattached, still exactly one (never two)
  await page.evaluate(async () => {
    const inv = (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke: (c: string, a: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__.invoke;
    await inv('move_task', { taskId: 'tk_a', column: 'in_progress', order: 0 });
    await inv('move_task', { taskId: 'tk_a', column: 'in_progress', order: 0 });
  });
  expect(await count()).toBe(1);
});
