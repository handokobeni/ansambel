// tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts
//
// Phase 3a-4 — Team Activity sidebar + mirror view E2E golden path.
//
// Architecture notes:
//   - No real Tauri binary; __TAURI_INTERNALS__ installed via tauri-shim.
//   - Base shim handles repos.load(), workspaces.load(), tasks.load() fired
//     by App.svelte on mount.
//   - A second page.addInitScript (runs after the shim in insertion order)
//     wraps internals.invoke and intercepts fetch_team_activity_rows,
//     delegating everything else to originalInvoke.
//   - Test 4 uses window.__TEAM_ACTIVITY_SIDEBAR_STATE__ to flip the response
//     at runtime (same pattern as publisher-roundtrip.spec.ts).
//
// Gated by ANSAMBEL_LARK_FIXTURE=1.

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';

const FIXTURE_GATE = process.env.ANSAMBEL_LARK_FIXTURE === '1';

const REPO_ID = 'repo_e2e_team_activity';
const REPO_PATH = '/tmp/fixture-team-activity-repo';

const BASE_REPO = {
  id: REPO_ID,
  name: 'fixture-team-activity-repo',
  path: REPO_PATH,
  gh_profile: null,
  default_branch: 'main',
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
};

test.describe('Phase 3a-4: Team Activity sidebar + mirror view', () => {
  test.skip(!FIXTURE_GATE, 'set ANSAMBEL_LARK_FIXTURE=1 to run');
  test.describe.configure({ mode: 'serial' });

  test('sidebar shows team rows for overlapping repos', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      initialRepos: [BASE_REPO],
      initialWorkspaces: [],
      initialTasks: [],
    });

    await page.addInitScript(() => {
      const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
        invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
      };
      const originalInvoke = internals.invoke.bind(internals);
      internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'fetch_team_activity_rows') {
          return {
            kind: 'rows',
            rows: [
              {
                workspace_id: 'ws_remote_a',
                repo_remote_url: 'https://github.com/foo/bar',
                repo_display_name: 'bar',
                task_title: 'Refactor auth',
                assignee_machine: 'bob@laptop',
                ansambel_status: 'running',
                last_activity_at: Date.now() - 5 * 60 * 1000,
                last_message_preview: 'doing the thing',
                branch_name: 'feat/auth',
                diff_summary: '',
                pr_url: '',
                private: false,
              },
            ],
          };
        }
        return originalInvoke(cmd, args);
      };
    });

    await page.goto('/');

    await expect(page.getByTestId('team-activity-panel')).toBeVisible();
    await expect(
      page.locator('[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]')
    ).toBeVisible();
  });

  test('clicking a row opens the mirror view with GitHub branch link', async ({
    page,
    harness,
  }) => {
    void harness;

    await installTauriShim(page, {
      initialRepos: [BASE_REPO],
      initialWorkspaces: [],
      initialTasks: [],
    });

    await page.addInitScript(() => {
      const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
        invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
      };
      const originalInvoke = internals.invoke.bind(internals);
      internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'fetch_team_activity_rows') {
          return {
            kind: 'rows',
            rows: [
              {
                workspace_id: 'ws_remote_a',
                repo_remote_url: 'https://github.com/foo/bar',
                repo_display_name: 'bar',
                task_title: 'Refactor auth',
                assignee_machine: 'bob@laptop',
                ansambel_status: 'running',
                last_activity_at: Date.now() - 5 * 60 * 1000,
                last_message_preview: 'doing the thing',
                branch_name: 'feat/auth',
                diff_summary: '',
                pr_url: '',
                private: false,
              },
            ],
          };
        }
        return originalInvoke(cmd, args);
      };
    });

    await page.goto('/');

    await page
      .locator('[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]')
      .click();

    await expect(page.getByTestId('team-workspace-mirror')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Open branch on GitHub' })).toHaveAttribute(
      'href',
      'https://github.com/foo/bar/tree/feat%2Fauth'
    );
  });

  test('back button returns to the prior main view', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      initialRepos: [BASE_REPO],
      initialWorkspaces: [],
      initialTasks: [],
    });

    await page.addInitScript(() => {
      const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
        invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
      };
      const originalInvoke = internals.invoke.bind(internals);
      internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'fetch_team_activity_rows') {
          return {
            kind: 'rows',
            rows: [
              {
                workspace_id: 'ws_remote_a',
                repo_remote_url: 'https://github.com/foo/bar',
                repo_display_name: 'bar',
                task_title: 'Refactor auth',
                assignee_machine: 'bob@laptop',
                ansambel_status: 'running',
                last_activity_at: Date.now() - 5 * 60 * 1000,
                last_message_preview: 'doing the thing',
                branch_name: 'feat/auth',
                diff_summary: '',
                pr_url: '',
                private: false,
              },
            ],
          };
        }
        return originalInvoke(cmd, args);
      };
    });

    await page.goto('/');

    await page
      .locator('[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]')
      .click();
    await expect(page.getByTestId('team-workspace-mirror')).toBeVisible();

    await page
      .getByTestId('team-workspace-mirror')
      .getByRole('button', { name: 'back to workspace' })
      .click();

    await expect(page.getByTestId('team-workspace-mirror')).not.toBeVisible();
  });

  test('mirror auto-closes when row disappears from polled response', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      initialRepos: [BASE_REPO],
      initialWorkspaces: [],
      initialTasks: [],
    });

    // Install invoke override with a mutable flag that the test body can flip.
    await page.addInitScript(() => {
      // Mutable state accessible from page.evaluate().
      (window as unknown as Record<string, unknown>)['__TEAM_ACTIVITY_SIDEBAR_STATE__'] = {
        serveRows: true,
      };

      const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
        invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
      };
      const originalInvoke = internals.invoke.bind(internals);
      internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'fetch_team_activity_rows') {
          const state = (window as unknown as Record<string, unknown>)[
            '__TEAM_ACTIVITY_SIDEBAR_STATE__'
          ] as { serveRows: boolean };
          if (state.serveRows) {
            return {
              kind: 'rows',
              rows: [
                {
                  workspace_id: 'ws_remote_a',
                  repo_remote_url: 'https://github.com/foo/bar',
                  repo_display_name: 'bar',
                  task_title: 'Refactor auth',
                  assignee_machine: 'bob@laptop',
                  ansambel_status: 'running',
                  last_activity_at: Date.now() - 5 * 60 * 1000,
                  last_message_preview: 'doing the thing',
                  branch_name: 'feat/auth',
                  diff_summary: '',
                  pr_url: '',
                  private: false,
                },
              ],
            };
          }
          return { kind: 'rows', rows: [] };
        }
        return originalInvoke(cmd, args);
      };
    });

    await page.goto('/');

    // Open the mirror view.
    await page
      .locator('[data-testid="team-activity-row"][data-workspace-id="ws_remote_a"]')
      .click();
    await expect(page.getByTestId('team-workspace-mirror')).toBeVisible();

    // Flip the server response so the next poll returns an empty list.
    await page.evaluate(() => {
      const state = (window as unknown as Record<string, unknown>)[
        '__TEAM_ACTIVITY_SIDEBAR_STATE__'
      ] as { serveRows: boolean };
      state.serveRows = false;
    });

    // The poll cycle is 10 s; wait 11 s for the next tick to fire and
    // reconcile() to drop the row + auto-clear selectedWorkspaceId.
    await expect(page.getByTestId('team-workspace-mirror')).not.toBeVisible({
      timeout: 15_000,
    });
  });

  test('panel renders disabled state when config is absent', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      initialRepos: [BASE_REPO],
      initialWorkspaces: [],
      initialTasks: [],
    });

    await page.addInitScript(() => {
      const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
        invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
      };
      const originalInvoke = internals.invoke.bind(internals);
      internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'fetch_team_activity_rows') {
          return { kind: 'disabled' };
        }
        return originalInvoke(cmd, args);
      };
    });

    await page.goto('/');

    await expect(page.getByText('Configure Team Activity in Settings')).toBeVisible();
  });
});
