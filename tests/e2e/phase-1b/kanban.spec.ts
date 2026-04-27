// tests/e2e/phase-1b/kanban.spec.ts
//
// Full golden-path E2E test for the Kanban UI introduced in Phase 1b.
//
// Flow:
//  1. Programmatically add a repo via the Tauri shim (clicking Add Repo button).
//  2. Click "+ Add task" in the Todo column.
//  3. Fill in NewTaskDialog (title + description) and submit.
//  4. Assert task card appears in the Todo column.
//  5. Dispatch synthetic dnd `finalize` event to drag the card to In Progress.
//  6. Assert `move_task` was called with column='in_progress'.
//  7. Assert the workspace auto-appeared in the Sidebar (shim side effect).

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-kanban-'));
  FIXTURE_REPO_PATH = tmpDir;
  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 'test@example.com'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.name', 'Test'], { cwd: tmpDir });
  execFileSync('git', ['commit', '--allow-empty', '-m', 'initial'], { cwd: tmpDir });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

test('Kanban: add task → appears in Todo → drag to In Progress → workspace auto-created', async ({
  page,
  harness,
}) => {
  void harness;

  // Track move_task invocations so we can assert on them
  const moveTaskCalls: Array<{ taskId: string; column: string; order: number }> = [];

  await page.exposeFunction(
    '__e2e_recordMoveTask',
    (taskId: string, column: string, order: number) => {
      moveTaskCalls.push({ taskId, column, order });
    }
  );

  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });

  // Wrap the shim's invoke to spy on move_task calls.
  // This runs after installTauriShim so we patch the already-installed invoke.
  await page.addInitScript(() => {
    window.addEventListener('DOMContentLoaded', () => {
      type Internals = { invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown> };
      const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as
        | Internals
        | undefined;
      if (!internals) return;
      const originalInvoke = internals.invoke.bind(internals);
      internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
        const result = await originalInvoke(cmd, args);
        if (cmd === 'move_task') {
          const recorder = (window as unknown as Record<string, unknown>)[
            '__e2e_recordMoveTask'
          ] as ((taskId: string, column: string, order: number) => void) | undefined;
          recorder?.(args.taskId as string, args.column as string, args.order as number);
        }
        return result;
      };
    });
  });

  await page.goto('/');

  // --- Step 1: Add the repo ---
  await page.waitForSelector('header', { timeout: 10000 });
  await expect(page.getByText('No repo selected')).toBeVisible({ timeout: 5000 });

  await page.getByRole('button', { name: /add repo/i }).click();

  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });

  // Plan mode KanbanBoard should be visible (default mode is 'plan')
  await expect(page.getByText('Todo')).toBeVisible({ timeout: 5000 });
  await expect(page.getByText('In Progress')).toBeVisible();

  // --- Step 2: Click "+ Add task" ---
  await page.getByRole('button', { name: /add task/i }).click();

  // --- Step 3: Fill NewTaskDialog and submit ---
  await expect(page.getByRole('dialog')).toBeVisible({ timeout: 3000 });
  await page.getByLabel(/title/i).fill('E2E kanban task');
  await page.getByLabel(/description/i).fill('Created by Playwright E2E test');
  await page.getByRole('button', { name: /^add task$/i }).click();

  // --- Step 4: Assert task card appears in Todo column ---
  const todoColumn = page.locator('[data-column="todo"]');
  await expect(todoColumn.getByText('E2E kanban task')).toBeVisible({ timeout: 5000 });

  // Dialog should be closed
  await expect(page.getByRole('dialog')).not.toBeVisible({ timeout: 3000 });

  // --- Step 5: Dispatch synthetic dnd `finalize` event to drag Todo → In Progress ---
  // svelte-dnd-action uses custom DOM events; we dispatch 'finalize' on the
  // target column zone with the task item in the detail.items list.
  const dispatchResult = await page.evaluate(async () => {
    // Get the task id from the shim state via invoke
    type Internals = { invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown> };
    const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as
      | Internals
      | undefined;
    if (!internals) return { error: 'no internals' };

    const tasks = (await internals.invoke('list_tasks', { repoId: undefined })) as Array<{
      id: string;
      title: string;
      column: string;
    }>;
    const task = tasks.find((t) => t.title === 'E2E kanban task');
    if (!task) return { error: 'task not found', tasks };

    const inProgressZone = document.querySelector(
      '[data-column="in_progress"]'
    ) as HTMLElement | null;
    if (!inProgressZone) return { error: 'in_progress zone not found' };

    const movedTask = {
      id: task.id,
      repo_id: '',
      workspace_id: null as string | null,
      title: task.title,
      description: '',
      column: 'in_progress',
      order: 0,
      created_at: Math.floor(Date.now() / 1000),
      updated_at: Math.floor(Date.now() / 1000),
    };

    const finalizeEvent = new CustomEvent('finalize', {
      detail: { items: [movedTask], info: { id: task.id } },
      bubbles: true,
    });
    inProgressZone.dispatchEvent(finalizeEvent);
    return { taskId: task.id };
  });

  expect(dispatchResult).not.toHaveProperty('error');

  // Give the app time to process the async move + workspace reload
  await page.waitForTimeout(600);

  // --- Step 6: Assert move_task was called with column='in_progress' ---
  expect(moveTaskCalls.length).toBeGreaterThan(0);
  expect(moveTaskCalls[0].column).toBe('in_progress');

  // --- Step 7: Assert workspace auto-appeared in Sidebar ---
  // The shim's move_task handler auto-creates a workspace named after the task title.
  // App.svelte handleMove() calls workspaces.loadForRepo() after moving, so the
  // sidebar should render the new workspace.
  const sidebar = page.locator('aside').first();
  await expect(sidebar.getByText('E2E kanban task')).toBeVisible({ timeout: 5000 });
});
