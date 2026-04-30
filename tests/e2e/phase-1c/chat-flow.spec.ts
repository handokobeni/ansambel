import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-chat-'));
  FIXTURE_REPO_PATH = tmpDir;
  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 't@e.com'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.name', 'T'], { cwd: tmpDir });
  execFileSync('git', ['commit', '--allow-empty', '-m', 'init'], {
    cwd: tmpDir,
  });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

test('Chat: drag task → switch to work mode → send message → see reply', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');

  // Add the repo
  await page.waitForSelector('header', { timeout: 10000 });
  await page.getByRole('button', { name: /add repo/i }).click();
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });

  // Add task in Plan mode
  await page.getByRole('button', { name: /add task/i }).click();
  await page.getByLabel(/title/i).fill('E2E chat task');
  await page.getByLabel(/description/i).fill('Test the chat flow');
  await page.getByRole('button', { name: /^add task$/i }).click();

  // Synthetic finalize → drag Todo → In Progress (auto-create workspace)
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
    const task = tasks.find((t) => t.title === 'E2E chat task');
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

  // Click the auto-created workspace in the sidebar
  const sidebar = page.locator('aside').first();
  await sidebar.getByText('E2E chat task').click();

  // Switch to Work mode
  await page.getByRole('button', { name: /^work$/i }).click();

  // WorkspaceView should be visible (header has title + branch)
  await expect(page.getByRole('heading', { name: 'E2E chat task' })).toBeVisible({
    timeout: 5000,
  });

  // Status pill should say "Running" after spawn (mock emits status:running)
  await expect(page.getByText(/running/i)).toBeVisible({ timeout: 5000 });

  // Send a message
  const textarea = page.getByLabel(/message/i);
  await textarea.fill('hello claude');
  await page.getByRole('button', { name: /^send$/i }).click();

  // Mock replies with "Mock reply to: hello claude" after 30ms
  await expect(page.getByText('Mock reply to: hello claude')).toBeVisible({
    timeout: 5000,
  });
});
