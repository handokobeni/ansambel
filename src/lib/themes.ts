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
  | 'error-bg'
  | 'toast-bg'
  | 'toast-border'
  | 'overlay-bg'
  | 'btn-subtle-bg'
  | 'btn-subtle-hover'
  | 'code-inline-bg'
  | 'code-block-bg';

export type ColorMode = 'dark' | 'light';

export type ThemeTokens = Record<TokenName, string>;

export interface ThemeDefinition {
  dark: ThemeTokens;
  light: ThemeTokens;
}

const warmDarkDark: ThemeTokens = {
  'bg-base': 'oklch(0.16 0.01 60)',
  'bg-sidebar': 'oklch(0.13 0.01 60)',
  'bg-titlebar': 'oklch(0.14 0.01 60)',
  'bg-card': 'oklch(0.2 0.01 60)',
  'bg-hover': 'oklch(0.24 0.01 60)',
  'bg-active': 'oklch(0.28 0.01 60)',
  border: 'oklch(0.24 0.01 60)',
  'border-light': 'oklch(0.3 0.01 60)',
  'text-muted': 'oklch(0.55 0.01 60)',
  'text-dim': 'oklch(0.65 0.01 60)',
  'text-secondary': 'oklch(0.75 0.01 60)',
  'text-primary': 'oklch(0.88 0.005 60)',
  'text-bright': 'oklch(0.96 0.005 60)',
  accent: 'oklch(0.78 0.14 70)',
  'status-ok': 'oklch(0.7 0.15 140)',
  'diff-add': 'oklch(0.72 0.13 140)',
  'diff-add-bg': 'oklch(0.22 0.05 140)',
  'diff-del': 'oklch(0.7 0.18 25)',
  'diff-del-bg': 'oklch(0.22 0.06 25)',
  error: 'oklch(0.7 0.18 25)',
  'error-bg': 'oklch(0.25 0.08 25)',
  'toast-bg': 'oklch(0.2 0.01 60 / 0.85)',
  'toast-border': 'oklch(1 0 0 / 0.06)',
  'overlay-bg': 'oklch(0 0 0 / 0.5)',
  'btn-subtle-bg': 'oklch(1 0 0 / 0.06)',
  'btn-subtle-hover': 'oklch(1 0 0 / 0.1)',
  'code-inline-bg': 'oklch(1 0 0 / 0.06)',
  'code-block-bg': 'oklch(0 0 0 / 0.32)',
};

const warmDarkLight: ThemeTokens = {
  'bg-base': 'oklch(0.97 0.005 60)',
  'bg-sidebar': 'oklch(0.94 0.005 60)',
  'bg-titlebar': 'oklch(0.92 0.005 60)',
  'bg-card': 'oklch(0.95 0.005 60)',
  'bg-hover': 'oklch(0.9 0.01 60)',
  'bg-active': 'oklch(0.86 0.01 60)',
  border: 'oklch(0.86 0.01 60)',
  'border-light': 'oklch(0.78 0.01 60)',
  'text-muted': 'oklch(0.7 0.01 60)',
  'text-dim': 'oklch(0.55 0.01 60)',
  'text-secondary': 'oklch(0.42 0.01 60)',
  'text-primary': 'oklch(0.24 0.005 60)',
  'text-bright': 'oklch(0.16 0.005 60)',
  accent: 'oklch(0.55 0.14 70)',
  'status-ok': 'oklch(0.5 0.15 140)',
  'diff-add': 'oklch(0.5 0.15 140)',
  'diff-add-bg': 'oklch(0.93 0.05 140)',
  'diff-del': 'oklch(0.55 0.2 25)',
  'diff-del-bg': 'oklch(0.93 0.05 25)',
  error: 'oklch(0.5 0.2 25)',
  'error-bg': 'oklch(0.93 0.05 25)',
  'toast-bg': 'oklch(0.97 0.005 60 / 0.85)',
  'toast-border': 'oklch(0 0 0 / 0.08)',
  'overlay-bg': 'oklch(0 0 0 / 0.25)',
  'btn-subtle-bg': 'oklch(0 0 0 / 0.05)',
  'btn-subtle-hover': 'oklch(0 0 0 / 0.1)',
  'code-inline-bg': 'oklch(0 0 0 / 0.05)',
  'code-block-bg': 'oklch(0 0 0 / 0.07)',
};

export const THEMES = {
  'warm-dark': {
    dark: warmDarkDark,
    light: warmDarkLight,
  },
} as const satisfies Record<string, ThemeDefinition>;

export type ThemeName = keyof typeof THEMES;

export const DEFAULT_THEME: ThemeName = 'warm-dark';
export const DEFAULT_MODE: ColorMode = 'dark';

export function themeNames(): ThemeName[] {
  return Object.keys(THEMES) as ThemeName[];
}

export function tokensForTheme(name: ThemeName, mode: ColorMode = DEFAULT_MODE): ThemeTokens {
  const theme = THEMES[name] ?? THEMES[DEFAULT_THEME];
  return theme[mode];
}
