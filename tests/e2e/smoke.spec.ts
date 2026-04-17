import { test, expect } from '@playwright/test';
import { TauriDevHarness } from './helpers/tauri-driver';

let harness: TauriDevHarness;

test.beforeAll(async () => {
  harness = new TauriDevHarness();
  await harness.start();
});

test.afterAll(async () => {
  await harness.stop();
});

test('app shell renders with Ansambel heading', async ({ page }) => {
  await page.goto('/');
  const heading = page.getByRole('heading', { level: 1 });
  await expect(heading).toContainText('Ansambel');
});
