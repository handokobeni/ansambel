import { test, expect } from './helpers/fixtures';

test('app shell renders with Ansambel heading', async ({ page, harness }) => {
  void harness; // ensures the worker-scoped harness starts before this spec runs
  await page.goto('/');
  const heading = page.getByRole('heading', { level: 1 });
  await expect(heading).toContainText('Ansambel');
});
