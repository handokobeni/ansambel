// tests/e2e/phase-1a/workspace-lifecycle.spec.ts
import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

// ---------------------------------------------------------------------------
// Fixture setup (shared across tests in this file)
// ---------------------------------------------------------------------------

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-ws-lifecycle-'));
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

// ---------------------------------------------------------------------------
// Helper
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

test('Workspace lifecycle: create then remove', async ({ page, harness }) => {
  void harness; // ensures the worker-scoped harness starts before this spec runs

  // Install full Tauri IPC shim with dialog returning fixture path
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });

  await page.goto('/');

  // Step 1: Add repo (uses the dialog shim installed above)
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

  // Step 6: Remove workspace — mock window.confirm to return true automatically
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
