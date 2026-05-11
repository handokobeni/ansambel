# Journal — 2026-05-11 — Phase 2b Windows CI fix + Pty trait refactor

## PR: feat(phase-2b) — follow-up commits di hari ke-3

**Branch:** `feat/phase-2b-interactive` (lanjutan dari debug session 2026-05-08)
**Author:** handokobeni **PR:** #17

### Summary

Tiga commit hari ini menutup chapter terakhir Phase 2b — Windows CI yang udah
gagal 4x sebelumnya akhirnya hijau, lalu di-refactor supaya ke depan gak gampang
fail lagi. Story-nya: bug pertama (Windows ConPTY gak EOF) sebenarnya cuma 1
dari 2 bug yang nempel di test yang sama. Bug kedua (input `\n` bukan `\r`)
menyembunyikan bug pertama, jadi fix pertama keliatan kayak gak kerja sampai bug
kedua dibetulin juga.

### Commits (chronological)

```
6759e9d fix(pty): close master on child exit so Windows ConPTY readers unblock
348ecc0 test(terminal): send CR+LF as line terminator so cmd.exe sees Enter
d78e9d0 refactor(pty): trait + MockPty for OS-independent unit tests
```

### Bug #1 — Windows ConPTY tidak EOF on clean child exit

CI Windows fail di 3 test PTY (script + 2 terminal). Watchdog yang sebelumnya
(commit `b639053` di journal 2026-05-08) cuma manggil `child.kill()` waktu
`try_wait()` deteksi exit. Di Linux: kernel auto-close PTY waktu child mati →
reader EOF. Di Windows ConPTY: child mati tapi `conhost.exe` masih pegang pipe
handle. `child.kill()` ke anak yang udah mati = no-op. Reader thread tetep
nge-block selamanya.

**Fix.** Ubah `PtySession.master` dari `Box<dyn MasterPty>` ke
`Option<Box<dyn MasterPty>>` + tambah `close_master()` yang
`self.master.take()`. Watchdog manggil itu setelah `kill()` → ClosePseudoConsole
jalan → reader dapet EOF → Exited chunk fired.

### Bug #2 — `\n` saja gak cukup untuk cmd.exe

Setelah fix #1 di-push, CI Windows masih fail di 2 dari 3 test (yang ke-3
lulus). Bytes log nunjukin: cmd.exe nge-echo `exit` tapi gak eksekusi. Real
keyboard ENTER di Windows console = `\r` (CR), bukan `\n` (LF). xterm.js di
production sudah benar (kirim `\r`), tapi test hardcoded `b"exit\n"`.

**Fix.** Ganti ke `b"exit\r\n"`. Unix shell terima dua-duanya; cmd.exe ngenalin
`\r` sebagai line terminator. Bug #1 sebenarnya selalu ada juga, tapi gejalanya
kesembunyiin oleh bug #2 — proses gak pernah mati, jadi watchdog gak pernah
ke-trigger.

### Refactor — kenapa nutup chapter Windows CI bukan cuma di-band-aid

3 round-trip CI Windows hari ini ngabisin ~15 menit lebih. Bukan unik — kemarin
kita habis 4 round-trip untuk debug bug Channel single-use. Pattern-nya jelas:
tiap kali test pakai real PTY, Windows CI rentan flake. Solusinya bukan
"perbaiki test-nya" (band-aid) tapi "ubah struktur supaya state-machine bisa
ditest tanpa real PTY".

**Refactor.**

- Ekstrak `trait Pty` di `platform/pty.rs` dengan methods: `pid`, `reader`,
  `writer`, `kill`, `close_master`, `try_wait`, `resize`.
- Rename `PtySession` → `PortablePty` (production impl, wrap `portable-pty`).
- Tambah `MockPty` + `MockPtyHandle` — in-memory backend pakai mpsc channel.
  Test bisa push stdout, inspect stdin, set exit code, close channel —
  deterministic, OS-independent, instant.
- Tambah `spawn_terminal_inner_with_pty` dan `run_pty_with_emit` variant yang
  nerima `Box<dyn Pty>` siap pakai. Production wrapper bikin `PortablePty` lewat
  `pty::spawn` lalu delegate. Test inject `MockPty` langsung.
- Konversi 6 test state-machine (double-spawn, kill idempotent, resize clamp,
  reattach delivers, write forwarding, byte streaming) dari real-shell ke
  MockPty. Sisa 2 test real-shell per file sebagai smoke check bahwa
  build_shell_command + portable-pty beneran kerja di host.

Hasilnya: 399 test pass di Linux, 7 jobs hijau di CI Windows + Linux pertama
kali tanpa ronde tebak-tebakan.

### Lessons captured

1. **Selalu baca CI log lengkap dulu.** Bug #2 (CR vs LF) sebenarnya keliatan di
   bytes log CI pertama — `wt>exit` ke-echo tapi tidak ada line break
   setelahnya. Saya skip itu karena fokus ke "watchdog gak EOF". Ronde kedua:
   lihat log lagi → langsung jelas.
2. **Dua bug nempel di test sama bisa nyembunyiin satu sama lain.** Bug #1
   selalu reproducible; bug #2 nyembunyiin gejala bug #1 (anak gak pernah mati →
   watchdog gak ketrigger → kasus EOF gak pernah dicoba). Setiap fix harus
   diverifikasi exit-path beneran kena, bukan cuma "test merah jadi hijau".
3. **Mock di unit test, real di smoke test.** Real PTY adalah external
   dependency yang gak deterministic di coverage-instrumented CI. Test
   state-machine logic harus pakai mock. Real PTY cuma layak kalau test-nya
   verifikasi integrasi dengan shell host (e.g., build_shell_command bener-bener
   spawn cwd yang benar).
4. **Refactor saat panas, dokumentasi setelah dingin.** Refactor Pty-trait
   dimulai pas masih hot dari debug — saya tau persis apa yang fragile. Kalau
   ditunda, ingatan tentang detail bug bakal pudar dan refactor-nya cenderung
   over-abstract.

### Test surface sekarang

| Layer                | Real PTY? | Count | OS coverage |
| -------------------- | --------- | ----- | ----------- |
| Unit (state machine) | ❌ Mock   | 6     | semua OS    |
| Unit (PTY backend)   | ✅ Real   | 11    | per-OS      |
| Integration (smoke)  | ✅ Real   | 2     | per-OS      |
| E2E (Playwright)     | ✅ Real   | 13    | semua OS    |

Total 399 test, semua hijau di Linux + Windows.

### Follow-up (di PR terpisah)

- macOS masuk CI matrix → di PR #19 `chore/ci-macos-and-platform-quirks`
- `docs/platform-quirks.md` capture semua quirk yang udah kena di branch ini
  (ConPTY EOF, CR vs LF, env inheritance, bash `-i`, Channel single-use,
  xterm.js hidden mount, CSP IPC) → PR #19 juga

Branch ini sendiri sekarang siap merge: CI hijau di Linux + Windows, 399 unit
test + 13 E2E pass.
