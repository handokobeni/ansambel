export type TokenName =
  | 'bg-base'
  | 'bg-sidebar'
  | 'bg-titlebar'
  | 'bg-card'
  | 'bg-hover'
  | 'bg-active'
  | 'border'
  | 'border-light'
  | 'text-muted'
  | 'text-dim'
  | 'text-secondary'
  | 'text-primary'
  | 'text-bright'
  | 'accent'
  | 'status-ok'
  | 'diff-add'
  | 'diff-add-bg'
  | 'diff-del'
  | 'diff-del-bg'
  | 'error'
  | 'error-bg';

export type ThemeTokens = Record<TokenName, string>;

export const THEMES = {
  'warm-dark': {
    'bg-base': 'oklch(0.16 0.01 60)',
    'bg-sidebar': 'oklch(0.13 0.01 60)',
    'bg-titlebar': 'oklch(0.14 0.01 60)',
    'bg-card': 'oklch(0.20 0.01 60)',
    'bg-hover': 'oklch(0.24 0.01 60)',
    'bg-active': 'oklch(0.28 0.01 60)',
    border: 'oklch(0.24 0.01 60)',
    'border-light': 'oklch(0.30 0.01 60)',
    'text-muted': 'oklch(0.55 0.01 60)',
    'text-dim': 'oklch(0.65 0.01 60)',
    'text-secondary': 'oklch(0.75 0.01 60)',
    'text-primary': 'oklch(0.88 0.005 60)',
    'text-bright': 'oklch(0.96 0.005 60)',
    accent: 'oklch(0.78 0.14 70)',
    'status-ok': 'oklch(0.70 0.15 140)',
    'diff-add': 'oklch(0.72 0.13 140)',
    'diff-add-bg': 'oklch(0.22 0.05 140)',
    'diff-del': 'oklch(0.70 0.18 25)',
    'diff-del-bg': 'oklch(0.22 0.06 25)',
    error: 'oklch(0.70 0.18 25)',
    'error-bg': 'oklch(0.25 0.08 25)',
  },
} as const satisfies Record<string, ThemeTokens>;

export type ThemeName = keyof typeof THEMES;

export function themeNames(): ThemeName[] {
  return Object.keys(THEMES) as ThemeName[];
}

export function tokensForTheme(name: ThemeName): ThemeTokens {
  return THEMES[name] ?? THEMES['warm-dark'];
}
