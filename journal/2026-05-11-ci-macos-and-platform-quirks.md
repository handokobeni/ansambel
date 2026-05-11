# Journal — 2026-05-11 — CI macOS matrix + platform quirks doc

## PR: chore(ci) — add macOS to CI matrix + platform-quirks doc

**Branch:** `chore/ci-macos-and-platform-quirks` **Author:** handokobeni **PR:**
#19

### Summary

Preventif hardening setelah seharian debugging Windows CI (lihat journal
`2026-05-11-phase-2b-windows-ci-fix.md`). Dua perubahan kecil, dampak besar
untuk fase-fase berikutnya:

1. **macOS masuk CI matrix.** `macos-14` (Apple Silicon) ditambahin ke dua
   matrix job — `rust` (cargo llvm-cov) dan `e2e` (Playwright smoke). Sekarang
   tiap PR di-test di Linux + Windows + macOS sekaligus dengan
   `fail-fast: false` jadi satu OS gagal gak cancel yang lain. Murah sekarang
   (codebase masih kecil), mahal nanti kalau Phase 3+ sudah numpuk dan baru
   ketauan macOS broken.
2. **`docs/platform-quirks.md`.** Catatan kanonik tiap gotcha OS-specific yang
   udah kita tabrak. Format setiap entry konsisten: symptom → root cause → fix
   pattern → file references. Tujuannya: sesi berikutnya (orang lain ATAU Claude
   Code di hari lain) gak perlu rediscover bug yang udah pernah kita selesaikan.
   CLAUDE.md dapet one-liner pointer di section sebelum Architecture.

### Konteks — kenapa sekarang

Hari ini saja CI Windows fail 3x sebelum hijau (lihat journal Phase 2b hari
ini). Tiap iterasi ~5 menit, jadi ~15 menit ke-buang per ronde nebak-nebak.
Bug-nya bukan acak — udah pernah ke-trigger sebelumnya dengan gejala beda. Tanpa
catatan, tiap orang yang nyentuh PTY/IPC code ke depan bakal ulang loop yang
sama.

### macOS — kenapa berani tambah sekarang

- Codebase masih ~50KB Rust + ~80KB TS. Surface error kecil.
- `portable-pty` punya macOS backend yang mature (pakai posix_openpt).
- `tauri-plugin-fs`, `tauri-plugin-opener`, `tauri-plugin-dialog` semua resmi
  support macOS.
- Test suite sekarang dominan MockPty (dari refactor hari ini) — gak sentuh real
  shell, jadi macos-spawn-quirks gak akan jadi bottleneck.
- Cost: ~2 menit per CI run (macOS-14 runner cepet). Worth it.

Kalau nanti macOS reveal bug (kemungkinan: keyring access, app data dir path,
wake lock API beda), kita dokumentasikan di `platform-quirks.md` sesuai pattern.

### `platform-quirks.md` — 9 entri awal

Semua dari work yang udah landed:

1. **Windows ConPTY tidak EOF on clean child exit** — drop master, bukan kill
   child
2. **Windows cmd.exe butuh `\r` bukan `\n`** — line terminator hygiene
3. **portable-pty CommandBuilder env kosong** — inherit `PATH/HOME/USER`
   manual + set `TERM`
4. **Bash/zsh butuh `-i` flag** — supaya print PS1
5. **Tauri `Channel<T>` single-use across invokes** — factory pattern, fresh
   Channel tiap invoke
6. **xterm.js blank di hidden container** — ResizeObserver-driven
   `waitForLayout` sebelum `term.open()`
7. **Tauri CSP butuh `ipc:` + `http://ipc.localhost`** di `connect-src`
8. **Test inputs cross-platform line terminator** — `\r\n` untuk PTY, forward
   slash untuk path JSON
9. **Per-OS dev surface** — MockPty untuk state-machine, real PTY cuma untuk
   smoke check

### Workflow ke depan

Tiap fix bug yang OS-specific (lolos di satu OS, fail di OS lain):

1. Buka `platform-quirks.md`
2. Tambah entry baru dengan format 4-section (Symptom / Root cause / Fix pattern
   / See)
3. Commit bareng fix-nya

Doc itu sendiri nutup dengan blok "Adding to this file" supaya pattern-nya
self-perpetuating.

### Commits

```
3072cfb chore(ci): add macOS to CI matrix + platform-quirks doc
```

Satu commit gabungan (CI + docs + CLAUDE.md pointer) karena ketiganya saling
melengkapi — split jadi tiga commit malah numpukin overhead review tanpa
benefit.

### Lessons captured untuk future

- **Pre-merge gate**: branch protection rule `main` sekarang perlu required
  check "Rust check & test (macos-14)" + "E2E smoke (macos-14)" selain dua OS
  lainnya. Catat sebagai todo follow-up — gak blocking PR ini karena di-set di
  Settings → Branches, bukan di code.
- **Workflow file maintainability**: matrix item tambah satu kalau OS baru.
  Linux deps install masih if-guarded ke `runner.os == 'Linux'` jadi macOS skip
  otomatis. Bagus.
- **Doc cost / benefit**: nulis `platform-quirks.md` butuh ~30 menit. Saving
  target: satu Windows-CI debug round-trip = ~15 menit. Break-even setelah 2
  lookup. Hari ini saja kita habis 3 round-trip — ROI udah positif di hari yang
  sama.
