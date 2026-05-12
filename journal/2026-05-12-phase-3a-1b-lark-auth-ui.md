# Journal — 2026-05-12 — Phase 3a-1b Lark auth + UI

## PR: feat(phase-3a-1b) — Lark rate-limit, keyring auth, settings UI

**Branch:** `feat/phase-3a-1b-lark-auth-ui` **Author:** handokobeni **Phase:**
3a-1 (split 2 of 2 — closes Phase 3a-1)

### Summary

Closing slice untuk Phase 3a-1: rate-limit guard di `LarkClient`, Tauri command
surface untuk auth + persistence (keyring + JSON), Svelte settings form, dan
smoke-test helper buat manual round-trip ke real Lark API. Sekarang user bisa
masuk credentials lewat UI, klik "Test connection", dan tahu credentials valid
sebelum Phase 3a-2 (kanban sync) mulai jalan. PR ini juga close hard rule "never
log app_secret atau access_token" via OS keyring + redacted Debug.

### Yang shipped

- **`platform/lark_client.rs`** — RateLimiter (token bucket 200 req/min, refill
  3.33/s) + `send_with_retry` helper. Semua 6 GET/POST/PUT/DELETE methods diput
  lewat helper itu untuk acquire-then-send-then-retry-on-429 semantics.
  `attachment_upload` (multipart) cuma `acquire` tanpa retry — multipart form
  bodies tidak bisa cleanly direbuild di `Fn()` closure tanpa clone bytes setiap
  attempt.
- **`commands/lark_auth.rs`** — module baru, ~280 LOC + ~280 LOC tests. 4 Tauri
  commands: `set_lark_credentials`, `get_lark_status`, `test_lark_connection`,
  `clear_lark_credentials`. `app_secret` → OS keyring (service `ansambel.lark`,
  account `default`); `app_id`/`app_token`/`table_id`/`base_url` →
  `lark_settings.json` di app data dir.
- **`SecretStore` trait** — pluggable keyring abstraction. Production impl
  (`KeyringStore`) wraps `keyring 3` crate; test impl (`InMemorySecretStore`)
  pakai `HashMap` thread-safe. CI runners tidak punya OS keyring reliably, jadi
  semua unit test pakai in-memory store.
- **`LarkStatus` IPC shape** —
  `{ configured, app_id, app_token, table_id, base_url, has_secret }`.
  `app_secret` itu sendiri tidak pernah masuk respon — cuma `has_secret: bool`
  yang nyebrang IPC.
- **Frontend `LarkSettings.svelte`** — form 5-input (4 wajib + optional
  base_url) + tombol Save / Test connection / Clear + banner status. Secret
  field selalu blank di load — user re-entry untuk replace. Pre-fill 3 fields
  lain dari status.
  `api.lark.{setCredentials, getStatus, testConnection, clear}` wrappers di
  `src/lib/ipc.ts`.
- **`src-tauri/tests/lark_smoke.rs`** — 3 integration tests, semua `#[ignore]`
  by default. Read env vars (`LARK_APP_ID`/`LARK_APP_SECRET`/etc); skip dengan
  message kalau env missing (jadi `cargo test -- --ignored` aman dijalankan
  partial). Tests: `smoke_tenant_access_token`, `smoke_bitable_list_records`,
  `smoke_bitable_create_then_delete`.
- **`.env.example`** — dokumentasi env vars yang dibutuhkan smoke test, dengan
  warning eksplisit "never commit `.env`".
- **CI exclusion update** — `commands/lark_auth.rs` ditambahkan ke
  `--ignore-filename-regex` di `.github/workflows/ci.yml`, sama precedent dengan
  thin Tauri command modules lain (repo.rs, task.rs, …) yang inner-nya
  full-tested tapi thin Tauri wrappers butuh `AppHandle`.

### Tests

