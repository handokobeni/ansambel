// tests/e2e/phase-1a/add-repo.spec.ts
import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

// Path to fixture repo — created in beforeAll
let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  // Create a real git repo in a temp dir so the backend mock can derive a name
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-mock-repo-'));
  FIXTURE_REPO_PATH = tmpDir;

  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 'test@example.com'], {
    cwd: tmpDir,
  });
  execFileSync('git', ['config', 'user.name', 'Test'], { cwd: tmpDir });
  // At least one commit for a valid repo with a default branch
  execFileSync('git', ['commit', '--allow-empty', '-m', 'initial'], {
    cwd: tmpDir,
  });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

test('Add Repo: clicking Add Repo opens dialog, backend adds repo, TitleBar shows repo name', async ({
  page,
  harness,
}) => {
  void harness; // ensures the worker-scoped harness starts before this spec runs

  // Install the Tauri IPC shim before the page loads — dialog returns fixture path
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });

  await page.goto('/');

  // Wait for app to be ready — TitleBar header must be present
  await page.waitForSelector('header', { timeout: 10000 });

  // Initially shows "No repo selected"
  await expect(page.getByText('No repo selected')).toBeVisible({ timeout: 5000 });

  // Click the Add Repo button
  const addBtn = page.getByRole('button', { name: /add repo/i });
  await expect(addBtn).toBeVisible();
  await addBtn.click();

  // After dialog mock resolves and backend processes, TitleBar shows repo name
  // The repo name is derived from the folder name of the fixture path
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 5000 });

  // "No repo selected" should no longer be visible
  await expect(page.getByText('No repo selected')).not.toBeVisible();
});
