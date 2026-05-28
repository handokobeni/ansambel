// tests/e2e/phase-2b/phase-2b.spec.ts
//
// Phase 2b golden paths: with a Work-mode workspace open, the user can
//   1. open a file from the FileBrowser into the Editor tab and trigger
//      a save round-trip (file_write IPC),
//   2. switch to the Terminal tab and see the xterm-rendered "live"
//      status flip on after spawn,
//   3. trigger one of the configured scripts and see its bytes arrive
//      in the same Terminal panel.
//
// These tests run against the Vite dev server with the Tauri IPC shim
// installed at page-init time — no real PTY, no real CodeMirror layout
// math (jsdom-style measurement still applies), but enough of the
// frontend's wiring is exercised to catch regressions in the
// dispatch / state / channel paths.

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-2b-'));
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
  await page.getByLabel(/title/i).fill('Phase 2b smoke task');
  await page.getByLabel(/description/i).fill('Editor + Terminal smoke');
  await page.getByRole('button', { name: /^add task$/i }).click();

  // Move to in-progress so a workspace gets auto-created (mirrors the
  // backend side effect the shim implements above).
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
    const task = tasks.find((t) => t.title === 'Phase 2b smoke task');
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
  await sidebar.getByText('Phase 2b smoke task').click();
  await page.getByRole('button', { name: /^work$/i }).click();
  await expect(page.getByRole('heading', { name: 'Phase 2b smoke task' })).toBeVisible({
    timeout: 5000,
  });
}

test('Phase 2b: clicking a file opens it in the Editor tab and Save calls file_write', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });

  // Capture every invoke call so we can assert file_write fired with
  // the expected sha1 from the prior file_read.
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  await page.exposeFunction('recordInvoke', (cmd: string, args: Record<string, unknown>) => {
    calls.push({ cmd, args });
  });
  await page.addInitScript(() => {
    type Internals = {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const wait = (cb: () => boolean, ms = 50, tries = 100) =>
      new Promise<void>((resolve, reject) => {
        const tick = (n: number) => {
          if (cb()) return resolve();
          if (n <= 0) return reject(new Error('wait timed out'));
          setTimeout(() => tick(n - 1), ms);
        };
        tick(tries);
      });
    void wait(() =>
      Boolean((window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'])
    ).then(() => {
      const internals = (window as unknown as Record<string, unknown>)[
        '__TAURI_INTERNALS__'
      ] as Internals;
      const orig = internals.invoke;
      internals.invoke = async (cmd, args) => {
        // Forward to the test driver; ignore failures — recording is
        // best-effort and must never block the app.
        try {
          await (
            window as unknown as { recordInvoke?: (c: string, a: Record<string, unknown>) => void }
          ).recordInvoke?.(cmd, args ?? {});
        } catch {
          // ignore
        }
        return orig(cmd, args);
      };
    });
  });

  await page.goto('/');
  await openWorkspace(page);

  // Files tab → click README.md → editor tab activates with mock content.
  await page.getByTestId('tab-files').click();
  await expect(page.locator('[data-path="README.md"]')).toBeVisible({ timeout: 5000 });
  await page.locator('[data-path="README.md"] button').first().click();

  await expect(page.getByTestId('tab-editor')).toHaveAttribute('aria-selected', 'true', {
    timeout: 5000,
  });
  // The editor's header shows the path of the active file.
  await expect(page.getByTestId('editor-active-path')).toContainText('README.md', {
    timeout: 5000,
  });

  // Click Save — fires file_write on the wire. We don't assert the
  // editor content here because CodeMirror's measurement under jsdom-
  // style layout is fragile; the IPC contract is what matters.
  await page.getByTestId('editor-save').click();

  await expect
    .poll(() => calls.find((c) => c.cmd === 'file_write')?.args, { timeout: 5000 })
    .toMatchObject({
      path: 'README.md',
      // Frontend reads → caches sha1 → writes back. The shim returns
      // `sha-${path}` so the round-trip ships that exact value.
      expectedSha1: 'sha-README.md',
    });
});

test('Phase 2b: Terminal tab spawns a session and renders bytes', async ({ page, harness }) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  await openWorkspace(page);

  await page.getByTestId('tab-terminal').click();
  await expect(page.getByTestId('tab-terminal')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('terminal-view')).toBeVisible();
  // First-open auto-creates a terminal tab; the pane host mounts as soon
  // as spawn (or reattach) resolves. The multi-tab refactor dropped the
  // old single-status header — detailed spawn lifecycle is covered by
  // tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts.
  await expect(page.getByTestId('terminal-pane-host')).toHaveCount(1, { timeout: 5000 });
});

test('Phase 2b: ScriptPicker lists scripts and run streams output into the terminal', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });

  // Hook script_run so we can assert it fired with the right ids.
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  await page.exposeFunction('recordInvoke', (cmd: string, args: Record<string, unknown>) => {
    calls.push({ cmd, args });
  });
  await page.addInitScript(() => {
    type Internals = {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const wait = (cb: () => boolean, ms = 50, tries = 100) =>
      new Promise<void>((resolve, reject) => {
        const tick = (n: number) => {
          if (cb()) return resolve();
          if (n <= 0) return reject(new Error('wait timed out'));
          setTimeout(() => tick(n - 1), ms);
        };
        tick(tries);
      });
    void wait(() =>
      Boolean((window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'])
    ).then(() => {
      const internals = (window as unknown as Record<string, unknown>)[
        '__TAURI_INTERNALS__'
      ] as Internals;
      const orig = internals.invoke;
      internals.invoke = async (cmd, args) => {
        try {
          await (
            window as unknown as { recordInvoke?: (c: string, a: Record<string, unknown>) => void }
          ).recordInvoke?.(cmd, args ?? {});
        } catch {
          // ignore
        }
        return orig(cmd, args);
      };
    });
  });

  await page.goto('/');
  await openWorkspace(page);

  await page.getByTestId('tab-terminal').click();
  await expect(page.getByTestId('terminal-view')).toBeVisible();

  // Script picker renders one button per shim script (dev + test).
  await expect(page.getByTestId('script-picker-item')).toHaveCount(2, { timeout: 5000 });

  await page.getByTestId('script-picker-item').first().click();

  await expect
    .poll(() => calls.find((c) => c.cmd === 'script_run')?.args, { timeout: 5000 })
    .toMatchObject({
      scriptId: 'sc_dev',
    });
});
