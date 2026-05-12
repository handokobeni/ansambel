# Journal — 2026-05-12 — Phase 2 retrospective

## Phase: 2 (Work Mode Complete) — shipped as v0.2.0

**Branches merged:** `feat/phase-2a-readonly-workmode` (PR #16),
`feat/phase-2b-interactive` (PR #17), `feat/phase-2c-mentions` (PR #20)
**Author:** handokobeni **Span:** 2026-05-07 → 2026-05-12 (5 working days
actual; planned 3 weeks)

### Tujuan Phase 2

Bangun "Work Mode" — surface kerja terisolasi per workspace yang melengkapi chat
panel: liat diff worktree, browse file tree, search isi repo, edit file langsung
(CodeMirror), buka terminal interaktif (xterm.js), jalankan script, dan
referensi file di chat lewat `@`-mention. Goal-nya: user bisa di Ansambel terus
tanpa harus pindah ke VS Code untuk apply Claude's suggestion atau buka terminal
terpisah untuk run command.

### Sub-phase breakdown

| Sub-phase | Tema                 | Surface                                                                        | PR  |
| --------- | -------------------- | ------------------------------------------------------------------------------ | --- |
| 2a        | Read-only foundation | Diff view, File browser, Search, Tab strip                                     | #16 |
| 2b        | Interactive surfaces | Editor (CodeMirror 6), Terminal (xterm.js), Script runner, Pty trait + MockPty | #17 |
| 2c        | Productivity glue    | @-file mentions di chat input                                                  | #20 |

### Yang shipped (user-visible)

- **Diff view** dengan colored hunks dari `git diff` shell-out, streaming via
  Tauri Channel untuk diff besar (>5MB)
- **File browser** lazy-expand per direktori, gitignore-aware, dengan
  click-to-open ke Editor tab
- **Search** filename + content via ripgrep (fallback walkdir kalau rg gak ada),
  streaming hits
- **Editor** CodeMirror 6 dengan multi-tab (per file), atomic write ke worktree,
  sha1 race-detect, ⌃4 shortcut
- **Terminal** xterm.js + portable-pty per workspace, ⌃5 shortcut, reattach
  setelah switch workspace
- **Script runner** dropdown picker per repo, output stream ke terminal buffer
  yang sama
- **5-tab strip** (Chat / Diff / Files / Editor / Terminal) dengan hidden-mount
  pattern — xterm scrollback dan CodeMirror undo history bertahan saat user
  pindah tab
- **`@`-file mentions** di chat input dengan ranked autocomplete, keyboard nav,
  gitignore-aware

### Yang gak shipped (sengaja, defer)

- Multi-cursor / advanced editor features → Phase 4+
- Diff "accept hunk" UX → Phase 4 (EditDiffBlock)
- Command palette / fuzzy-finder (Cmd+K) → Phase 8
- Inline LSP hover / diagnostics di editor → Phase 6
- Code folding, minimap → Phase 8
- @-mention untuk symbol/function → Phase 6 (LSP)
- File preview on hover → Phase 8

### Effort actual vs planned

| Sub-phase | Planned       | Actual               | Notes                                      |
| --------- | ------------- | -------------------- | ------------------------------------------ |
| 2a        | ~1 minggu     | 1 hari (05-07)       | Pattern Phase 1 ke-reuse, gitignore matang |
| 2b        | ~1.5 minggu   | 3 hari (05-08→05-11) | Termasuk 3 hari debug saga Windows CI      |
| 2c        | ~0.5 minggu   | ~3 jam (05-11)       | Hand-roll ranking, scope ditahan ketat     |
| **Total** | **~3 minggu** | **5 hari**           | ~3× lebih cepat dari estimasi awal         |

Estimasi cadence Phase 2 overview (dari `2026-05-06-...phase-2-overview.md`)
asumsi: TDD penuh + 95% coverage + cross-platform CI. Faktor yang mempercepat:

- Pattern Phase 1 (Tauri command + inner test + IPC wrapper) matured — gak ada
  exploration cost untuk pola baru
- ripgrep + tree-walker udah dipakai di Phase 1, jadi Phase 2a tinggal bungkus
- Hidden-mount pattern di tab strip ke-derive langsung dari hard rule
  "xterm/CodeMirror never remount" — gak ada brainstorm

Faktor yang memperlambat (Phase 2b 3 hari):

- Bug Windows ConPTY EOF (lihat journal 2026-05-11-phase-2b-windows-ci-fix.md):
  3 round-trip CI sebelum hijau karena dua bug nempel di test yang sama
- Refactor `Pty` trait + MockPty (~6 jam) sebagai investasi anti-flake untuk
  fase-fase berikutnya

### Lessons captured (sudah masuk doc)

1. **Mock external dependencies di unit test, real di smoke test.** Real PTY
   adalah sumber Windows-CI flake. MockPty pattern (Phase 2b refactor) sekarang
   jadi norma untuk semua resource OS-specific.
2. **Drop master, bukan kill child, untuk Windows ConPTY EOF.** Dokumentasi di
   `docs/platform-quirks.md` entry #1.
3. **`\r` bukan `\n` untuk Windows cmd.exe.** Dokumentasi di
   `docs/platform-quirks.md` entry #2.
4. **Svelte 5 + testing-library: external-state pattern.** Component exported
   functions gak callable lewat test reference; parent owns nav state. Lihat
   journal 2026-05-11-phase-2c-mentions.md.
5. **Coverage threshold 95% adalah tight gate.** Tambah test edge-case sebagai
   bagian feature commit, bukan follow-up.
6. **macOS murah ditambahin sekarang.** Codebase masih kecil; cost CI ~2 menit
   per OS. Lihat journal 2026-05-11-ci-macos-and-platform-quirks.md.

### Test surface end-of-phase

| Layer                 | Count | Notes                                          |
| --------------------- | ----- | ---------------------------------------------- |
| Rust unit (lib)       | ~410+ | semua command, persistence, pty, state         |
| TS unit + component   | ~660+ | mentions, autocomplete, editor, tabs, etc.     |
| E2E (Playwright)      | ~16   | per sub-phase golden path, cross-OS            |
| Real PTY integration  | 2     | smoke check build_shell_command + portable-pty |
| MockPty state machine | 6+    | deterministic, cross-OS                        |

CI matrix: Linux + Windows + macOS untuk Rust + E2E. Branch protection
mengharuskan 3-OS hijau sebelum merge.

### Architecture decisions yang masih load-bearing untuk Phase 3+

- **`platform::pty::Pty` trait + MockPty + PortablePty** — pattern reusable
  untuk fase yang nyentuh OS-specific resource lain (Phase 9 headless? Phase 3
  sync polling?)
- **`platform-quirks.md`** sebagai catatan kanonik OS-specific bugs — workflow
  self-perpetuating, di-pointer dari CLAUDE.md
- **Tab strip 5-tab dengan hidden-mount** — slot untuk fase berikutnya
  (Knowledge tab? Activity tab?) tinggal nambah di TabStrip + WorkspaceView
- **Streaming via Tauri Channel** untuk semua high-frequency I/O (diff, search,
  agent stream, terminal, script run) — pattern matured, no per-feature design
  needed lagi
- **Per-workspace `SvelteMap` cache** untuk data yang fetch-once (file list,
  repo config) — bisa di-reuse untuk Phase 3a sync data

### Lo apa yang gak gue lakuin (rasanya skip tapi belum prioritas)

- **Drag-drop @-mention** dari File browser → Chat input
- **History/recent mentions** di autocomplete (prioritize file yang sering
  di-mention dulu)
- **File-write retry on sha1 race** — sekarang fail keras, user harus reload
  file dulu kalau out-of-sync
- **Multi-window per workspace** — tab strip + workspace switcher cukup untuk
  solo dev, multi-window cuma untuk multi-monitor
- **Per-script env vars / args** — script runner cuma jalanin raw command string

### Apa selanjutnya

- **v0.2.0 di-tag** setelah journal ini (di branch
  chore/phase-2-retro-and-v0.2.0)
- **Phase 3a — Lark Bitable team sync** sudah ada plan doc (PR #18 merged).
  Estimasi 6 minggu solo. Implementasi mulai kapan user decide.
- **Phase 3b** (Jira / multi-provider) deferred ke after 3a, arsitektur trait
  sudah disiapin di 3a-1.

### Closing thought

Phase 2 ngebuktikan asumsi awal: dengan pattern Phase 1 matured + hard rule yang
udah diketat di CLAUDE.md (TDD, 95% coverage, MockPty, ≥1 test per command +
component), feature surface gede bisa di-ship cepat tanpa kompromi kualitas.
Yang lambat: integrasi dengan OS yang behavior-nya gak intuitif (ConPTY!). Yang
cepat: feature yang pattern-nya udah ada.

Buat Phase 3+: pertahankan disiplin TDD, expand `platform-quirks.md` tiap nemu
bug OS-specific, dan jangan tambah dependency untuk masalah yang bisa di-handle
~40 LOC.
