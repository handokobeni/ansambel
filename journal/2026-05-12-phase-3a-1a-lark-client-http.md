# Journal — 2026-05-12 — Phase 3a-1a Lark API client (HTTP)

## PR: feat(phase-3a-1a) — Lark API client (HTTP)

**Branch:** `feat/phase-3a-1a-lark-client-http` **Author:** handokobeni
**Phase:** 3a-1 (split 1 of 2)

### Summary

Slice pertama Phase 3a-1: typed HTTP wrapper untuk Lark/Feishu Open Platform
yang nanti dipakai semua sub-phase Phase 3a berikutnya (Bitable sync, attachment
handoff, IM notification). Pure library layer — belum ada Tauri command, belum
ada UI, belum disuntik ke TaskProvider. Sengaja di-split jadi PR sendiri supaya
review unit fokus ke "HTTP correctness" tanpa ke-mix sama keyring persistence /
frontend wiring.

### Yang shipped

- **`src-tauri/src/platform/lark_client.rs`** — module baru, ~700 LOC
  production + ~600 LOC tests
- **7 HTTP endpoint** typed:
  - `tenant_access_token()` — internal auth dengan cache + 10-menit margin
    refresh
  - `bitable_list_records()` — auto-paginate, filter passthrough, hard cap
    100×500 records
  - `bitable_create_record()` — POST + return record_id
  - `bitable_update_record()` — PUT partial fields
  - `bitable_delete_record()` — DELETE by record_id
  - `attachment_upload()` — multipart POST → file_token
  - `attachment_download()` — GET raw bytes
  - `im_send_message()` — POST dengan receive_id_type query param
- **Error surface**: `AppError::Lark(String)` variant baru; setiap method bedain
  HTTP-level error (5xx, network) vs Lark-protocol error (non-zero `code` field)
- **Security**: `LarkConfig.Debug` impl redact `app_secret`. Hard rule "never
  log app_secret" enforced di code review nanti.

### Deps baru

- **reqwest 0.12** (rustls-tls + json + multipart features) — production HTTP
- **wiremock 0.6** (dev-dep) — mock HTTP server untuk unit test

### Tests (25 new, all green pada Linux + Windows + macOS)

Token + cache (8):

- `should_use_cached_*` × 3 — pure cache logic, no HTTP
- `lark_config_debug_redacts_app_secret` — security guard
- `tenant_token_fetches_and_returns_value` — happy path
- `tenant_token_cached_until_near_expiry` — verify cache (1 HTTP call)
- `tenant_token_refreshed_after_expiry` — verify refresh (2 HTTP calls)
- `tenant_token_surfaces_non_zero_code_as_error` — protocol error

Bitable CRUD (8):

- `bitable_list_returns_records_in_one_page`
- `bitable_list_auto_paginates`
- `bitable_list_passes_filter_query`
- `bitable_list_surfaces_non_zero_code_as_error`
- `bitable_create_assigns_record_id`
- `bitable_create_missing_record_in_response_is_error`
- `bitable_update_succeeds_with_partial_fields`
- `bitable_delete_succeeds`
- `bitable_delete_surfaces_non_zero_code`

Drive (4):

- `attachment_upload_returns_file_token`
- `attachment_upload_empty_file_token_is_error`
- `attachment_download_returns_bytes`
- `attachment_download_surfaces_404`

IM (2):

- `im_send_message_returns_message_id`
- `im_send_message_surfaces_non_zero_code`

Network (3):

- `tenant_token_surfaces_network_error` (connect refused)
- `tenant_token_surfaces_http_5xx_as_error`
- (+ implicit retry via wiremock's expectation count)

Total lib tests: 409 → 434.

### Decision: split jadi 3a-1a + 3a-1b

Plan doc 3a-1 originally satu unit (~4 hari) yang covered:

1. HTTP client + auth (this PR)
2. Rate limit guard (200 req/min) + 429 retry
3. `commands/lark_auth.rs` — keyring + persistence + test_connection
4. Frontend `LarkSettings.svelte`
5. Smoke test helper

Push state sekarang udah ~1300 LOC. Phase 2 PR rata-rata ~600 LOC. Split
menghindari "wall of code" review fatigue. Split point natural: HTTP correctness
(testable dengan wiremock saja) vs integration (keyring + Tauri command + Svelte
UI yang butuh manual test).

**3a-1a (this PR):** Pure HTTP client + tests. No app integration. **3a-1b (next
PR):** Rate limit + auth commands + frontend + smoke helper. Will hit real Lark
API in manual smoke test before merge.

### Yang belum di-test (intentional defer ke 3a-1b)

- **Rate limit** — token bucket guard belum implement. Lark hard cap 200
  req/min/app. Risk: belum bocor di test karena unit test tidak trigger volume
  itu. Add di 3a-1b.
- **429 retry** — Lark return 429 dengan Retry-After header. Belum handle. Add
  di 3a-1b.
- **Bitable extra params for attachment** — Bitable attachment download butuh
  `extra={"bitablePerm":{"tableId":"...","rev":...}}` query param.
  `attachment_download` versi sekarang cuma cocok untuk non-Bitable media. 3a-2
  wraps dengan param yang tepat.

### Lessons

- **`Debug` impl + redaction lebih aman dari std derive** — secret fields tidak
  boleh nyangkut di error log atau panic message. Test
  `lark_config_debug_redacts_app_secret` jadi regression guard.
- **wiremock + `.expect(N)`** — clean way to assert "no extra HTTP call" tanpa
  manual verify counter. Lebih readable dari `Arc<AtomicUsize>`.
- **`base_url` configurable** — satu trick simple bikin entire HTTP layer
  testable tanpa lift-shifting ke trait abstraction.
- **Truncate response body in error messages** — production Lark error response
  bisa panjang (HTML error page, dll). 200-char cap + `…` suffix bikin error log
  readable.

### Sisa untuk close 3a-1

Di PR berikutnya (`feat/phase-3a-1b-...`):

- [ ] Rate limit guard (token bucket di LarkClient internal)
- [ ] 429 retry dengan exponential backoff (1 attempt max)
- [ ] `commands/lark_auth.rs` — set/get/clear credentials, test_connection
- [ ] Keyring storage untuk app_secret + JSON persistence untuk non- secret
      config
- [ ] Frontend `LarkSettings.svelte` + IPC wrappers
- [ ] Smoke test script (`scripts/smoke-lark.ts` atau cargo test feature flag)
- [ ] Manual smoke test pakai real Lark credentials user
- [ ] Journal `2026-05-XX-phase-3a-1b-...`
