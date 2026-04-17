import { describe, it, expect } from 'vitest';
import { THEMES, tokensForTheme, type ThemeName, themeNames } from './themes';

describe('themes', () => {
  it('exports a warm-dark theme with required tokens', () => {
    const t = tokensForTheme('warm-dark');
    const required = [
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
    ] as const;
    for (const token of required) expect(t).toHaveProperty(token);
  });

  it('themeNames lists all registered themes', () => {
    const names: ThemeName[] = themeNames();
    expect(names).toContain('warm-dark');
    expect(names.length).toBeGreaterThan(0);
  });

  it('THEMES contains an entry for every theme name', () => {
    for (const n of themeNames()) {
      expect(THEMES[n]).toBeDefined();
    }
  });

  it('tokensForTheme falls back to warm-dark for unknown names', () => {
    // @ts-expect-error — intentional wrong name for fallback test
    const t = tokensForTheme('unknown-theme');
    expect(t).toEqual(THEMES['warm-dark']);
  });
});
