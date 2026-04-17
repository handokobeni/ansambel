# ADR 0001: Tech stack

Date: 2026-04-17 Status: accepted

## Context

Ansambel orchestrates multiple Claude Code agents in git worktrees. We need a
desktop app that runs on Windows and Linux (macOS nice-to-have), exposes native
PTY / process control, renders a reactive UI with many simultaneously streaming
log panes, and stays lightweight enough that users can run 10+ concurrent Claude
Code agents without the orchestrator itself being a bottleneck.

Korlap ships Tauri v2 + Svelte 5 + Rust and demonstrates the pattern works;
adopting the same stack makes the korlap codebase a usable reference while we
build from scratch.

## Decision

- **Shell:** Tauri v2. Native per-OS WebView (WKWebView / WebView2 / WebKitGTK)
  keeps binary size ~15 MB and idle RAM <150 MB. Rust backend gives us
  first-class process, PTY, and git tooling.
- **Frontend framework:** Svelte 5 with runes. Fine-grained reactivity is a
  better fit than React's vDOM for many simultaneously streaming panes, and the
  runtime is smaller than React or Vue.
- **Runtime / package manager:** Bun. Fast installs, built-in TypeScript,
  first-class vitest support. Windows support is good enough from v1.1+.
- **Styling:** Tailwind v4 utility classes plus CSS custom properties for
  themeable tokens defined in `src/lib/themes.ts`.
- **Testing:** Vitest + Playwright with 95% coverage gate. See
  `.claude/CLAUDE.md` for discipline.

## Alternatives considered

- **Electron.** Rejected: 80–150 MB binary, higher RAM per window, worse
  suitability for long-running multi-process orchestration.
- **Web app + local daemon.** Rejected for now: two-tier complexity, not "native
  feel". Can be added later by extracting the Rust backend as a daemon.
- **React instead of Svelte.** Rejected: larger hiring pool is the only win we
  care about, and streaming performance requires more manual memoization
  discipline than Svelte.
- **SolidJS.** Considered competitive; rejected in favour of Svelte because the
  korlap reference codebase is Svelte and the ecosystem for Tailwind / vitest in
  Svelte is well proven.

## Consequences

- The Mac-only parts of korlap (traffic-light positioning, hardcoded `~/Library`
  paths, keychain assumptions) need a platform abstraction layer built from day
  one.
- Windows PTY uses ConPTY through portable-pty — we need to test that path
  explicitly in CI.
- Svelte 5 runes are new; the team must get comfortable with $state / $derived
  semantics.
