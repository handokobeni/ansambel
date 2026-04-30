import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-tools-'));
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

test('Chat shows formatted tool calls and a compact-boundary marker in one turn', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, {
    dialogOpenPath: FIXTURE_REPO_PATH,
    replyProfile: 'tools',
  });
  await page.goto('/');

  await page.waitForSelector('header', { timeout: 10000 });
  await page.getByRole('button', { name: /add repo/i }).click();
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });

  await page.getByRole('button', { name: /add task/i }).click();
  await page.getByLabel(/title/i).fill('E2E tool rendering task');
  await page.getByLabel(/description/i).fill('Exercise tool + compact rendering');
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
    const task = tasks.find((t) => t.title === 'E2E tool rendering task');
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
  await sidebar.getByText('E2E tool rendering task').click();

  await page.getByRole('button', { name: /^work$/i }).click();
  await expect(page.getByRole('heading', { name: 'E2E tool rendering task' })).toBeVisible({
    timeout: 5000,
  });
  await expect(page.getByText(/running/i)).toBeVisible({ timeout: 5000 });

  // Trigger the 'tools' reply profile.
  const textarea = page.getByLabel(/message/i);
  await textarea.fill('please read foo and run ls');
  await page.getByRole('button', { name: /^send$/i }).click();

  // Read tool — formatter must show "Read" + "foo.ts:1-50".
  const readBubble = page.locator('[data-tool-name="Read"]').first();
  await expect(readBubble).toBeVisible({ timeout: 5000 });
  await expect(readBubble).toContainText('Read');
  await expect(readBubble).toContainText('foo.ts:1-50');

  // Bash tool — formatter shows "Bash" + "$ ls -la".
  const bashBubble = page.locator('[data-tool-name="Bash"]').first();
  await expect(bashBubble).toBeVisible({ timeout: 5000 });
  await expect(bashBubble).toContainText('Bash');
  await expect(bashBubble).toContainText('$ ls -la');

  // Tool result preview rendered.
  const toolResult = page.locator('[data-tool-result]').first();
  await expect(toolResult).toBeVisible({ timeout: 5000 });
  await expect(toolResult).toContainText('127.0.0.1 localhost');

  // Compact marker — system role bubble with token annotation.
  const compactMarker = page.locator('[data-role="system"]').first();
  await expect(compactMarker).toBeVisible({ timeout: 5000 });
  await expect(compactMarker).toContainText(/compact/i);
  await expect(compactMarker).toContainText(/45k/);

  // Final assistant text is also there — confirms the turn closed cleanly
  // after the tool/compact intermissions.
  await expect(page.getByText('Tool turn reply to: please read foo and run ls')).toBeVisible({
    timeout: 5000,
  });
});
