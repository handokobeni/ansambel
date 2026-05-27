# Ansambel

> Orchestrate your AI ensemble.

Cross-platform (Windows + Linux + macOS) desktop app that orchestrates parallel
Claude Code agents in isolated git worktrees. Modeled after
[korlap](https://github.com/ariaghora/korlap) (macOS-only) and
[Conductor](https://www.conductor.build).

**Status:** Phase 2 shipped (v0.2.0) — Work Mode Complete (diff, files, search,
editor, terminal, scripts, @-file mentions). Phase 3a (Lark Bitable team sync)
planned.

## Stack

- Tauri v2 + Rust + Svelte 5 + Bun + Tailwind v4
- Claude Code CLI as agent process
- Jira Cloud + Lark Bitable as task providers (Phase 3)

## Development

Prerequisites:

- Rust stable (1.82+)
- Bun latest
- Linux only: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- WSL only: `wslu` (`sudo apt install wslu`) — provides `wslview`, the URL
  handler the opener plugin shells out to. Without it (and without `xdg-open`),
  external links such as the Team Activity watch view's "Open branch on GitHub"
  / "Open PR" buttons silently fail to launch a browser; the app surfaces an
  error toast in that case rather than opening the link.

```bash
bun install             # also installs git hooks via husky
bun tauri dev           # launch in dev mode
bun run check           # type check
bun run lint            # ESLint + Prettier check
bun run lint:fix        # auto-fix formatting and lint issues
bun run test            # unit tests
bun run test:coverage   # unit + coverage gate (95%)
bun run e2e             # E2E smoke
cd src-tauri && cargo fmt --all -- --check && cargo clippy --lib --all-targets -- -D warnings && cargo test --lib && cd ..
```

## Documentation

- Design spec: `docs/superpowers/specs/2026-04-17-ansambel-design.md`
- Phase plans: `docs/superpowers/plans/`
- Architecture decisions: `docs/adr/`

## License

[MIT](./LICENSE) © 2026 Handoko Beni.
