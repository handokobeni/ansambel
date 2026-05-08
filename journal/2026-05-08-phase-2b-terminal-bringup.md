# Journal — 2026-05-08 — Phase 2b terminal bringup

## Phase 2b post-shipping — terminal bringup + coverage debug session

**Branch:** `feat/phase-2b-interactive` (post-merge follow-ups) **Author:**
handokobeni

### Summary

Phase 2b shipped (PR #17, journal 2026-05-09) tapi user manual-test nemu
beberapa bug yang gak ke-cover unit/E2E. Plus CI Windows runner gagal di tiga
PTY test. Sesi ini menyelesaikan delapan fix berturut- turut — sebagian
regression yang teridentifikasi via debugging iteratif (xterm tidak render →
render tapi shell diam → bytes flow tapi xterm tidak update), sebagian
gate-pelapis (CI coverage + Windows PTY EOF). Final fix: **Tauri's `Channel<T>`
adalah single-use across invokes** — bug subtle yang memakan 6 commit untuk
sampai ke akar.

### Lini timing fix (kronologis)

| Commit    | Fix                                                            | Cara identifikasi                                             |
| --------- | -------------------------------------------------------------- | ------------------------------------------------------------- |
| `e301913` | test(coverage): tutup branch gap Phase 2b agar global ≥ 95%    | CI fail                                                       |
| `b639053` | fix(pty): watchdog Windows ConPTY paksa-detect child exit      | CI Windows fail                                               |
| `2872e9a` | fix(terminal): import `@xterm/xterm/css/xterm.css`             | screenshot user — strip kecil + cursor floating               |
| `2af9647` | fix(terminal): defer `term.open()` sampai container ada layout | sama, hidden-mount jadi 0×0 saat open                         |
| `ddd810c` | fix(pty): inherit env (PATH/HOME/USER) supaya bash mau prompt  | screenshot kosong, log spawn ada                              |
| `fffcaea` | fix(pty): pass `-i` ke bash + diagnostic logs                  | bytes mungkin gak di-emit dari shell                          |
| `f98f834` | chore: log per chunk + xterm-ready banner                      | banner tampil → render OK; backend log konfirm bytes mengalir |
| `2d6a1ed` | chore: console.log per chunk di frontend                       | DevTools confirm onmessage gak fire                           |
| `cb1bb6f` | fix(csp): allow `ipc: ws:` di connect-src                      | DevTools error: "Refused to connect to ipc://"                |
| `3cb42ab` | fix(csp): drop CSP entirely (red herring)                      | masih "Couldn't find callback id"                             |
| `beaf776` | **fix(terminal): fresh Channel per invoke**                    | akar masalah: Tauri Channel single-use                        |

### The actual root cause

Tauri's `Channel<T>` argument tipe ini _single-use across command invokes_. Saat
command yang menerima `Channel<T>` parameter return `Err`, Rust drop Channel
struct — yang otomatis kirim cleanup signal ke JS untuk
`unregisterCallback(id)`. Channel JS object yang sama, kalau dipakai untuk
invoke kedua, ID-nya udah dead. Backend bisa push bytes ke ID tersebut, tapi JS
nyari `window._<id>` dan gak nemu — DevTools tampil error "Couldn't find
callback id" sekali per chunk.

Pattern lama yang bug:

```ts
const channel = new Channel<TerminalChunk>();
channel.onmessage = handle;

try {
  await api.terminal.reattach(workspaceId, channel);  // → reject (no session)
                                                       //   → Rust Channel dropped
                                                       //   → JS callback id deregistered
} catch {
  await api.terminal.spawn(workspaceId, channel, ...); // → bytes ke dead id
}
```

Pattern baru yang benar:

```ts
const handle = (chunk: TerminalChunk) => { /* ... */ };
const makeChannel = (): Channel<TerminalChunk> => {
  const ch = new Channel<TerminalChunk>();
  ch.onmessage = handle;
  return ch;
};

try {
  await api.terminal.reattach(workspaceId, makeChannel());
} catch {
  await api.terminal.spawn(workspaceId, makeChannel(), ...);
}
```

Dua Channel JS terpisah, dua callback id terpisah. Reattach gugur gak
menjatuhkan callback id spawn. Bytes nyampe.

### Fix-fix lain yang sah (bukan red herring)

Walaupun bukan akar, beberapa commit sebelumnya tetap bug nyata yang harus
diperbaiki:

- **`2872e9a` xterm CSS import** — `@xterm/xterm/css/xterm.css` gak pernah
  di-import di `main.ts`. Tanpa CSS, xterm cell metrics collapse — render tetap
  broken kalau Channel issue beres tapi CSS belum.
- **`2af9647` defer `term.open()` sampai layout siap** — Terminal panel mount
  via hidden-mount (`class:hidden` / `display:none`). Saat `Terminal.svelte`
  mount pertama kali, container 0×0. Kalau `term.open(containerRef)` dipanggil
  di state itu, cell metrics baked broken permanent walaupun ResizeObserver fire
  setelah. Solusi: `waitForLayout()` helper yang menunggu non-zero contentRect
  via ResizeObserver, dengan safety timeout 500ms untuk jsdom-style.
- **`ddd810c` inherit env vars** — `portable-pty::CommandBuilder` start dengan
  empty env. Tanpa PATH/HOME/USER, bash refuse prompt bahkan dengan TTY
  attached. Copy semua env dari parent, lalu override `TERM=xterm-256color`.
- **`fffcaea` `-i` flag** — bash heuristic untuk "should I prompt" butuh
  explicit `-i` selain TTY-attached stdin di sebagian config.
- **`b639053` Windows ConPTY watchdog** — Windows ConPTY gak reliably propagate
  EOF saat child exit clean. Watchdog poll `try_wait` 100ms → call `kill()`
  paksa close master saat child exited → reader unblock → Exited chunk fire.
  Linux unaffected (EOF fire normal).
- **`e301913` coverage gap** — global branch coverage drop dari 95.15% ke 94.09%
  karena `Editor.svelte` updateListener path, `Terminal.svelte` ResizeObserver
  branches, dan `editor-tabs` store not-found early-return branches gak
  ke-exercise. Tambah 6 test targeted; kembali ke 95.18%.

### Lessons / catatan

- **Always read all available DevTools output before guessing**. Pertama kali
  kita ngotot CSS, kedua env, ketiga `-i`, keempat CSP. Semua plausible secara
  kontekstual tapi gak match realita. "Couldn't find callback id" di console
  udah ada sejak awal — kalau saya minta DevTools output di iterasi pertama,
  ke-skip 5 commit.
- **Tauri Channel<T> lifecycle**: Channel di Rust side di-drop saat command
  return Err — drop trigger cleanup ke JS yang unregister callback id. Pattern
  reattach-or-spawn HARUS pakai Channel terpisah. Plan lain yang punya pattern
  serupa: agent reattach (Phase 1c). TODO: audit `WorkspaceView.svelte` agent
  reattach path — kemungkinan kena bug yang sama, tapi mungkin agent
  always-running jadi gak terdampak.
- **Hidden-mount + xterm.open()** punya gotcha: render-time apa pun di container
  0×0 mengeras jadi cell metrics broken. Pattern fix: `waitForLayout`
  ResizeObserver-driven dengan safety timeout untuk jsdom-style runtimes.
  Reusable untuk Editor + Terminal — tapi Editor kebetulan gak kena karena
  CodeMirror lebih lenient.
- **`-i` flag interactive shell + env inherit** kombinasi yang selalu butuh saat
  spawn shell via portable-pty. Standalone helpful documentation di code
  comment.
- **CSP CSP `default-src 'self'` di Tauri 2.x** memang menjepit IPC kustom.
  Re-add `connect-src 'self' ipc: http://ipc.localhost ws: wss:` setelah Channel
  fix dipasang — tetap relevan walaupun bukan akar masalah terminal.

### State akhir

- Branch coverage: 95.18% (gate 95%) ✓
- Rust unit tests: 395 passed (semua) ✓
- Frontend unit tests: 603 passed ✓
- E2E specs: 13/13 (Phase 1a + 1b + 1c + 1e + 2a + 2b + smoke) ✓
- Manual test Terminal tab: prompt bash muncul, ketikan diterima, command jalan,
  exit terdeteksi (Linux ✓, Windows pending CI verify)
- Diagnostic logs (`tracing::info!`) di backend reader/forwarder + xterm ready
  banner di frontend masih ada — bisa diturunkan ke `debug` di cleanup PR
  berikutnya tanpa urgensi.

### TODO follow-up

- [ ] Audit agent reattach path di `WorkspaceView.svelte` untuk pattern
      Channel-reuse-after-Err yang serupa. Mungkin gak terdampak karena agent
      reattach always-success (no fallback to spawn), tapi worth verifying.
- [ ] Turunkan diagnostic logs (`terminal reader chunk`,
      `channel forwarded chunk`, `[xterm ready — waiting for shell]`) ke
      debug/dev-only. Tidak urgent — info-level cuma muncul saat Terminal aktif.
- [ ] Investigate `connect-src 'self' ipc: http://ipc.localhost` cocok untuk
      semua platform (Linux WebKitGTK, Windows WebView2, macOS WKWebView) — saat
      ini di-test cuma di Linux WSL2.
