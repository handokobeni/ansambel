# Ansambel

> Orchestrate your AI ensemble.

Cross-platform (Windows + Linux + macOS) desktop app that orchestrates
parallel Claude Code agents in isolated git worktrees. Modeled after
[korlap](https://github.com/ariaghora/korlap) (macOS-only) and
[Conductor](https://www.conductor.build).

**Status:** Phase 0 — Foundation (pre-alpha).

## Stack

- Tauri v2 + Rust + Svelte 5 + Bun + Tailwind v4
- Claude Code CLI as agent process
- Jira Cloud + Lark Bitable as task providers (Phase 3)

## Development

Prerequisites:

- Rust stable (1.82+)
- Bun latest
- Linux only: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`

```bash
bun install
bun tauri dev           # launch in dev mode
bun run check           # type check
bun run test            # unit tests
bun run test:coverage   # unit + coverage gate (95%)
bun run e2e             # E2E smoke
cd src-tauri && cargo test --lib && cd ..
```

## Documentation

- Design spec: `docs/superpowers/specs/2026-04-17-ansambel-design.md`
- Phase plans: `docs/superpowers/plans/`
- Architecture decisions: `docs/adr/`

## License

[MIT](./LICENSE) © 2026 Handoko Beni.
