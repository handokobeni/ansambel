// tests/e2e/phase-3a-3-publisher/publisher-roundtrip.spec.ts
//
// Phase 3a-3 — workspace state publisher round-trip smoke.
//
// Architecture notes (why no real Tauri binary, no real Bitable POST):
//   The harness runs Vite dev at localhost:1420 with no Tauri binary;
//   __TAURI_INTERNALS__ is installed via page.addInitScript()
//   (see tests/e2e/helpers/tauri-shim.ts).
//
//   This spec layers a stateful publisher-domain mock on top of the base
//   shim that:
//     - serves get_lark_status (so LarkGlobalSettings hydrates without
//       Connected/Disconnected churn)
//     - serves get_team_activity_config / set_team_activity_config
//       against an in-page record (so the form save round-trips)
//     - serves setup_team_activity_table returning 12 schema columns
//     - serves set_workspace_team_activity_private and records each call
//       so the toggle assertions can read the captured arguments back out
//
//   The Bitable HTTP POST is NOT exercised here; that path is covered by
//   Rust integration tests with MockServer (Task 5 + Task 6 + Task 7).
//   The user-facing trigger we care about is the IPC command boundary.
//
// Gated by ANSAMBEL_LARK_FIXTURE=1 — the spec doesn't talk to Lark, but it
// needs the dev-server harness which other suites skip by default.

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import * as os from 'node:os';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';

const HAS_FIXTURE_FLAG = process.env.ANSAMBEL_LARK_FIXTURE === '1';

// ── Fixtures ─────────────────────────────────────────────────────────────────

const REPO_ID = 'repo_e2e_publisher';
const WORKSPACE_ID = 'ws_e2e_publisher';
const APP_TOKEN = 'bascnPublisherE2E';
const TABLE_ID = 'tblPublisherE2E';
const MACHINE_LABEL = 'tester@e2e-host';

const TEAM_ACTIVITY_COLUMNS = [
  'workspace_id',
  'repo_remote_url',
  'repo_display_name',
  'task_title',
  'assignee_machine',
  'ansambel_status',
  'last_activity_at',
  'last_message_preview',
  'branch_name',
  'diff_summary',
  'pr_url',
  'private',
];

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-publisher-'));
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

// ── Spec ─────────────────────────────────────────────────────────────────────

