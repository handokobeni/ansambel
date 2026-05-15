// tests/e2e/phase-3a-3/phase-3a-3-binding-wizard.spec.ts
//
// Phase 3a-3 env-gated smoke: configure a per-repo Lark binding via the
// new wizard, verify board hydrates. Skipped unless LARK_* env vars
// present (same pattern as phase-3a-2).

import { test, expect } from '@playwright/test';

const requiredEnv = ['LARK_APP_ID', 'LARK_APP_SECRET', 'LARK_APP_TOKEN', 'LARK_TABLE_ID'];
const hasCreds = requiredEnv.every((k) => Boolean(process.env[k]));

test.describe('Phase 3a-3 binding wizard', () => {
  test.skip(!hasCreds, 'requires LARK_* env vars');

  test('connect → detect → confirm mapping → board hydrates', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid^="repo-row-"]', {
      timeout: 10000,
    });
    const firstRepo = page.locator('[data-testid^="repo-row-"]').first();
    await firstRepo.click({ button: 'right' });
    await page.getByTestId('connect-binding').click();
    await page.getByTestId('wizard-app-token').fill(process.env.LARK_APP_TOKEN!);
    await page.getByTestId('wizard-table-id').fill(process.env.LARK_TABLE_ID!);
    await page.getByTestId('wizard-detect').click();
    await expect(page.getByTestId('wizard-step-2')).toBeVisible({
      timeout: 15000,
    });
    await page.getByTestId('wizard-continue').click();
    const onStep3 = await page
      .getByTestId('wizard-step-3')
      .isVisible()
      .catch(() => false);
    if (onStep3) await page.getByTestId('wizard-save').click();
    await expect(page.getByText('TODO')).toBeVisible({ timeout: 15000 });
  });
});
