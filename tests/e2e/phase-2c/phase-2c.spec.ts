// tests/e2e/phase-2c/phase-2c.spec.ts
//
// Phase 2c golden path: with a Work-mode workspace open, the user can
// trigger the @-mention autocomplete in the chat input, navigate the
// dropdown, and select a file. The selected path is inserted into the
// textarea body with a trailing space.

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-2c-'));
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
  await page.getByLabel(/title/i).fill('Phase 2c mention task');
  await page.getByLabel(/description/i).fill('@ mention smoke');
  await page.getByRole('button', { name: /^add task$/i }).click();

  // Force the task into in_progress so a workspace is created.
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
    const task = tasks.find((t) => t.title === 'Phase 2c mention task');
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
  await sidebar.getByText('Phase 2c mention task').click();
  await page.getByRole('button', { name: /^work$/i }).click();
  await expect(page.getByRole('heading', { name: 'Phase 2c mention task' })).toBeVisible({
    timeout: 5000,
  });
}

test('Phase 2c: typing @src in the chat input opens the autocomplete and selecting inserts the path', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  // The chat input is part of the workspace view by default — switch
  // to the Chat tab to ensure it is foregrounded.
  await page.getByTestId('tab-chat').click();

  const textarea = page.getByLabel('Message');
  await expect(textarea).toBeVisible({ timeout: 5000 });
  await textarea.focus();
  await textarea.fill('@src');

  // Autocomplete should mount and at least one row contains "src/".
  await expect(page.getByTestId('mention-autocomplete')).toBeVisible({ timeout: 5000 });
  const rows = page.getByTestId('mention-row');
  await expect(rows.first()).toBeVisible();

  // Press Enter to confirm the highlighted suggestion.
  await textarea.press('Enter');

  // Textarea value should now be the full path + trailing space.
  await expect(textarea).toHaveValue(/^@src\/[a-z./]+ $/);
  // Autocomplete dismisses once the mention is no longer at the caret.
  await expect(page.getByTestId('mention-autocomplete')).toHaveCount(0);
});

test('Phase 2c: pressing Escape dismisses the autocomplete without inserting', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  await page.getByTestId('tab-chat').click();
  const textarea = page.getByLabel('Message');
  await textarea.focus();
  await textarea.fill('@s');

  await expect(page.getByTestId('mention-autocomplete')).toBeVisible({ timeout: 5000 });
  await textarea.press('Escape');

  await expect(page.getByTestId('mention-autocomplete')).toHaveCount(0);
  // Value is unchanged (no path inserted).
  await expect(textarea).toHaveValue('@s');
});

test('Phase 2c: ArrowDown moves highlight to the next row', async ({ page, harness }) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  await page.getByTestId('tab-chat').click();
  const textarea = page.getByLabel('Message');
  await textarea.focus();
  await textarea.fill('@');

  await expect(page.getByTestId('mention-autocomplete')).toBeVisible({ timeout: 5000 });
  const rows = page.getByTestId('mention-row');
  // Before keypress, first row is highlighted.
  await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true');
  await textarea.press('ArrowDown');
  await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true');
});
