import {
  DEFAULT_MODE,
  DEFAULT_THEME,
  THEMES,
  tokensForTheme,
  type ColorMode,
  type ThemeName,
} from '$lib/themes';

const STORAGE_KEY = 'ansambel-theme';
const MODE_STORAGE_KEY = 'ansambel-color-mode';

export type ColorModeSetting = ColorMode | 'system';

function readStoredTheme(): ThemeName {
  /* v8 ignore next 1 — SSR/Node guard: not reachable in jsdom. */
  if (typeof localStorage === 'undefined') return DEFAULT_THEME;
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored && stored in THEMES ? (stored as ThemeName) : DEFAULT_THEME;
}

function readStoredColorMode(): ColorModeSetting {
  // Default = dark. Korlap's design direction is "warm dark, all blacks
  // tinted amber" — that is the project's identity, so a fresh install
  // should land there rather than tracking the OS (which on a headless
  // WSL/Linux session resolves to light because no DE is reporting a
  // preference). Users who explicitly pick `system` keep it.
  /* v8 ignore next 1 — SSR/Node guard: not reachable in jsdom. */
  if (typeof localStorage === 'undefined') return 'dark';
  const stored = localStorage.getItem(MODE_STORAGE_KEY);
  if (stored === 'dark' || stored === 'light' || stored === 'system') return stored;
  return 'dark';
}

function prefersDark(): boolean {
  /* v8 ignore next 3 — SSR/Node guard: not reachable in jsdom. */
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return true;
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function resolveMode(setting: ColorModeSetting): ColorMode {
  if (setting === 'dark' || setting === 'light') return setting;
  return prefersDark() ? 'dark' : 'light';
}

class ThemeStore {
  themeId = $state<ThemeName>(readStoredTheme());
  colorMode = $state<ColorModeSetting>(readStoredColorMode());

  private mediaListenerAttached = false;

  setTheme(id: ThemeName): void {
    if (!(id in THEMES)) return;
    this.themeId = id;
    if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, id);
    this.applyCssVars();
  }

  setColorMode(mode: ColorModeSetting): void {
    this.colorMode = mode;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(MODE_STORAGE_KEY, mode);
    }
    this.applyCssVars();
  }

  /**
   * Flip explicit mode. If the current setting is `system`, resolve it
   * against `prefers-color-scheme` first so the toggle has a definite
   * starting point — and persist the result so the user keeps the
   * resolved value across reloads.
   */
  toggleColorMode(): void {
    const current = resolveMode(this.colorMode);
    this.setColorMode(current === 'dark' ? 'light' : 'dark');
  }

  isDark(): boolean {
    return resolveMode(this.colorMode) === 'dark';
  }

  applyCssVars(): void {
    /* v8 ignore next 1 — SSR/Node guard: not reachable in jsdom. */
    if (typeof document === 'undefined') return;
    const mode = resolveMode(this.colorMode);
    const tokens = tokensForTheme(this.themeId, mode);
    const root = document.documentElement;
    for (const [key, value] of Object.entries(tokens)) {
      root.style.setProperty(`--${key}`, value);
    }
    root.setAttribute('data-theme', this.themeId);
    root.setAttribute('data-mode', mode);
    root.style.colorScheme = mode; // 'dark' or 'light'
  }

  /**
   * Wire up CSS vars and start watching system preference changes so a
   * `system` setting tracks the OS in real time. Safe to call multiple
   * times — the media listener is attached at most once.
   */
  initTheme(): void {
    this.applyCssVars();
    if (this.mediaListenerAttached) return;
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => {
      if (this.colorMode === 'system') this.applyCssVars();
    };
    mq.addEventListener('change', handler);
    this.mediaListenerAttached = true;
  }
}

export const theme = new ThemeStore();
// Re-export for callers that want to construct an isolated store in tests.
export { ThemeStore, DEFAULT_THEME, DEFAULT_MODE };
export type { ColorMode, ThemeName };
