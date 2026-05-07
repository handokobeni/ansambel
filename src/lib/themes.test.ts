import { describe, it, expect } from 'vitest';
import {
  THEMES,
  tokensForTheme,
  themeNames,
  DEFAULT_THEME,
  DEFAULT_MODE,
  type ThemeName,
  type ColorMode,
  type TokenName,
} from './themes';

const REQUIRED_TOKENS: TokenName[] = [
  'bg-base',
  'bg-sidebar',
  'bg-titlebar',
  'bg-card',
  'bg-hover',
  'bg-active',
  'border',
  'border-light',
  'text-muted',
  'text-dim',
  'text-secondary',
  'text-primary',
  'text-bright',
  'accent',
  'status-ok',
  'diff-add',
  'diff-add-bg',
  'diff-del',
  'diff-del-bg',
  'error',
  'error-bg',
  'toast-bg',
  'toast-border',
  'overlay-bg',
  'btn-subtle-bg',
  'btn-subtle-hover',
  'code-inline-bg',
  'code-block-bg',
];

describe('themes', () => {
  it('default theme defaults are sane', () => {
    expect(DEFAULT_THEME).toBe('warm-dark');
    expect(DEFAULT_MODE).toBe('dark');
  });

  it('warm-dark theme defines every required token in both modes', () => {
    for (const mode of ['dark', 'light'] as ColorMode[]) {
      const tokens = tokensForTheme('warm-dark', mode);
      for (const token of REQUIRED_TOKENS) {
        expect(tokens, `${mode}.${token}`).toHaveProperty(token);
        expect(tokens[token], `${mode}.${token}`).toBeTruthy();
      }
    }
  });

  it('themeNames lists all registered themes', () => {
    const names: ThemeName[] = themeNames();
    expect(names).toContain('warm-dark');
    expect(names.length).toBeGreaterThan(0);
  });

  it('THEMES contains a {dark, light} entry for every theme name', () => {
    for (const n of themeNames()) {
      expect(THEMES[n]).toBeDefined();
      expect(THEMES[n].dark).toBeDefined();
      expect(THEMES[n].light).toBeDefined();
    }
  });

  it('tokensForTheme defaults to dark mode when mode omitted', () => {
    const t = tokensForTheme('warm-dark');
    expect(t).toEqual(THEMES['warm-dark'].dark);
  });

  it('tokensForTheme returns the requested mode', () => {
    const dark = tokensForTheme('warm-dark', 'dark');
    const light = tokensForTheme('warm-dark', 'light');
    expect(dark).toBe(THEMES['warm-dark'].dark);
    expect(light).toBe(THEMES['warm-dark'].light);
    // Light mode background must be lighter than dark (sanity check that
    // we did not accidentally clone the dark palette).
    expect(dark['bg-base']).not.toEqual(light['bg-base']);
  });

  it('tokensForTheme falls back to default theme for unknown names', () => {
    // @ts-expect-error — intentional wrong name for fallback test
    const t = tokensForTheme('unknown-theme');
    expect(t).toEqual(THEMES[DEFAULT_THEME].dark);
  });

  it('tokensForTheme falls back to default theme + requested mode for unknown names', () => {
    // @ts-expect-error — intentional wrong name for fallback test
    const t = tokensForTheme('unknown-theme', 'light');
    expect(t).toEqual(THEMES[DEFAULT_THEME].light);
  });
});
