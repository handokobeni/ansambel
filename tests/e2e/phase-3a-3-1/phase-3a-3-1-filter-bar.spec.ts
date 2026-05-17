// tests/e2e/phase-3a-3-1/phase-3a-3-1-filter-bar.spec.ts
//
// Phase 3a-3.1 E2E — FilterBar narrows the Kanban via Lark search endpoint.
//
// Architecture notes (why no launchApp/seedLarkBinding helpers):
//   The project runs Vite dev server at localhost:1420 with no real Tauri
//   binary. All Tauri IPC is intercepted by the __TAURI_INTERNALS__ shim
//   installed via page.addInitScript() (see tests/e2e/helpers/tauri-shim.ts).
//
//   This spec extends the base shim with a stateful Lark mock that:
//     - returns a pre-seeded Bitable binding (no wizard interaction needed)
//     - returns Status field metadata for the field picker
//     - tracks filter updates via set_lark_repo_binding
//     - narrows refresh_tasks results based on the stored filter condition
//
//   No real Lark hit, no LARK_* env vars required.

import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import * as os from 'node:os';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';

// ── Fixtures ─────────────────────────────────────────────────────────────────

const REPO_ID = 'repo_e2e_filter';
const APP_TOKEN = 'appE2E';
const TABLE_ID = 'tblE2E';
const FIELD_ID_STATUS = 'fld_status';

const TASK_DONE = {
  id: 'tk_done',
  repo_id: REPO_ID,
  workspace_id: null,
  title: 'Done task',
  description: '',
  column: 'done',
  order: 0,
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
};

const TASK_TODO = {
  id: 'tk_todo',
  repo_id: REPO_ID,
  workspace_id: null,
  title: 'Todo task',
  description: '',
  column: 'todo',
  order: 0,
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
};

const LARK_BINDING = {
  app_token: APP_TOKEN,
  table_id: TABLE_ID,
  filters: { conjunction: 'and', conditions: [] },
  field_mapping: {
    title_field_id: 'fld_title',
    title_field_name: 'Title',
    status_field_id: FIELD_ID_STATUS,
    status_field_name: 'Status',
  },
  status_value_mapping: {
    todo: 'Todo',
    in_progress: 'In Progress',
    review: 'Review',
    done: 'Done',
  },
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
};

const LARK_FIELDS = [
  {
    field_id: FIELD_ID_STATUS,
    field_name: 'Status',
    type: 3,
    is_primary: false,
    property: {
      options: [
        { id: 'o1', name: 'Todo' },
        { id: 'o2', name: 'Done' },
      ],
    },
  },
];

// ── Helpers ───────────────────────────────────────────────────────────────────

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-filter-'));
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

test.describe.configure({ mode: 'serial' });

