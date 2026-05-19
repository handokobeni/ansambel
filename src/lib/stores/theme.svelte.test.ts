import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const STORAGE_KEY = 'ansambel-theme';
const MODE_STORAGE_KEY = 'ansambel-color-mode';

async function freshStore() {
  // Reset module so each test starts with module-level state cleared
  // and a fresh subscription to media-query / window events.
  vi.resetModules();
  return await import('./theme.svelte');
}

function setMatchMedia(matches: boolean) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches,
      media: '(prefers-color-scheme: dark)',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
      onchange: null,
    }),
  });
}

describe('theme store', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute('style');
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.removeAttribute('data-mode');
    setMatchMedia(true);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('exposes default theme and color mode (dark by default)', async () => {
    // The project's identity is "warm dark"; a fresh install must land
    // on dark rather than tracking a (possibly headless) OS preference.
    const { theme } = await freshStore();
    expect(theme.themeId).toBe('warm-dark');
    expect(theme.colorMode).toBe('dark');
  });

  it.each(['dark', 'light', 'system'] as const)(
    'reads persisted color mode "%s" from localStorage',
    async (mode) => {
      localStorage.setItem(MODE_STORAGE_KEY, mode);
      const { theme } = await freshStore();
      expect(theme.colorMode).toBe(mode);
    }
  );

  it('falls back to dark when stored mode is invalid', async () => {
    localStorage.setItem(MODE_STORAGE_KEY, 'rainbow');
    const { theme } = await freshStore();
    expect(theme.colorMode).toBe('dark');
  });

  it('falls back to dark when stored mode is the empty string', async () => {
    localStorage.setItem(MODE_STORAGE_KEY, '');
    const { theme } = await freshStore();
    expect(theme.colorMode).toBe('dark');
  });

  it('reads persisted theme id from localStorage', async () => {
    localStorage.setItem(STORAGE_KEY, 'warm-dark');
    const { theme } = await freshStore();
    expect(theme.themeId).toBe('warm-dark');
  });

  it('ignores unknown stored theme id and falls back to default', async () => {
    localStorage.setItem(STORAGE_KEY, 'galaxy');
    const { theme } = await freshStore();
    expect(theme.themeId).toBe('warm-dark');
  });

  it('treats empty stored theme id as missing and falls back to default', async () => {
    localStorage.setItem(STORAGE_KEY, '');
    const { theme } = await freshStore();
    expect(theme.themeId).toBe('warm-dark');
  });

  it('setColorMode persists value and applies CSS vars', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('light');
    expect(theme.colorMode).toBe('light');
    expect(localStorage.getItem(MODE_STORAGE_KEY)).toBe('light');
    // Light mode bg should be applied as a custom property.
    const bgBase = document.documentElement.style.getPropertyValue('--bg-base');
    expect(bgBase).toBeTruthy();
  });

  it('setColorMode dark applies dark tokens', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('light');
    const lightBg = document.documentElement.style.getPropertyValue('--bg-base');
    theme.setColorMode('dark');
    const darkBg = document.documentElement.style.getPropertyValue('--bg-base');
    expect(darkBg).not.toBe(lightBg);
  });

  it('toggleColorMode flips dark→light→dark and persists explicit mode', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('dark');
    theme.toggleColorMode();
    expect(theme.colorMode).toBe('light');
    theme.toggleColorMode();
    expect(theme.colorMode).toBe('dark');
  });

  it('toggleColorMode resolves system mode against prefers-color-scheme', async () => {
    setMatchMedia(true); // system = dark
    const { theme } = await freshStore();
    theme.setColorMode('system');
    theme.toggleColorMode();
    // System resolved as dark, toggling should yield light.
    expect(theme.colorMode).toBe('light');
  });

  it('isDark returns true for dark mode', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('dark');
    expect(theme.isDark()).toBe(true);
  });

  it('isDark returns false for light mode', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('light');
    expect(theme.isDark()).toBe(false);
  });

  it('isDark resolves system mode via prefers-color-scheme', async () => {
    setMatchMedia(false);
    const { theme } = await freshStore();
    theme.setColorMode('system');
    expect(theme.isDark()).toBe(false);
  });

  it('initTheme sets data-theme and data-mode on documentElement', async () => {
    const { theme } = await freshStore();
    theme.initTheme();
    expect(document.documentElement.getAttribute('data-theme')).toBe('warm-dark');
    expect(['dark', 'light']).toContain(document.documentElement.getAttribute('data-mode'));
  });

  it('initTheme is safe to call multiple times (idempotent)', async () => {
    const { theme } = await freshStore();
    theme.initTheme();
    theme.initTheme();
    expect(document.documentElement.getAttribute('data-theme')).toBe('warm-dark');
  });

  it('setTheme updates the active theme id, persists, and applies vars', async () => {
    const { theme } = await freshStore();
    theme.setTheme('warm-dark');
    expect(theme.themeId).toBe('warm-dark');
    expect(localStorage.getItem(STORAGE_KEY)).toBe('warm-dark');
    const bgBase = document.documentElement.style.getPropertyValue('--bg-base');
    expect(bgBase).toBeTruthy();
  });

  it('setTheme ignores unknown theme ids (no-op)', async () => {
    const { theme } = await freshStore();
    const before = theme.themeId;
    // @ts-expect-error — intentionally invalid id
    theme.setTheme('nope-not-a-theme');
    expect(theme.themeId).toBe(before);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it('initTheme attaches a media listener that re-applies vars on system change', async () => {
    let captured: ((e: MediaQueryListEvent) => void) | null = null;
    // Track call count so we can flip the OS preference between the
    // initial apply (matches=true → dark) and the listener-driven apply
    // (matches=false → light). The branch we are covering is the
    // `if (this.colorMode === 'system') this.applyCssVars();` body.
    let calls = 0;
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      configurable: true,
      value: vi.fn().mockImplementation(() => {
        calls += 1;
        return {
          matches: calls < 3, // first 2 calls = dark, then flip to light
          media: '(prefers-color-scheme: dark)',
          addEventListener: (_evt: string, cb: (e: MediaQueryListEvent) => void) => {
            captured = cb;
          },
          removeEventListener: vi.fn(),
          addListener: vi.fn(),
          removeListener: vi.fn(),
          dispatchEvent: vi.fn(),
          onchange: null,
        };
      }),
    });
    const { theme } = await freshStore();
    theme.setColorMode('system');
    theme.initTheme();
    expect(captured).not.toBeNull();
    const before = document.documentElement.style.getPropertyValue('--bg-base');
    captured!({ matches: false } as MediaQueryListEvent);
    const after = document.documentElement.style.getPropertyValue('--bg-base');
    expect(after).toBeTruthy();
    expect(after).not.toBe(before);
  });

  it('initTheme is a safe no-op when matchMedia is unavailable', async () => {
    // Some headless / older runtime contexts don't expose matchMedia.
    // The store must not throw when initialising under those conditions.
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      configurable: true,
      value: undefined,
    });
    const { theme } = await freshStore();
    expect(() => theme.initTheme()).not.toThrow();
  });

  it('applyCssVars sets colorScheme to "dark" on documentElement', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('dark');
    expect(document.documentElement.style.colorScheme).toBe('dark');
  });

  it('applyCssVars sets colorScheme to "light" on documentElement', async () => {
    const { theme } = await freshStore();
    theme.setColorMode('light');
    expect(document.documentElement.style.colorScheme).toBe('light');
  });
});
