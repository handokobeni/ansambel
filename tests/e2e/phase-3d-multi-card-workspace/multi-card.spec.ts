// tests/e2e/phase-3d-multi-card-workspace/multi-card.spec.ts
//
// Phase 3d — Multi-card workspace E2E golden paths.
//
// Architecture notes:
//   - No real Tauri binary; __TAURI_INTERNALS__ installed via tauri-shim.
//   - The base shim handles repos.load(), workspaces.load(), tasks.load() and
//     also implements move_task (auto-creates a workspace + sets
//     task.workspace_id). It does NOT, however, populate the new
//     `task_ids: string[]` field on the auto-created workspace, and it has no
//     handlers for `link_task_to_workspace` / `unlink_task_from_workspace`.
//   - A second page.addInitScript layers on top of the shim's invoke and:
//       * forwards move_task to the base shim, then mirrors the returned
//         task.workspace_id into the matching workspace's task_ids[] so the
//         sidebar's per-row card-count + expand list reflects reality;
//       * implements link_task_to_workspace + unlink_task_from_workspace with
//         the refcount-aware cleanup semantics the real backend ships;
//       * delegates everything else to the base shim.
//   - DnD: rather than driving real pointer events into svelte-dnd-action
//     (brittle in Playwright), the spec dispatches the same synthetic
//     `finalize` CustomEvent the kanban listens for — identical to phase-1b's
//     kanban spec — so the App.svelte `handleMove` path (including the
//     auto-create undo toast) runs end-to-end.
//   - Per-card menu trigger is selected by scoping the unscoped
//     `data-testid="task-menu-trigger"` under the per-card wrapper
//     `data-task-id` set by KanbanBoard, sidestepping the need to patch a
//     per-task-id testid (which would have broken existing unit tests).

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';

const REPO_ID = 'repo_e2e';

interface MovedTask {
  id: string;
  repo_id: string;
  workspace_id: string | null;
  title: string;
  description: string;
  column: string;
  order: number;
  created_at: number;
  updated_at: number;
}

