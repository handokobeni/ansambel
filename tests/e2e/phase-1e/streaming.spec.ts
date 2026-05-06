import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-stream-'));
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

test('Streaming: assistant bubble length grows monotonically as partials arrive', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, {
    dialogOpenPath: FIXTURE_REPO_PATH,
    replyProfile: 'streaming',
  });
  await page.goto('/');

  // Add the repo and create a task wired to a workspace, mirroring chat-flow.
  await page.waitForSelector('header', { timeout: 10000 });
  await page.getByRole('button', { name: /add repo/i }).click();
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });

  await page.getByRole('button', { name: /add task/i }).click();
  await page.getByLabel(/title/i).fill('E2E streaming task');
  await page.getByLabel(/description/i).fill('Verify streaming cadence');
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
    const task = tasks.find((t) => t.title === 'E2E streaming task');
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
  await sidebar.getByText('E2E streaming task').click();

  await page.getByRole('button', { name: /^work$/i }).click();
  await expect(page.getByRole('heading', { name: 'E2E streaming task' })).toBeVisible({
    timeout: 5000,
  });
  await expect(page.getByText(/running/i)).toBeVisible({ timeout: 5000 });

  // Send the message that triggers the streaming reply profile.
  const textarea = page.getByLabel(/message/i);
  await textarea.fill('hello claude');
  await page.getByRole('button', { name: /^send$/i }).click();

  // The streaming profile emits 4 partial deltas at 30, 60, 90, 120 ms with a
  // final non-partial at 150 ms — a single same-id assistant bubble whose text
  // grows on each delta. Identify it as the assistant bubble that is NOT the
  // user echo bubble.
  const assistantBubble = page
    .locator('article[data-role="assistant"][data-message-id^="msg_stream_"]')
    .first();

  // Wait until the bubble appears at all — the very first partial creates it.
  await expect(assistantBubble).toBeVisible({ timeout: 5000 });

  // Capture growth snapshots while the reply streams. We poll faster than the
  // 30 ms emit cadence so each delta is observed at least once.
  const lengths: number[] = [];
  for (let i = 0; i < 12; i++) {
    const text =
      (await assistantBubble
        .locator('p')
        .first()
        .innerText()
        .catch(() => '')) ?? '';
    if (text.length > 0) lengths.push(text.length);
    await page.waitForTimeout(20);
  }

  // The bubble must end on the final, non-partial form. Wait for it before
  // asserting on growth so we know the stream completed within the budget.
  await expect(assistantBubble).toHaveAttribute('data-role', 'assistant');
  await expect(assistantBubble.locator('p').first()).toHaveText(
    'Streaming reply to: hello claude',
    {
      timeout: 5000,
    }
  );

  // The streaming indicator (▍) must disappear once the final non-partial
  // arrives, signalling the bubble has closed.
  await expect(assistantBubble.locator('[aria-label="streaming"]')).toHaveCount(0);

  // Growth must be monotonic — never regresses. We only check non-empty
  // snapshots; the very first poll might land before the first delta.
  expect(lengths.length).toBeGreaterThan(0);
  for (let i = 1; i < lengths.length; i++) {
    expect(lengths[i]).toBeGreaterThanOrEqual(lengths[i - 1]);
  }

  // We must observe at least 2 distinct growth points — proves it actually
  // streamed rather than appearing in one shot.
  const distinct = new Set(lengths);
  expect(distinct.size).toBeGreaterThanOrEqual(2);
});
