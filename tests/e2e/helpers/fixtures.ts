import { test as base, expect } from '@playwright/test';
import { TauriDevHarness } from './tauri-driver';

type Fixtures = {
  harness: TauriDevHarness;
};

/**
 * Shared Playwright fixture that starts one TauriDevHarness for the whole
 * worker and reuses it across specs. Because workers=1 (Tauri singleton),
 * this means exactly one dev server for the entire test run.
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export const test = base.extend<{}, Fixtures>({
  harness: [
    // eslint-disable-next-line no-empty-pattern
    async ({}, use) => {
      const harness = new TauriDevHarness();
      await harness.start();
      try {
        await use(harness);
      } finally {
        await harness.stop();
      }
    },
    { scope: 'worker' },
  ],
});

export { expect };