/** Install the multi-card override on top of the base shim. */
async function installMultiCardOverride(page: import('@playwright/test').Page): Promise<void> {
  await page.addInitScript(() => {
    type Internals = {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const internals = (window as unknown as Record<string, unknown>)[
      '__TAURI_INTERNALS__'
    ] as Internals;
    const original = internals.invoke.bind(internals);

    // Fetch the (mutable) workspaces + tasks arrays the base shim manages by
    // calling its list_* commands. We mutate the returned objects in place —
    // since the base shim hands back live references (not copies of the
    // inner objects), our changes persist.
    async function fetchWorkspaces(): Promise<
      Array<{ id: string; repo_id: string; task_ids?: string[] }>
    > {
      return (await original('list_workspaces', {})) as Array<{
        id: string;
        repo_id: string;
        task_ids?: string[];
      }>;
    }
    async function fetchTasks(): Promise<
      Array<{ id: string; repo_id: string; workspace_id: string | null }>
    > {
      return (await original('list_tasks', {})) as Array<{
        id: string;
        repo_id: string;
        workspace_id: string | null;
      }>;
    }

    internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
      // Both of these are called unconditionally from App.svelte's onMount.
      // The base shim returns undefined for unknown commands, which causes
      // `Object.entries(undefined)` to throw in the lark-bindings store and
      // aborts the auto-select repo / hydrate tasks sequence — leaving the
      // board empty. Returning shaped defaults mirrors phase-3c's pattern.
      if (cmd === 'list_lark_repo_bindings') return {};
      if (cmd === 'fetch_team_activity_rows') return { kind: 'disabled' };
      if (cmd === 'get_selected_repo') return null;

      // Clone workspaces on every read. The base shim returns live refs
      // to its inner workspace objects; if we mutate `ws.task_ids` in-place
      // (which we do in the move_task / link / unlink overrides below) and
      // then hand the same ref back to `workspaces.loadForRepo`, the
      // SvelteMap stays at version unchanged (prev_res === value) and the
      // sidebar doesn't re-derive. Deep-cloning here guarantees a fresh
      // identity per `list_workspaces` call, which the SvelteMap notices.
      if (cmd === 'list_workspaces') {
        const list = (await original(cmd, args)) as Array<Record<string, unknown>>;
        return list.map((w) => ({ ...w, task_ids: [...((w.task_ids as string[]) ?? [])] }));
      }
      if (cmd === 'list_tasks') {
        const list = (await original(cmd, args)) as Array<Record<string, unknown>>;
        return list.map((t) => ({ ...t }));
      }

      if (cmd === 'move_task') {
        const result = (await original(cmd, args)) as
          | { id: string; workspace_id: string | null }
          | undefined;
        if (result && result.workspace_id) {
          const allWs = await fetchWorkspaces();
          const ws = allWs.find((w) => w.id === result.workspace_id);
          if (ws) {
            const ids = (ws.task_ids ?? []) as string[];
            if (!ids.includes(result.id)) ids.push(result.id);
            ws.task_ids = ids;
          }
        }
        return result;
      }

      if (cmd === 'link_task_to_workspace') {
        const taskId = args.taskId as string;
        const newWsId = args.workspaceId as string;
        const allWs = await fetchWorkspaces();
        const allTasks = await fetchTasks();
        const task = allTasks.find((t) => t.id === taskId);
        if (!task) return undefined;
        // Detach from existing workspace (refcount cleanup if it goes empty).
        if (task.workspace_id && task.workspace_id !== newWsId) {
          const oldWs = allWs.find((w) => w.id === task.workspace_id);
          if (oldWs) {
            oldWs.task_ids = (oldWs.task_ids ?? []).filter((id: string) => id !== taskId);
            if (oldWs.task_ids.length === 0) {
              // Mirror refcount-aware cleanup: remove empty workspace.
              await original('remove_workspace', { workspaceId: oldWs.id });
            }
          }
        }
        const newWs = allWs.find((w) => w.id === newWsId);
        if (newWs) {
          const ids = (newWs.task_ids ?? []) as string[];
          if (!ids.includes(taskId)) ids.push(taskId);
          newWs.task_ids = ids;
        }
        task.workspace_id = newWsId;
        return undefined;
      }

      if (cmd === 'unlink_task_from_workspace') {
        const taskId = args.taskId as string;
        const force = args.force as boolean;
        const allWs = await fetchWorkspaces();
        const allTasks = await fetchTasks();
        const task = allTasks.find((t) => t.id === taskId);
        if (!task || !task.workspace_id) {
          return { kind: 'unlinked' };
        }
        const ws = allWs.find((w) => w.id === task.workspace_id);
        if (!ws) {
          return { kind: 'unlinked' };
        }
        const remainingAfter = (ws.task_ids ?? []).filter((id: string) => id !== taskId);
        const wouldRemove = remainingAfter.length === 0;
        if (!force && wouldRemove) {
          return {
            kind: 'would_remove',
            workspace_title: (ws as unknown as { title: string }).title,
          };
        }
        ws.task_ids = remainingAfter;
        task.workspace_id = null;
        if (wouldRemove) {
          await original('remove_workspace', { workspaceId: ws.id });
          return { kind: 'removed' };
        }
        return { kind: 'unlinked' };
      }

      return original(cmd, args);
    };
  });
}

/**
 * Dispatch the same synthetic `finalize` CustomEvent that svelte-dnd-action
 * fires on drop. Mirrors the helper baked into phase-1b/kanban.spec.ts but
 * lifted into the spec file for readability.
 */
async function dragToInProgress(
  page: import('@playwright/test').Page,
  taskId: string
): Promise<MovedTask | null> {
  return page.evaluate(
    ({ tid }) => {
      const zone = document.querySelector('[data-column="in_progress"]') as HTMLElement | null;
      if (!zone) return null;
      const moved = {
        id: tid,
        repo_id: 'repo_e2e',
        workspace_id: null as string | null,
        title: tid,
        description: '',
        column: 'in_progress',
        order: 0,
        created_at: Math.floor(Date.now() / 1000),
        updated_at: Math.floor(Date.now() / 1000),
      };
      zone.dispatchEvent(
        new CustomEvent('finalize', {
          detail: { items: [moved], info: { id: tid } },
          bubbles: true,
        })
      );
      return moved;
    },
    { tid: taskId }
  );
}

