// tests/e2e/phase-2a/work-mode.spec.ts
//
// Phase 2a golden path: with a Work-mode workspace open, the user can
// switch between Chat / Diff / Files tabs, see the diff render with
// added/removed line tagging, lazy-expand the file tree, and use the
// global Ctrl+P search to jump to a file (which switches to Files tab).

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-2a-'));
  FIXTURE_REPO_PATH = tmpDir;
  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 't@e.com'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.name', 'T'], { cwd: tmpDir });
  execFileSync('git', ['commit', '--allow-empty', '-m', 'init'], { cwd: tmpDir });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

async function openWorkspace(page: import('@playwright/test').Page) {
  await page.waitForSelector('header', { timeout: 10000 });
  await page.getByRole('button', { name: /add repo/i }).click();
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });

  await page.getByRole('button', { name: /add task/i }).click();
  await page.getByLabel(/title/i).fill('Phase 2a smoke task');
  await page.getByLabel(/description/i).fill('Open work mode and exercise tabs');
  await page.getByRole('button', { name: /^add task$/i }).click();

  await page.evaluate(async () => {
    type Internals = {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as
      | Internals
      | undefined;
    if (!internals) return;
    const tasks = (await internals.invoke('list_tasks', {})) as Array<{
      id: string;
      title: string;
    }>;
    const task = tasks.find((t) => t.title === 'Phase 2a smoke task');
    if (!task) return;
    const zone = document.querySelector('[data-column="in_progress"]') as HTMLElement | null;
    zone?.dispatchEvent(
      new CustomEvent('finalize', {
        detail: {
          items: [{ ...task, column: 'in_progress', order: 0 }],
          info: { id: task.id },
        },
        bubbles: true,
      })
    );
  });
  await page.waitForTimeout(500);

  const sidebar = page.locator('aside').first();
  await sidebar.getByText('Phase 2a smoke task').click();
  await page.getByRole('button', { name: /^work$/i }).click();
  await expect(page.getByRole('heading', { name: 'Phase 2a smoke task' })).toBeVisible({
    timeout: 5000,
  });
}

test('Phase 2a: Diff tab renders streamed unified diff with row tagging', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  // The tab strip is visible; chat is active by default.
  await expect(page.getByTestId('tab-strip')).toBeVisible();
  await expect(page.getByTestId('tab-chat')).toHaveAttribute('aria-selected', 'true');

  await page.getByTestId('tab-diff').click();
  await expect(page.getByTestId('tab-diff')).toHaveAttribute('aria-selected', 'true');

  // The mock shim emits one modified file with -old / +new — assert both
  // line kinds rendered.
  await expect(page.getByTestId('diff-file').first()).toBeVisible({ timeout: 5000 });
  const delLine = page.locator('[data-line-kind="del"]').first();
  const addLine = page.locator('[data-line-kind="add"]').first();
  await expect(delLine).toBeVisible();
  await expect(addLine).toBeVisible();
  await expect(delLine).toContainText('old');
  await expect(addLine).toContainText('new');
});

test('Phase 2a: Files tab lazy-loads directory children on click', async ({ page, harness }) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  await page.getByTestId('tab-files').click();
  // Root entries from the shim: src (dir) + README.md (file)
  await expect(page.locator('[data-testid="file-row"]')).toHaveCount(2, { timeout: 5000 });
  await expect(page.locator('[data-path="src"]')).toBeVisible();
  await expect(page.locator('[data-path="README.md"]')).toBeVisible();

  // Click `src` to expand — its child `app.ts` should appear lazily.
  await page.locator('[data-path="src"] button').first().click();
  await expect(page.locator('[data-path="src/app.ts"]')).toBeVisible({ timeout: 5000 });
});

test('Phase 2a: Ctrl+P search → click hit → switch to Files tab with selection', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  // Open Search via the global shortcut. Playwright sends OS-appropriate
  // modifier; on Linux runners ControlOrMeta resolves to Control which
  // matches our `ctrl+p` registration.
  await page.keyboard.press('Control+p');
  await expect(page.getByTestId('search-modal')).toBeVisible({ timeout: 5000 });

  await page.getByTestId('search-input').fill('app');
  await page.getByTestId('search-input').press('Enter');

  // The mock shim returns one filename hit for `app`.
  await expect(page.getByTestId('search-hit')).toHaveCount(1, { timeout: 5000 });

  await page.getByTestId('search-hit').first().locator('button').click();

  // Modal closes; we land on the Files tab and the picked row is
  // revealed — for nested matches FileBrowser auto-expands ancestor
  // directories so the row is actually visible.
  await expect(page.getByTestId('search-modal')).toBeHidden();
  await expect(page.getByTestId('tab-files')).toHaveAttribute('aria-selected', 'true');
  // The shim returns `src/app.ts` — its parent `src` should be expanded
  // and the leaf row marked aria-selected.
  await expect(page.locator('[data-path="src/app.ts"]')).toHaveAttribute('aria-selected', 'true', {
    timeout: 5000,
  });
});
