// tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts
//
// Phase 3c — Terminal multi-tab E2E golden path.
//
// Architecture notes:
//   - No real Tauri binary; __TAURI_INTERNALS__ installed via tauri-shim.
//   - Base shim handles repos.load(), workspaces.load(), tasks.load() fired
//     by App.svelte on mount.
//   - A second page.addInitScript (runs after the shim in insertion order)
//     wraps internals.invoke and intercepts terminal_spawn / terminal_reattach /
//     terminal_kill to track live terminal ids; all other commands delegate to
//     the base shim.
//   - Navigation: the shim seeds one repo + one workspace in `in_progress`.
//     App.svelte auto-selects the first repo on mount → workspaces populate →
//     clicking the workspace row in the sidebar selects it, then clicking
//     "Work" enters Work mode → WorkspaceView renders → clicking tab-terminal
//     opens the Terminal pane.

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';

test('a workspace can open multiple terminal tabs that persist across switching', async ({
  page,
  harness,
}) => {
  void harness;

  await installTauriShim(page, {
    initialRepos: [
      {
        id: 'repo_e2e',
        name: 'term-repo',
        path: '/tmp/term-repo',
        gh_profile: null,
        default_branch: 'main',
        created_at: 1700000000,
        updated_at: 1700000000,
      },
    ],
    initialWorkspaces: [
      {
        id: 'ws_e2e',
        repo_id: 'repo_e2e',
        branch: 'main',
        base_branch: 'main',
        custom_branch: false,
        title: 'T',
        description: '',
        status: 'waiting',
        column: 'in_progress',
        created_at: 1,
        updated_at: 1,
        worktree_dir: '/tmp/term-repo',
        team_activity_private: false,
        task_id: null,
      },
    ],
    initialTasks: [],
  });

  // Layer a second init script (runs after the base shim) to intercept
  // terminal IPC and record live terminal ids.
  await page.addInitScript(() => {
    const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const original = internals.invoke.bind(internals);
    const live = new Set<string>();
    (window as unknown as Record<string, unknown>)['__TERMS__'] = () => live.size;
    internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === 'terminal_spawn') {
        live.add(args['terminalId'] as string);
        // Delegate to the base shim so the channel still gets a byte nudge.
        return original(cmd, args);
      }
      if (cmd === 'terminal_reattach') {
        return live.has(args['terminalId'] as string) ? undefined : Promise.reject('no session');
      }
      if (cmd === 'terminal_kill') {
        live.delete(args['terminalId'] as string);
        return undefined;
      }
      if (cmd === 'terminal_write' || cmd === 'terminal_resize') return undefined;
      // App.svelte's larkBindings.load() calls list_lark_repo_bindings; the
      // base shim returns undefined for unknown commands which causes
      // Object.entries(undefined) to throw and aborts the auto-select logic.
      // Return an empty record so the bindings store loads cleanly.
      if (cmd === 'list_lark_repo_bindings') return {};
      // Similarly, fetch_team_activity_rows must return a valid shape or the
      // TeamActivityPanel will throw; mirror the disabled case.
      if (cmd === 'fetch_team_activity_rows') return { kind: 'disabled' };
      return original(cmd, args);
    };
  });

  await page.goto('/');

  // Wait for the sidebar to hydrate with the seeded workspace. The shim's
  // auto-select logic in App.svelte selects the first repo and loads its
  // workspaces, so the workspace row should appear shortly after mount.
  await expect(page.locator('[data-workspace-id="ws_e2e"]')).toBeVisible({ timeout: 10_000 });

  // Select the workspace (clicks the row in the sidebar).
  await page.locator('[data-workspace-id="ws_e2e"]').click();

  // Switch to Work mode (the TitleBar "Work" button).
  await page.getByRole('button', { name: /^work$/i }).click();

  // Click the Terminal tab in the workspace tab strip.
  await page.getByTestId('tab-terminal').click();
  await expect(page.getByTestId('tab-terminal')).toHaveAttribute('aria-selected', 'true');

  // Terminal.svelte auto-adds the first tab on first open.  The TerminalPane
  // then tries reattach (fails → no session) and falls back to spawn, which
  // our override above intercepts and adds to `live`.
  await expect
    .poll(
      () => page.evaluate(() => (window as unknown as Record<string, () => number>)['__TERMS__']()),
      { timeout: 10_000 }
    )
    .toBeGreaterThanOrEqual(1);

  // One pane host mounted (kept alive).
  await expect(page.getByTestId('terminal-pane-host')).toHaveCount(1, { timeout: 5_000 });

  // Add a second terminal via the "+" button.
  await page.getByTestId('terminal-tab-add').click();

  // Now two live PTYs: the newly added pane also spawns.
  await expect
    .poll(
      () => page.evaluate(() => (window as unknown as Record<string, () => number>)['__TERMS__']()),
      { timeout: 10_000 }
    )
    .toBe(2);

  // Both pane hosts mounted and kept alive (display-toggled, not removed).
  await expect(page.getByTestId('terminal-pane-host')).toHaveCount(2, { timeout: 5_000 });
});
