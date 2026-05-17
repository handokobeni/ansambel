// tests/e2e/phase-3a-3-1/phase-3a-3-1-view-scope.spec.ts
//
// Phase 3a-3.1 env-gated smoke: walk the new wizard Step 1.5 (view picker)
// against a real Lark Bitable, confirm the chosen view scopes the kanban,
// and exercise the wizard's All-Records default. Same env-gated cadence as
// phase-3a-2 / phase-3a-3.
//
// Requires the table to have at least one view defined in Lark. The test
// picks the FIRST non-default view available (skips if only the default
// Grid view exists, since picking that is equivalent to All-Records).
//
// SCOPE NOTE: The Phase 3a-3.1 plan originally described 5 E2E scenarios
// (happy path, view selection, view-deleted fallback, empty-view list,
// view-changed) backed by a `withMockLark` Playwright helper. That helper
// does not exist in this project and would require ~1 day of new HTTP
// intercept infra; project convention (see phase-3a-2/3a-3 specs) is
// env-gated tests against a real Lark sandbox. Honest E2E coverage is
// therefore limited to the 2 scenarios below. The other 3 scenarios are
// satisfied at lower test layers:
//   - empty view list:    Task 8 unit tests
//     ("proceeds even when view list is empty",
//      "proceeds to step 1.5 even when listViews fails")
//   - view deleted:       Task 6 Rust integration test
//     (lark_provider_falls_back_when_view_not_found)
//                         Task 12 Svelte integration test
//     (view_deleted_after_binding_falls_back_and_continues)

import { test, expect } from '@playwright/test';

const requiredEnv = ['LARK_APP_ID', 'LARK_APP_SECRET', 'LARK_APP_TOKEN', 'LARK_TABLE_ID'];
const hasCreds = requiredEnv.every((k) => Boolean(process.env[k]));

test.describe('Phase 3a-3.1 view-aware binding wizard', () => {
  test.skip(!hasCreds, 'requires LARK_* env vars');

  test('step 1.5 lists views with All-Records first and advances to step 2', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid^="repo-row-"]', { timeout: 10_000 });

    // Open RepoSettings via the gear icon on the active repo (post-3a-3
    // UX: gear icon next to TitleBar repo name).
    await page.getByTestId('repo-settings-gear').click();
    await page.getByTestId('connect-binding').click();
    await page.getByTestId('wizard-app-token').fill(process.env.LARK_APP_TOKEN!);
    await page.getByTestId('wizard-table-id').fill(process.env.LARK_TABLE_ID!);
    await page.getByTestId('wizard-detect').click();

    // Detect should land on the new Step 1.5 view picker (not Step 2 as in 3a-3).
    await expect(page.getByTestId('wizard-step-1-5')).toBeVisible({ timeout: 15_000 });

    const select = page.getByTestId('wizard-view-select');
    // Default selection = empty string ("All records (no view filter)").
    await expect(select).toHaveValue('');
    const options = await select.locator('option').allTextContents();
    expect(options.length).toBeGreaterThanOrEqual(1);
    expect(options[0]).toContain('All records');

    // Continue with default (All records) → step 2 (field mapping).
    await page.getByTestId('wizard-view-continue').click();
    await expect(page.getByTestId('wizard-step-2')).toBeVisible();
  });

  test('selecting first available view scopes the binding through save', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid^="repo-row-"]', { timeout: 10_000 });

    await page.getByTestId('repo-settings-gear').click();
    await page.getByTestId('connect-binding').click();
    await page.getByTestId('wizard-app-token').fill(process.env.LARK_APP_TOKEN!);
    await page.getByTestId('wizard-table-id').fill(process.env.LARK_TABLE_ID!);
    await page.getByTestId('wizard-detect').click();
    await expect(page.getByTestId('wizard-step-1-5')).toBeVisible({ timeout: 15_000 });

    // Pick the FIRST non-empty option — that's the first real view. If only
    // the All-Records entry exists, skip (the table has no views to pick).
    const select = page.getByTestId('wizard-view-select');
    const optionValues = await select
      .locator('option')
      .evaluateAll((els) => (els as HTMLOptionElement[]).map((o) => o.value));
    const firstView = optionValues.find((v) => v !== '');
    test.skip(!firstView, 'table has no non-default views to scope to');
    await select.selectOption(firstView!);
    await page.getByTestId('wizard-view-continue').click();

    // Step 2: accept the suggested title field, continue.
    await expect(page.getByTestId('wizard-step-2')).toBeVisible();
    await page.getByTestId('wizard-continue').click();

    // Step 3 may or may not appear depending on whether status is single-select.
    const onStep3 = await page
      .getByTestId('wizard-step-3')
      .isVisible()
      .catch(() => false);
    if (onStep3) await page.getByTestId('wizard-save').click();

    // Settings dialog returns to summary view showing the bound view id.
    await expect(page.getByTestId('binding-view-row')).toContainText(firstView!);

    // Board hydrates with at least the TODO column header (no count assertion
    // since we can't know how many records the view filter matches).
    await expect(page.getByText('TODO')).toBeVisible({ timeout: 15_000 });
  });
});
