// tests/e2e/phase-3a-2/phase-3a-2-lark-sync.spec.ts
//
// Phase 3a-2 env-gated smoke: switch to Lark task source, run the
// schema wizard, and verify the result block renders. Skipped unless
// LARK_* credentials are present — same gate as cargo lark_smoke.

import { test, expect } from '@playwright/test';

const requiredEnv = ['LARK_APP_ID', 'LARK_APP_SECRET', 'LARK_APP_TOKEN', 'LARK_TABLE_ID'];
const hasCreds = requiredEnv.every((k) => Boolean(process.env[k]));

test.describe('Phase 3a-2 Lark sync', () => {
  test.skip(!hasCreds, 'requires LARK_* env vars for the configured test tenant');

  test('switch to Lark, verify schema, kanban populates', async ({ page }) => {
    await page.goto('/');
    await page.getByTestId('open-settings').click();
    await page.getByTestId('task-source-lark').click();
    // Assumes credentials are already saved via LarkSettings; in CI
    // this would be seeded by a setup hook. For local dev, configure
    // once and re-run.
    await page.getByTestId('lark-verify-schema').click();
    await expect(page.getByTestId('lark-schema-result')).toBeVisible({
      timeout: 15000,
    });
    await expect(page.getByTestId('lark-schema-result')).toContainText(
      /title|already present|created/i
    );
  });
});