test.describe('multi-card workspace golden paths', () => {
  test('auto-create + Undo toast removes the workspace', async ({ page, harness }) => {
    void harness;
    await installTauriShim(page, {
      initialRepos: [
        {
          id: REPO_ID,
          name: 'mcw-repo',
          path: '/tmp/mcw-repo',
          gh_profile: null,
          default_branch: 'main',
          created_at: 1700000000,
          updated_at: 1700000000,
        },
      ],
      initialWorkspaces: [],
      initialTasks: [
        {
          id: 'tk_a',
          repo_id: REPO_ID,
          workspace_id: null,
          title: 'Card A',
          description: '',
          column: 'todo',
          order: 0,
          created_at: 1700000000,
          updated_at: 1700000000,
        },
      ],
    });
    await installMultiCardOverride(page);

    await page.goto('/');

    // Wait for the kanban board to hydrate.
    await expect(page.locator('[data-task-id="tk_a"]')).toBeVisible({ timeout: 10_000 });

    // Drag tk_a → In Progress (synthetic finalize event).
    const moved = await dragToInProgress(page, 'tk_a');
    expect(moved).not.toBeNull();

    // The auto-create undo toast should appear.
    const undoButton = page.getByTestId('toast-action-Undo create');
    await expect(undoButton).toBeVisible({ timeout: 5_000 });

    // Capture the new workspace id from the sidebar before we undo: the
    // toast text reads `Created workspace «<title>»`, but the testid we want
    // to assert disappearance of is `ws-row-card-count-<id>`.
    const wsRow = page.locator('[data-testid^="ws-row-card-count-"]').first();
    await expect(wsRow).toBeVisible({ timeout: 5_000 });
    const wsTestId = await wsRow.getAttribute('data-testid');
    expect(wsTestId).toMatch(/^ws-row-card-count-/);

    // Click Undo create.
    await undoButton.click();

    // The workspace row should disappear and the card should return to Todo.
    await expect(page.getByTestId(wsTestId!)).toHaveCount(0, { timeout: 5_000 });
    await expect(page.locator('[data-column="todo"] [data-task-id="tk_a"]')).toBeVisible({
      timeout: 5_000,
    });
  });

  test('link via card menu attaches a second card to W', async ({ page, harness }) => {
    void harness;
    await installTauriShim(page, {
      initialRepos: [
        {
          id: REPO_ID,
          name: 'mcw-repo',
          path: '/tmp/mcw-repo',
          gh_profile: null,
          default_branch: 'main',
          created_at: 1700000000,
          updated_at: 1700000000,
        },
      ],
      initialWorkspaces: [],
      initialTasks: [
        {
          id: 'tk_a',
          repo_id: REPO_ID,
          workspace_id: null,
          title: 'Card A',
          description: '',
          column: 'todo',
          order: 0,
          created_at: 1700000000,
          updated_at: 1700000000,
        },
        {
          id: 'tk_b',
          repo_id: REPO_ID,
          workspace_id: null,
          title: 'Card B',
          description: '',
          column: 'todo',
          order: 1,
          created_at: 1700000001,
          updated_at: 1700000001,
        },
      ],
    });
    await installMultiCardOverride(page);

    await page.goto('/');
    await expect(page.locator('[data-task-id="tk_a"]')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('[data-task-id="tk_b"]')).toBeVisible();

    // Step 1: Auto-create W from card A by moving it to In Progress.
    await dragToInProgress(page, 'tk_a');

    // Wait for the workspace's sidebar row to render.
    const countLocator = page.locator('[data-testid^="ws-row-card-count-"]').first();
    await expect(countLocator).toBeVisible({ timeout: 5_000 });
    await expect(countLocator).toContainText('1 card');
    const countTestId = (await countLocator.getAttribute('data-testid')) as string;
    const wsId = countTestId.replace('ws-row-card-count-', '');

    // Dismiss the auto-create undo toast so it doesn't sit on top of the
    // sidebar while we drive the picker (the toast is sticky for 10 s).
    const undoBtn = page.getByTestId('toast-action-Undo create');
    if (await undoBtn.isVisible().catch(() => false)) {
      // The toast has a dismiss × — but the simplest cleanup is to just
      // proceed; the actions don't block hit-testing because the picker is
      // a z-50 modal layered above the toast.
    }

    // Step 2: Open card B's menu and click "Link to workspace…".
    const cardB = page.locator('[data-task-id="tk_b"]');
    // Hover the card so the opacity-0 group-hover menu trigger is visible.
    await cardB.hover();
    await cardB.locator('[data-testid="task-menu-trigger"]').click();
    await page.getByTestId('task-menu-link-workspace').click();

    // Step 3: Click the first picker row (only W exists in the repo).
    await expect(page.getByTestId('link-picker-row').first()).toBeVisible({ timeout: 5_000 });
    await page.getByTestId('link-picker-row').first().click();

    // Step 4: The sidebar row's card count must now read "2 cards".
    await expect(page.getByTestId(`ws-row-card-count-${wsId}`)).toContainText('2 cards', {
      timeout: 5_000,
    });

    // Step 5: Expand the row and confirm BOTH titles surface.
    await page.getByTestId(`ws-row-expand-${wsId}`).click();
    await expect(page.getByTestId('ws-row-card-tk_a')).toBeVisible({ timeout: 5_000 });
    await expect(page.getByTestId('ws-row-card-tk_b')).toBeVisible();
  });

  test('unlink confirm modal fires when last + empty', async ({ page, harness }) => {
    void harness;
    // Single card already linked to an empty (no other cards) workspace.
    await installTauriShim(page, {
      initialRepos: [
        {
          id: REPO_ID,
          name: 'mcw-repo',
          path: '/tmp/mcw-repo',
          gh_profile: null,
          default_branch: 'main',
          created_at: 1700000000,
          updated_at: 1700000000,
        },
      ],
      initialWorkspaces: [
        {
          id: 'ws_only',
          repo_id: REPO_ID,
          branch: 'ansambel/only',
          base_branch: 'main',
          custom_branch: false,
          title: 'Only Workspace',
          description: '',
          status: 'waiting',
          column: 'in_progress',
          created_at: 1700000000,
          updated_at: 1700000000,
          worktree_dir: '/mock/worktrees/ws_only',
          team_activity_private: false,
          task_ids: ['tk_a'],
        },
      ],
      initialTasks: [
        {
          id: 'tk_a',
          repo_id: REPO_ID,
          workspace_id: 'ws_only',
          title: 'Card A',
          description: '',
          column: 'in_progress',
          order: 0,
          created_at: 1700000000,
          updated_at: 1700000000,
        },
      ],
    });
    await installMultiCardOverride(page);

    await page.goto('/');
    await expect(page.locator('[data-task-id="tk_a"]')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('ws-row-card-count-ws_only')).toBeVisible({ timeout: 5_000 });

    // Open card A's menu → click Unlink.
    const cardA = page.locator('[data-task-id="tk_a"]');
    await cardA.hover();
    await cardA.locator('[data-testid="task-menu-trigger"]').click();
    await page.getByTestId('task-menu-unlink').click();

    // The would_remove preview triggers the confirm modal.
    await expect(page.getByTestId('unlink-modal-text')).toBeVisible({ timeout: 5_000 });
    await page.getByTestId('unlink-modal-confirm').click();

    // The workspace row should be gone (force-unlink + empty → cleanup).
    await expect(page.getByTestId('ws-row-card-count-ws_only')).toHaveCount(0, {
      timeout: 5_000,
    });
  });
});