- **Rust lib**: 434 → 480 tests (46 new):
  - `lark_client` rate limiter (10 new): pure `try_acquire`/`refill` semantics
    - `parse_retry_after` parsing + `RateLimiter::acquire` sleep behavior +
      `send_with_retry` end-to-end 429 → retry → 200.
  - `commands::lark_auth` (28 new): set/get/clear round-trip, validation
    rejection per field, base_url default + custom + blank-as-unset,
    `load_lark_config` errors per missing field, `test_lark_connection`
    success + bad-creds against wiremock, `InMemorySecretStore` round-trip +
    idempotent delete, `KeyringStore` constructable smoke (no actual OS keyring
    call).
- **Rust integration**: 3 new smoke tests in `tests/lark_smoke.rs`, ignored by
  default, no-op skip when env unset.
- **Frontend**: 7 new IPC wrapper tests (set/get/test/clear) + 14 new
  `LarkSettings.svelte` component tests (form rendering, validation, pre-fill,
  Save/Test/Clear actions, banner kinds, error surfacing).

### Decisions worth pinning

- **`attachment_upload` skips retry helper**. Multipart `Form` is moved into
  `reqwest::RequestBuilder::multipart(form)` and consumed on `.send()`. The
  `Fn()` closure that `send_with_retry` requires would force us to clone bytes
  once per attempt. For potentially-large attachments that cost is punishing.
  Tradeoff: an attachment upload that hits 429 surfaces as an error instead of
  auto-retrying. We accept it because the rate limit acquire still happens
  first, so the only path that produces 429 here is a cross-process burst —
  re-do at the call site, not the transport layer.

- **`Retry-After` capped at 60s**. A misbehaving Lark response saying
  "Retry-After: 3600" must not pause the whole app for an hour. Capped + the
  test `parse_retry_after_caps_at_60_seconds` pins the behavior.

- **Smoke create+delete is opt-in via `LARK_SMOKE_FIELD`**. Even with real
  creds, we won't write to a user's table unless they explicitly name a field
  the test is allowed to populate. Keeps `cargo test -- --ignored` safe for
  read-only validation.

- **Secret never returned by `get_lark_status`**. Only `has_secret: bool`.
  TypeScript types in `src/lib/types.ts` deliberately omit a secret field so the
  IPC contract is enforced at compile time on the frontend too.

### Hard-rule audit

| Rule                                    | Where it lives                               |
| --------------------------------------- | -------------------------------------------- |
| Never log `app_secret`                  | `LarkConfig.Debug` redacts; no `tracing::*!` |
|                                         | call ever takes a `LarkConfig` field         |
| Never log `tenant_access_token`         | `fetch_tenant_token` returns the value but   |
|                                         | callers (commands::lark_auth, kanban sync    |
|                                         | TBD) bind it to `_token` and never print it  |
| `app_secret` → OS keyring               | `KeyringStore::set` only path that writes    |
| `app_secret` never crosses IPC outbound | `LarkStatus` has no secret field             |
| `.env` gitignored                       | Already in root `.gitignore`                 |

### Lessons

- **`$state<T>(initial)` over `let x: T = $state(initial)`**. Svelte 5 runes
  type inference picks up the second form as `typeof initial`, so writing
  `let status: LarkStatus | null = $state(null)` infers `status` as `null`
  forever. Use `$state<LarkStatus | null>(null)` so the type widens.
- **Per-attempt `Fn()` closure** is the cleanest retry pattern. Caller writes
  the request shape once, helper invokes it twice when needed. Only failure mode
  is "request body owns moveable state" — covered by the `attachment_upload`
  carve-out above.
- **Test-friendly `SecretStore` trait** is a small abstraction that pays back
  the price within the same PR. No keyring access in CI; the in-memory impl
  exercises every code path on the inner helpers.

### What's next (Phase 3a-2)

Kanban ↔ Bitable sync layer. With auth + transport sorted, 3a-2 wires
`bitable_list_records` to a TaskProvider that mirrors rows ↔ tasks, plus a small
reconciliation loop. Out of scope here.