test.describe('Phase 3a-3.1 — Filter bar narrows kanban', () => {
  // Test 1: Add filter → kanban narrows to Done task only
  test('filter bar adds Status=Done and kanban narrows to one task', async ({ page, harness }) => {
    void harness; // ensure worker-scoped harness starts dev server

    // Install base shim with the two initial tasks.
    await installTauriShim(page, {
      dialogOpenPath: FIXTURE_REPO_PATH,
      initialRepos: [
        {
          id: REPO_ID,
          name: 'filter-e2e-repo',
          path: FIXTURE_REPO_PATH,
          gh_profile: null,
          default_branch: 'main',
          created_at: 1_700_000_000,
          updated_at: 1_700_000_000,
        },
      ],
      initialTasks: [TASK_DONE, TASK_TODO],
    });

    // Layer a Lark extension on top of the base shim.
    // Must run before page.goto() so it is installed during app init.
    await page.addInitScript(
      ({
        repoId,
        binding,
        fields,
        taskDone,
        taskTodo,
      }: {
        repoId: string;
        binding: unknown;
        fields: unknown[];
        taskDone: unknown;
        taskTodo: unknown;
      }) => {
        // State shared across invoke calls inside the page context.
        const larkState: { binding: unknown; filterConditions: unknown[] } = {
          binding,
          filterConditions: [], // grows when set_lark_repo_binding fires
        };

        // Wait until the base shim is installed, then wrap invoke().
        // addInitScript scripts run sequentially in insertion order, so the
        // base shim is guaranteed to be in place when this runs.
        const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
          invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
        };

        const originalInvoke = internals.invoke.bind(internals);

        internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
          switch (cmd) {
            // ── Lark binding commands ──────────────────────────────
            case 'list_lark_repo_bindings':
              return { [repoId]: larkState.binding };

            case 'get_lark_repo_binding':
              return args.repoId === repoId ? larkState.binding : null;

            case 'set_lark_repo_binding': {
              // Capture the updated binding (with filter conditions).
              const updated = args.binding as { filters: { conditions: unknown[] } };
              larkState.binding = args.binding;
              larkState.filterConditions = updated?.filters?.conditions ?? [];
              return undefined;
            }

            case 'delete_lark_repo_binding':
              return undefined;

            // ── Lark field metadata ────────────────────────────────
            case 'list_lark_fields':
              return fields;

            // ── Task refresh (filter-aware) ─────────────────────────
            // Mirrors what the real backend does: when conditions are set,
            // only return tasks whose synthetic "Status" value matches.
            case 'refresh_tasks': {
              const conditions = larkState.filterConditions;
              // If a Status "is" filter is active with value "Done", return only Done task.
              const doneFilter = (
                conditions as Array<{ field_name: string; operator: string; value: string[] }>
              ).find(
                (c) =>
                  c.field_name === 'Status' &&
                  c.operator === 'is' &&
                  c.value.some((v) => typeof v === 'string' && v.toLowerCase().includes('done'))
              );
              if (doneFilter) return [taskDone];
              // No active filter — return both tasks (mirrors list_tasks initial state).
              return [taskDone, taskTodo];
            }

            default:
              return originalInvoke(cmd, args);
          }
        };
      },
      {
        repoId: REPO_ID,
        binding: LARK_BINDING,
        fields: LARK_FIELDS,
        taskDone: TASK_DONE,
        taskTodo: TASK_TODO,
      }
    );

    await page.goto('/');

    // Wait for the app to hydrate: both tasks must be visible initially.
    // The base shim serves initialTasks via list_tasks; the Lark extension
    // overrides list_lark_repo_bindings so FilterBar is mounted.
    await expect(page.getByText('Done task')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText('Todo task')).toBeVisible({ timeout: 5_000 });
    // FilterBar is visible because larkBindings.get(repoId) is truthy.
    await expect(page.getByRole('button', { name: /add filter/i })).toBeVisible({ timeout: 5_000 });

    // ── Step 1: open the field picker ────────────────────────────────────────
    await page.getByRole('button', { name: /add filter/i }).click();

    // The picker calls list_lark_fields; wait for "Status" to appear.
    await expect(page.getByText('Status')).toBeVisible({ timeout: 5_000 });

    // ── Step 2: pick the "Status" field ──────────────────────────────────────
    await page.getByText('Status').click();

    // Operator picker (role="listbox") should appear.
    const operatorPicker = page.getByRole('listbox', { name: /pick operator/i });
    await expect(operatorPicker).toBeVisible({ timeout: 3_000 });

    // ── Step 3: pick the "is" operator ───────────────────────────────────────
    // Use exact:true so "is" does not match "isNot", "isEmpty", "isNotEmpty".
    await operatorPicker.getByRole('option', { name: 'is', exact: true }).click();

    // FilterBar inserts a chip with an editable value input (default value = '').
    // Type "Done" into the value field for the Status condition chip.
    const valueInput = page.locator('input[type="text"]').first();
    await expect(valueInput).toBeVisible({ timeout: 3_000 });
    await valueInput.fill('Done');
    await valueInput.dispatchEvent('input');

    // ── Step 4: wait for debounced refresh_tasks → kanban narrows ────────────
    // filterStore.update fires set_lark_repo_binding + tasks.refresh() after
    // 300 ms debounce. The tasks store clears and re-populates with the
    // narrowed result, causing KanbanBoard to re-hydrate its columns.
    await expect(page.getByText('Done task')).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText('Todo task')).not.toBeVisible({ timeout: 5_000 });
  });

  // Test 2: Remove the filter → both tasks visible again
  test('removing the only filter restores all tasks', async ({ page, harness }) => {
    void harness;

    await installTauriShim(page, {
      dialogOpenPath: FIXTURE_REPO_PATH,
      initialRepos: [
        {
          id: REPO_ID,
          name: 'filter-e2e-repo',
          path: FIXTURE_REPO_PATH,
          gh_profile: null,
          default_branch: 'main',
          created_at: 1_700_000_000,
          updated_at: 1_700_000_000,
        },
      ],
      // Start with only the Done task visible (filter already active).
      initialTasks: [TASK_DONE],
    });

    // Seed binding with a pre-existing Status=Done filter condition.
    const bindingWithFilter = {
      ...LARK_BINDING,
      filters: {
        conjunction: 'and',
        conditions: [
          {
            field_id: FIELD_ID_STATUS,
            field_name: 'Status',
            operator: 'is',
            value: ['Done'],
          },
        ],
      },
    };

    await page.addInitScript(
      ({
        repoId,
        binding,
        fields,
        taskDone,
        taskTodo,
      }: {
        repoId: string;
        binding: unknown;
        fields: unknown[];
        taskDone: unknown;
        taskTodo: unknown;
      }) => {
        const larkState: { binding: unknown; filterConditions: unknown[] } = {
          binding,
          filterConditions: (binding as { filters: { conditions: unknown[] } }).filters.conditions,
        };

        const internals = (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] as {
          invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
        };

        const originalInvoke = internals.invoke.bind(internals);

        internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
          switch (cmd) {
            case 'list_lark_repo_bindings':
              return { [repoId]: larkState.binding };

            case 'get_lark_repo_binding':
              return args.repoId === repoId ? larkState.binding : null;

            case 'set_lark_repo_binding': {
              const updated = args.binding as { filters: { conditions: unknown[] } };
              larkState.binding = args.binding;
              larkState.filterConditions = updated?.filters?.conditions ?? [];
              return undefined;
            }

            case 'delete_lark_repo_binding':
              return undefined;

            case 'list_lark_fields':
              return fields;

            case 'refresh_tasks': {
              const conditions = larkState.filterConditions as Array<{
                field_name: string;
                operator: string;
                value: string[];
              }>;
              const doneFilter = conditions.find(
                (c) =>
                  c.field_name === 'Status' &&
                  c.operator === 'is' &&
                  c.value.some((v) => typeof v === 'string' && v.toLowerCase().includes('done'))
              );
              if (doneFilter) return [taskDone];
              // Filter cleared — return both tasks.
              return [taskDone, taskTodo];
            }

            default:
              return originalInvoke(cmd, args);
          }
        };
      },
      {
        repoId: REPO_ID,
        binding: bindingWithFilter,
        fields: LARK_FIELDS,
        taskDone: TASK_DONE,
        taskTodo: TASK_TODO,
      }
    );

    await page.goto('/');

    // Initially only Done task should be visible (pre-filtered state).
    await expect(page.getByText('Done task')).toBeVisible({ timeout: 10_000 });
    // The filter chip for Status must be present.
    await expect(page.getByRole('button', { name: /remove condition/i })).toBeVisible({
      timeout: 5_000,
    });

    // ── Remove the filter condition chip ─────────────────────────────────────
    await page.getByRole('button', { name: /remove condition/i }).click();

    // After the 300 ms debounce, refresh_tasks is called with no conditions.
    // Both tasks should appear.
    await expect(page.getByText('Done task')).toBeVisible({ timeout: 3_000 });
    await expect(page.getByText('Todo task')).toBeVisible({ timeout: 3_000 });
  });
});