test.describe('Phase 3a-3 publisher — round-trip smoke', () => {
  test.skip(!HAS_FIXTURE_FLAG, 'requires ANSAMBEL_LARK_FIXTURE=1');
  test.describe.configure({ mode: 'serial' });

  // ── Test 1: settings-save happy path ────────────────────────────────────
  test('team activity config save flow toasts success', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      dialogOpenPath: FIXTURE_REPO_PATH,
      initialRepos: [
        {
          id: REPO_ID,
          name: 'publisher-e2e-repo',
          path: FIXTURE_REPO_PATH,
          gh_profile: null,
          default_branch: 'main',
          created_at: 1_700_000_000,
          updated_at: 1_700_000_000,
        },
      ],
      initialWorkspaces: [],
      initialTasks: [],
    });

    // Layer the publisher-domain mock. Runs after the base shim because
    // addInitScripts execute in insertion order.
    await page.addInitScript(
      ({ columns }: { columns: string[] }) => {
        const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
          invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
        };
        const originalInvoke = internals.invoke.bind(internals);

        // Publisher-domain in-page state. `setCalls` records every
        // set_team_activity_config payload so the assertions can verify
        // the form actually round-tripped through the IPC layer.
        const teamActivityState = {
          config: null as null | { app_token: string; table_id: string; machine_label: string },
          setCalls: [] as Array<{ app_token: string; table_id: string; machine_label: string }>,
          setupCalls: [] as Array<{ appToken: string; tableId: string }>,
        };
        // Expose for the test runner to read after interactions.
        (window as unknown as Record<string, unknown>)['__TEAM_ACTIVITY_STATE__'] =
          teamActivityState;

        internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
          switch (cmd) {
            // ── Lark global creds (so LarkGlobalSettings doesn't error) ──
            case 'get_lark_status':
              return {
                app_id: '',
                base_url: 'https://open.larksuite.com',
                has_secret: false,
                last_token_refresh_at: null,
              };

            // ── Team activity config ─────────────────────────────────────
            case 'get_team_activity_config':
              return teamActivityState.config;

            case 'set_team_activity_config': {
              const cfg = args.config as {
                app_token: string;
                table_id: string;
                machine_label: string;
              };
              teamActivityState.setCalls.push({ ...cfg });
              // Match backend semantics: empty token clears the config.
              teamActivityState.config = cfg.app_token.length === 0 ? null : { ...cfg };
              return undefined;
            }

            case 'setup_team_activity_table': {
              teamActivityState.setupCalls.push({
                appToken: args.appToken as string,
                tableId: args.tableId as string,
              });
              return columns;
            }

            default:
              return originalInvoke(cmd, args);
          }
        };
      },
      { columns: TEAM_ACTIVITY_COLUMNS }
    );

    await page.goto('/');

    // Open the Settings dialog from the TitleBar gear button.
    await page.getByTestId('open-settings').click();

    // TeamActivitySettings mounts below LarkSettings; wait for it.
    await expect(page.getByTestId('team-activity-settings')).toBeVisible({ timeout: 10_000 });
    // Loading completes; status starts at "Not configured".
    await expect(page.getByTestId('team-activity-status')).toContainText(/not configured/i, {
      timeout: 5_000,
    });

    // Fill the 3-field form.
    await page.getByTestId('team-activity-app-token').fill(APP_TOKEN);
    await page.getByTestId('team-activity-table-id').fill(TABLE_ID);
    await page.getByTestId('team-activity-machine-label').fill(MACHINE_LABEL);

    // Save.
    await page.getByTestId('team-activity-save').click();

    // Toast appears (the toast container has role="status").
    await expect(page.locator('.toast')).toContainText(/team activity configured/i, {
      timeout: 5_000,
    });

    // Status flips to Active.
    await expect(page.getByTestId('team-activity-status')).toContainText(/active/i, {
      timeout: 5_000,
    });

    // Verify the IPC call captured the flat-shape payload (Task 19 note:
    // the previous phase tripped on a stale binding shape — guard against
    // it here by asserting on app_token / table_id / machine_label keys).
    const captured = await page.evaluate(
      () =>
        (
          window as unknown as {
            __TEAM_ACTIVITY_STATE__: {
              setCalls: Array<{ app_token: string; table_id: string; machine_label: string }>;
            };
          }
        ).__TEAM_ACTIVITY_STATE__.setCalls
    );
    expect(captured).toHaveLength(1);
    expect(captured[0]).toEqual({
      app_token: APP_TOKEN,
      table_id: TABLE_ID,
      machine_label: MACHINE_LABEL,
    });
  });

  // ── Test 2: per-workspace privacy toggle round-trip ─────────────────────
  test('per-workspace privacy toggle round-trips through IPC', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      dialogOpenPath: FIXTURE_REPO_PATH,
      initialRepos: [
        {
          id: REPO_ID,
          name: 'publisher-e2e-repo',
          path: FIXTURE_REPO_PATH,
          gh_profile: null,
          default_branch: 'main',
          created_at: 1_700_000_000,
          updated_at: 1_700_000_000,
        },
      ],
      initialWorkspaces: [
        {
          id: WORKSPACE_ID,
          repo_id: REPO_ID,
          branch: 'feat/publisher-e2e',
          base_branch: 'main',
          custom_branch: false,
          title: 'publisher e2e workspace',
          description: '',
          status: 'waiting',
          column: 'in_progress',
          created_at: 1_700_000_000,
          updated_at: 1_700_000_000,
          worktree_dir: `/mock/worktrees/${WORKSPACE_ID}`,
          task_id: null,
        },
      ],
      initialTasks: [],
    });

    await page.addInitScript(
      ({ workspaceId }: { workspaceId: string }) => {
        const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
          invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
        };
        const originalInvoke = internals.invoke.bind(internals);

        const privacyState = {
          // Track every privacy toggle so the test can assert both clicks
          // actually crossed the IPC boundary with the expected flag.
          calls: [] as Array<{ workspaceId: string; isPrivate: boolean }>,
        };
        (window as unknown as Record<string, unknown>)['__PRIVACY_STATE__'] = privacyState;

        internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
          switch (cmd) {
            case 'get_lark_status':
              return {
                app_id: '',
                base_url: 'https://open.larksuite.com',
                has_secret: false,
                last_token_refresh_at: null,
              };

            case 'set_workspace_team_activity_private': {
              privacyState.calls.push({
                workspaceId: args.workspaceId as string,
                isPrivate: args.isPrivate as boolean,
              });
              return undefined;
            }

            // Ensure the seeded workspace shows up — the base shim already
            // returns initialWorkspaces, but list_workspaces with a repoId
            // filter is what App.svelte calls on hydrate.
            case 'list_workspaces':
              return originalInvoke(cmd, args);

            default:
              return originalInvoke(cmd, args);
          }
        };
        void workspaceId; // captured for the closure, no runtime use yet
      },
      { workspaceId: WORKSPACE_ID }
    );

    await page.goto('/');

    // Hydrate: repo is auto-selected, workspaces loaded. The sidebar uses
    // `data-workspace-id`. Click the row to select; then flip to Work mode
    // so WorkspaceView mounts (Plan mode renders the kanban instead).
    const workspaceRow = page.locator(`[data-workspace-id="${WORKSPACE_ID}"]`);
    await expect(workspaceRow).toBeVisible({ timeout: 10_000 });
    await workspaceRow.click();
    await page.getByRole('button', { name: 'Work' }).click();

    // WorkspaceView mounts → privacy toggle is in the header.
    const toggle = page.getByTestId('team-activity-privacy-toggle');
    await expect(toggle).toBeVisible({ timeout: 5_000 });
    // Initial state: public (team_activity_private defaults to false).
    await expect(toggle).toHaveAttribute('data-private', 'false');

    // First click: public → private.
    await toggle.click();
    await expect(toggle).toHaveAttribute('data-private', 'true', { timeout: 3_000 });

    // Second click: private → public.
    await toggle.click();
    await expect(toggle).toHaveAttribute('data-private', 'false', { timeout: 3_000 });

    // Verify both calls reached the IPC layer with the expected flag.
    const calls = await page.evaluate(
      () =>
        (
          window as unknown as {
            __PRIVACY_STATE__: { calls: Array<{ workspaceId: string; isPrivate: boolean }> };
          }
        ).__PRIVACY_STATE__.calls
    );
    expect(calls).toHaveLength(2);
    expect(calls[0]).toEqual({ workspaceId: WORKSPACE_ID, isPrivate: true });
    expect(calls[1]).toEqual({ workspaceId: WORKSPACE_ID, isPrivate: false });
  });
});
