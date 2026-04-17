import { test, expect } from './helpers/fixtures';
import { installTauriShim } from './helpers/tauri-shim';

test('app shell renders TitleBar and sidebar shell', async ({ page, harness }) => {
  void harness; // ensures the worker-scoped harness starts before this spec runs
  await installTauriShim(page, {});
  await page.goto('/');
  // TitleBar header is present
  await expect(page.locator('header')).toBeVisible({ timeout: 10000 });
  // Shows "No repo selected" when no repos loaded
  await expect(page.getByText('No repo selected')).toBeVisible({ timeout: 5000 });
});
