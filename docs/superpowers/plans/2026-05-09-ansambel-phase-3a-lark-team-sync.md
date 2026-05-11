# Phase 3a — Lark Bitable + Team Sync + Handoff

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

## Goal

Wire Ansambel ke Lark Bitable supaya workflow "AI Lembur" team-scale jalan:

- Task di Lark Bitable jadi source-of-truth — Ansambel hydrate kanban dari sana,
  bukan dari local task store yang kemarin.
- Tiap engineer install Ansambel di PC-nya sendiri. AI agent jalan lokal.
- Sync layer publish workspace state (status, last activity, diff summary, PR
  link) ke Bitable row task. Engineer lain di tim **lihat** progress real-time
  di sidebar "Team Activity" — read-only, tidak akses kode/conversation.
- Manual handoff: engineer A → klik "Hand off" → bundle (conversation history
  - uncommitted changes + todos) di-upload sebagai Lark attachment. Engineer B
    accept → unbundle ke local workspace, lanjut tanpa kehilangan konteks.

**Out of scope (defer ke phase berikutnya):**

- Jira integration → Phase 3b (kalau dibutuhkan; abstraksi `TaskProvider` di
  Phase 3a sudah disiapkan supaya plug-in)
- Scheduled auto-run (AI Lembur 24/7) → Phase 7-mini
- Headless daemon mode → Phase 9 (kalau diputuskan)
- Auto-PR submission → datang bareng Phase 7-mini

## Architecture

```
                           Lark Bitable (single source of truth)
                           ┌──────────────────────────────────┐
                           │ Table: ansambel_tasks            │
                           │ Columns:                         │
                           │  ─ task_id, title, description   │
                           │  ─ kanban_column                 │
                           │  ─ repo_id (← Phase 3a-2)        │
                           │  ─ assignee_machine              │
                           │  ─ ansambel_status               │
                           │  ─ last_activity_at              │
                           │  ─ last_message_preview          │
                           │  ─ pr_url                        │
                           │  ─ private (checkbox)            │
                           │  ─ blocked_question              │
                           │  ─ handoff_target                │
                           │  ─ handoff_bundle (attachment)   │
                           └────────┬─────────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
              ▼                     ▼                     ▼
  ┌───────────────────┐   ┌───────────────────┐   ┌───────────────────┐
  │ Engineer A's PC   │   │ Engineer B's PC   │   │ Engineer C's PC   │
  │                   │   │                   │   │                   │
  │ Ansambel desktop  │   │ Ansambel desktop  │   │ Ansambel desktop  │
  │  + Local AI agent │   │  + Local AI agent │   │  + (watcher only) │
  │  + Worktree       │   │  + Worktree       │   │                   │
  │  + messages.jsonl │   │  + messages.jsonl │   │                   │
  └───────────────────┘   └───────────────────┘   └───────────────────┘
```

Tiap Ansambel jadi penghubung Lark ↔ local execution. Tidak ada P2P antara PC
engineer — sync semua via Lark API.

**Visibility (strict per-repo scope):** engineer hanya lihat row Bitable yang
`repo_id`-nya ada di list repo lokal mereka. Engineer non-pemilik repo **tidak
melihat row sama sekali** — bahkan tidak tahu task itu exist via Team Activity
sidebar. Filter dilakukan client-side di
`src/lib/stores/team-activity.svelte.ts` (`row.repo_id ∈ self.local_repos`). Ini
UX-level enforcement: data tetap di Bitable, kalau engineer buka Lark web UI
langsung dia masih bisa lihat semua row. Asumsi: anggota Bitable table sudah
dalam circle of trust di level Lark workspace.

**Privacy escape:** `private` toggle per workspace (state tidak di-publish, row
Bitable existing di-clear). Per-table membership di Settings memberi boundary
tambahan (engineer cuma connect ke Bitable table yang relevan).

## Tech stack

Backend Rust:

- `reqwest` — sudah ada (HTTP client untuk Lark API)
- `tokio` — sudah ada (async runtime)
- `serde_json` — sudah ada
- `chrono` — sudah ada (timestamps)
- `tar` + `flate2` — **baru** (handoff bundle archive)

Frontend Svelte: tidak ada dep baru.

Auth: Lark Custom App credentials (`app_id` + `app_secret`) → tenant access
token. Disimpan di OS keyring lewat existing `platform/keyring.rs`.

## Bitable schema

Engineer yang setup Lark Custom App harus bikin satu Bitable table dengan
field-field ini:

| Nama field             | Tipe                                                                   | Wajib | Tujuan                   |
| ---------------------- | ---------------------------------------------------------------------- | ----- | ------------------------ |
| `task_id`              | Auto-number                                                            | ya    | Primary key              |
| `title`                | Text                                                                   | ya    | Display di kanban        |
| `description`          | Multi-line text                                                        | tidak | Detail task              |
| `kanban_column`        | Single-select (`todo`/`in_progress`/`review`/`done`)                   | ya    | Sumber kanban Ansambel   |
| `repo_id`              | Text                                                                   | ya    | Filter scope per-repo    |
| `priority`             | Single-select (`low`/`medium`/`high`)                                  | tidak | Sortir kanban            |
| `assignee_machine`     | Text (`handoko@laptop-1`)                                              | tidak | Siapa pegang sekarang    |
| `ansambel_status`      | Single-select (`idle`/`running`/`waiting`/`blocked`/`pr_ready`/`done`) | tidak | Phase 3a-2 state publish |
| `last_activity_at`     | Datetime                                                               | tidak | Phase 3a-2               |
| `last_message_preview` | Text (≤ 200 char)                                                      | tidak | Phase 3a-2               |
| `pr_url`               | URL                                                                    | tidak | Phase 3a-2               |
| `private`              | Checkbox                                                               | tidak | Phase 3a-7 escape        |
| `blocked_question`     | Text                                                                   | tidak | Phase 3a-6 surface       |
| `handoff_target`       | Text (`budi@laptop-2`)                                                 | tidak | Phase 3a-8               |
| `handoff_bundle`       | Attachment                                                             | tidak | Phase 3a-8 (≤ 50 MB)     |
| `handoff_at`           | Datetime                                                               | tidak | Phase 3a-8               |

Schema di-document di `docs/superpowers/specs/lark-bitable-schema.md`. Engineer
tim ngirim setup script `bun run setup-lark` untuk auto-create field via Lark
schema API (idempotent).

---

## File structure

```
src-tauri/src/
├── task_provider/
│   ├── mod.rs                                 # MODIFY: TaskProvider trait
│   ├── local.rs                               # KEEP: existing local store
│   └── lark.rs                                # CREATE: Lark Bitable plugin
├── commands/
│   ├── lark_auth.rs                           # CREATE: app credentials + token mgmt
│   ├── team_activity.rs                       # CREATE: subscribe/poll Bitable rows
│   ├── handoff.rs                             # CREATE: bundle + extract
│   └── ...
├── platform/
│   └── lark_client.rs                         # CREATE: typed Lark API client
└── state.rs                                   # MODIFY: add `task_provider: Box<dyn TaskProvider>`

src/lib/
├── ipc.ts                                     # MODIFY: api.lark.* + api.team.* + api.handoff.*
├── types.ts                                   # MODIFY: LarkAuthState, TeamActivity, Handoff*
├── stores/
│   ├── tasks.svelte.ts                        # MODIFY: switch from local to provider-driven
│   ├── team-activity.svelte.ts                # CREATE
│   └── team-activity.svelte.test.ts           # CREATE
├── components/
│   ├── settings/
│   │   ├── LarkSettings.svelte                # CREATE
│   │   ├── LarkSettings.test.ts               # CREATE
│   │   └── PrivacyToggle.svelte               # CREATE
│   ├── sidebar/
│   │   ├── TeamActivityPanel.svelte           # CREATE
│   │   ├── TeamActivityPanel.test.ts          # CREATE
│   │   └── TeamWorkspaceMirror.svelte         # CREATE: read-only watch view
│   └── workspace/
│       ├── WorkspaceView.svelte               # MODIFY: PrivacyToggle + Hand off button
│       ├── HandoffDialog.svelte               # CREATE
│       └── HandoffDialog.test.ts              # CREATE
└── stores/
    └── handoff.svelte.ts                      # CREATE
```

---

## Sub-phase breakdown

### 3a-1 — Lark API client + auth (P0, ~4 hari)

**Why:** Foundation untuk semua sub-phase berikutnya. Tanpa client typed + token
refresh, sub-phase lain tidak bisa hit Lark API.

**Backend:**

- [ ] `platform/lark_client.rs` — typed wrapper for Lark Open Platform API
  - [ ] `tenant_access_token()` — POST `/auth/v3/tenant_access_token/internal`,
        cache in-memory dengan refresh sebelum expire (1h 50min, jaga 10 menit
        margin)
  - [ ] `bitable_list_records(app_token, table_id, filter?)` — GET pagination
  - [ ] `bitable_create_record(app_token, table_id, fields)` — POST
  - [ ] `bitable_update_record(app_token, table_id, record_id, fields)` — PUT
  - [ ] `bitable_delete_record(app_token, table_id, record_id)` — DELETE
  - [ ] `attachment_upload(parent_node, bytes)` — POST
        drive/v1/medias/upload_all
  - [ ] `attachment_download(file_token)` — GET drive/v1/medias/{token}/download
  - [ ] `im_send_message(receive_id, msg_type, content)` — POST untuk Phase 3a-6
- [ ] Rate limit guard: max 200 req/min/app, queue with backoff
- [ ] Tests: mock HTTP server, verify request shape + retry behavior

**Backend (auth state):**

- [ ] `commands/lark_auth.rs`
  - [ ] `set_lark_credentials(app_id, app_secret, app_token, table_id)` — store
        di OS keyring + `repos.json`-style settings file
  - [ ] `get_lark_status()` — return
        `{ authenticated, table_name?, last_sync_at? }`
  - [ ] `clear_lark_credentials()` — wipe keyring entries
- [ ] Hard rule: never log app_secret atau access_token

**Frontend:**

- [ ] `src/lib/components/settings/LarkSettings.svelte` — form for `app_id`,
      `app_secret`, `app_token`, `table_id` + Test connection button
- [ ] `src/lib/ipc.ts` —
      `api.lark.{setCredentials, getStatus, testConnection,     clear}`
- [ ] Tests: setup form, auth round-trip with mock backend

**Tests (TDD order):**

- [ ] `tenant_token_cached_until_near_expiry`
- [ ] `tenant_token_refreshed_after_expiry`
- [ ] `bitable_list_returns_records`
- [ ] `bitable_create_assigns_record_id`
- [ ] `bitable_update_succeeds_with_partial_fields`
- [ ] `attachment_upload_returns_file_token`
- [ ] `attachment_download_returns_bytes`
- [ ] `rate_limit_exceeded_backs_off_then_retries`
- [ ] `network_error_surfaces_descriptive_message`
- [ ] `auth_state_persists_across_restart`

---

### 3a-2 — TaskProvider abstraction + Lark plugin (P0, ~3 hari)

**Why:** Decouple kanban store dari Lark — supaya Phase 3b (Jira) atau "local
mode" tetap support. Plus moving task source to Lark hydrate-on-startup shifts
source of truth.

**Backend:**

- [ ] `src-tauri/src/task_provider/mod.rs` — trait `TaskProvider`:
  - `fn list_tasks(repo_filter: Option<&str>) -> Result<Vec<Task>>`
  - `fn create_task(args: CreateTaskArgs) -> Result<Task>`
  - `fn update_task(id: &str, patch: TaskPatch) -> Result<()>`
  - `fn move_task(id: &str, column: KanbanColumn, order: i64) -> Result<()>`
  - `fn delete_task(id: &str) -> Result<()>`
- [ ] `task_provider/local.rs` — wrap existing `tasks.json` impl ke trait
- [ ] `task_provider/lark.rs` — Lark Bitable impl
  - `list_tasks` → `bitable_list_records` + map fields
  - `move_task` → update `kanban_column`
  - `delete_task` → `bitable_delete_record`
- [ ] `state.rs` — `AppState.task_provider: Box<dyn TaskProvider + Send>` —
      wired di `lib.rs` based on settings (`local` vs `lark`)
- [ ] Existing `commands/task.rs` di-refactor pakai trait, no breaking change

**Frontend:**

- [ ] `src/lib/stores/tasks.svelte.ts` — fetch via `api.task.list()` (sudah
      ada), tapi backend underneath swap ke Lark provider
- [ ] Settings panel: dropdown "Task source: Local / Lark Bitable"

**Tests:**

- [ ] `task_provider_local_round_trip` — sanity, no regression
- [ ] `task_provider_lark_list_maps_fields` — given mock Bitable response,
      assert Task struct populated correctly
- [ ] `task_provider_lark_move_updates_kanban_column`
- [ ] `task_provider_lark_create_returns_task_with_record_id`
- [ ] `task_provider_lark_delete_removes_row`
- [ ] `task_provider_lark_list_filters_by_repo`
- [ ] E2E: configure Lark → tasks muncul di kanban → move column → row di
      Bitable updated

---

### 3a-3 — Workspace state publisher (P0, ~4 hari)

**Why:** Inti sync — Ansambel publish workspace state ke Bitable row task secara
real-time supaya tim lain lihat siapa lagi kerja apa.

**Backend:**

- [ ] `commands/team_activity.rs` — `state_publisher` async task:
  - Subscribe ke event stream workspaces (status flips, message append, diff
    change, file save, PR creation)
  - Debounce: minimum 3 detik antar publish per workspace (rate limit jaga)
  - Map event → Bitable field update:
    - status flip → `ansambel_status`
    - message append (assistant) → `last_message_preview` (truncate 200 char,
      sanitize `sk-...`/`Bearer ...`/`eyJ...` jadi `[REDACTED]`)
    - file save → `last_activity_at`, regen `diff_summary` jadi
      `+45 -12 across 3 files`
    - PR creation → `pr_url` + `ansambel_status = pr_ready`
  - Skip kalau workspace `private = true` di Bitable
- [ ] `state.rs` — tambah `event_tx: broadcast::Sender<WorkspaceEvent>` ke
      AppState; existing handlers (terminal, agent, file_io) emit event
- [ ] Cleanup: kalau workspace di-set private, hapus existing publish data
      (`assignee_machine`, `ansambel_status`, dll set ke null)
- [ ] Cleanup on unmount/quit: skip — biarkan last state visible di Bitable
      sampai assignee aktif lagi

**Frontend:**

- [ ] Sanitizer: regex match common credential pattern di message preview
      sebelum di-publish (mirror backend sanitizer untuk safety lapis kedua)

**Tests:**

- [ ] `state_publisher_publishes_status_change`
- [ ] `state_publisher_debounces_rapid_updates`
- [ ] `state_publisher_truncates_long_messages_to_200_chars`
- [ ] `state_publisher_redacts_api_key_pattern`
- [ ] `state_publisher_redacts_bearer_token_pattern`
- [ ] `state_publisher_redacts_jwt_pattern`
- [ ] `state_publisher_skips_when_workspace_private`
- [ ] `state_publisher_clears_fields_when_set_to_private`
- [ ] `state_publisher_handles_lark_rate_limit_with_retry`
- [ ] `state_publisher_diff_summary_format`

---

### 3a-4 — Team Activity sidebar + watch view (P0, ~5 hari)

**Why:** Surface yang bikin sync layer terasa bagi engineer. "Saya buka sidebar,
langsung tau siapa lagi kerja apa di tim — terbatas hanya untuk repo yang saya
punya."

**Frontend:**

- [ ] `src/lib/stores/team-activity.svelte.ts` — fetch + cache Bitable rows
      dengan **strict client-side filter**:
  - `assignee_machine != self` (jangan tampilkan workspace milik sendiri di
    panel "team")
  - `row.repo_id ∈ repos.list().map(r => r.id)` — hanya tampil row dari repo
    yang ada di local Ansambel
  - Re-evaluate filter saat `repos` store berubah (add/remove repo) — Sidebar
    update real-time tanpa restart
  - Polling 5 detik (configurable)
- [ ] `src/lib/components/sidebar/TeamActivityPanel.svelte` — nested di Sidebar
      di bawah "WORKSPACES":
  - Group by `repo_id` (hanya repo yang engineer punya yang muncul sebagai group
    header)
  - Tiap row: status dot + title + assignee + last_activity_at "2m ago"
  - **Tidak ada filter dropdown** — scope sudah hard ke local_repos
  - Empty state kalau engineer tidak punya repo overlap: "No team activity in
    your repos right now."
- [ ] Klik row → buka `TeamWorkspaceMirror` (read-only). Karena semua row di
      sidebar guaranteed engineer punya repo-nya, tidak ada branching "punya" vs
      "tidak punya" — modal/"Add this repo" prompt dihapus.

**Frontend (mirror view):**

- [ ] `src/lib/components/sidebar/TeamWorkspaceMirror.svelte` — read-only panel:
  - Header: title + assignee + status pill
  - Conversation preview: last 5 messages dari Bitable preview field (cuma teks
    summary, bukan full chat)
  - Branch + diff summary
  - PR link kalau ada
  - **Tidak ada terminal, editor, atau live chat** — full content tetap di
    engineer A's machine
- [ ] Refresh button — re-fetch Bitable row

**Tests:**

- [ ] `team_activity_store_polls_bitable_every_5s`
- [ ] `team_activity_store_filters_self_machine`
- [ ] `team_activity_store_filters_to_local_repos_only`
- [ ] `team_activity_store_re_evaluates_when_repo_added`
- [ ] `team_activity_store_drops_rows_when_repo_removed`
- [ ] `team_activity_panel_groups_by_repo`
- [ ] `team_activity_panel_shows_empty_state_when_no_overlap`
- [ ] `team_activity_panel_click_row_opens_mirror`
- [ ] `mirror_renders_message_preview_from_bitable`
- [ ] `mirror_renders_pr_link_when_present`
- [ ] `mirror_refresh_button_re_fetches`

---

### 3a-5 — Task claim atomicity (P1, ~3 hari)

**Why:** Cegah dua engineer ambil task yang sama bersamaan (terutama relevant
saat Phase 7 autopilot nanti aktifin auto-pickup, tapi worth dilakukan sekarang
sebagai foundation).

**Backend:**

- [ ] Saat engineer "Start working" di task (move ke In Progress / spawn
      workspace dari kanban), Ansambel attempt CAS-style claim:
  1. `bitable_list_records` filter `task_id = X`
  2. Cek `assignee_machine == null` AND `kanban_column != in_progress`
  3. Kalau OK:
     `bitable_update_record(record_id, { assignee_machine: self,   kanban_column: in_progress })`
  4. Re-fetch row, verify `assignee_machine == self`
  5. Kalau iya — claim berhasil. Spawn workspace.
  6. Kalau tidak — task sudah di-claim orang lain. Surface "Task X already
     claimed by Y" toast.
- [ ] Lark Bitable tidak support native CAS — fallback ke check-then-write
      dengan immediate re-verify. Race window kecil (~100ms) tapi bukan zero.
      Untuk Phase 3a ini OK; Phase 7 autopilot bisa add server-side lock table
      kalau perlu lebih ketat.

**Frontend:**

- [ ] Surface "Already claimed" toast dengan Lark task link supaya engineer bisa
      lihat siapa yang grab duluan

**Tests:**

- [ ] `claim_succeeds_when_unassigned`
- [ ] `claim_fails_when_already_assigned`
- [ ] `claim_race_returns_loser_to_caller`
- [ ] `claim_release_clears_assignee_machine_on_workspace_close`

---

### 3a-6 — Block notification via Lark IM (P1, ~3 hari)

**Why:** Saat AI hit AskUserQuestion (datang dari Phase 4) — atau saat ada error
blocking — engineer offline di sebelah, tim lain perlu tau supaya bisa respon
atau take over.

**Catatan:** Phase 4 belum ship — AskUserQuestion belum ada. Phase 3a-6 ini ship
infra sekarang, integration ke AskUserQuestion event di Phase 4. Saat sub-phase
ini ship, trigger sementara: **manual "Mark blocked" button** + `error` agent
event.

**Backend:**

- [ ] Subscribe ke event `WorkspaceEvent::Blocked { workspace_id, reason }`
- [ ] On block:
  - Update Bitable row: `ansambel_status = blocked`, `blocked_question = reason`
  - Send Lark IM message:
    - Receive: `assignee_machine`'s registered user (config: machine → user_id
      mapping di settings)
    - Content: `🚫 Task "{title}" blocked: {reason}\n→ {ansambel_url}`
- [ ] Configurable: send ke channel tim juga (config: `team_chat_id` opsional)

**Frontend:**

- [ ] Settings: input `team_chat_id` (Lark group chat id) untuk broadcast block
      notif
- [ ] Settings: input `lark_user_id` per machine (default: current Lark user
      yang authenticated)
- [ ] Button manual "Mark blocked" di workspace header (untuk testing sebelum
      Phase 4 AskUserQuestion ada)

**Tests:**

- [ ] `block_event_updates_bitable_status_to_blocked`
- [ ] `block_event_sends_im_to_assignee_user`
- [ ] `block_event_broadcasts_to_team_chat_when_configured`
- [ ] `block_event_skipped_when_workspace_private`
- [ ] `manual_block_button_emits_block_event`
- [ ] `block_resolution_clears_blocked_question`

---

### 3a-7 — Settings + privacy controls (P0, ~2-3 hari)

**Why:** Per-workspace privacy escape hatch + Bitable table membership +
default-private toggle. Repo filter sudah hard di 3a-4 (tidak ada UI knob).

**Frontend:**

- [ ] `src/lib/components/settings/` — Settings panel grow:
  - Tab "Lark Integration" (sudah ada dari 3a-1) — extend dengan multi-table
    support: list of `{ app_token, table_id, name }` instead of single. Hanya
    table yang ke-add di sini yang Ansambel poll — boundary tambahan supaya
    engineer cuma terima data dari tim yang relevan.
  - Tab "Privacy" — defaults toggle:
    - "New workspaces are private by default" (default off)
    - "Sanitize regex patterns" — daftar regex extra (selain default secret
      patterns)
- [ ] `src/lib/components/workspace/PrivacyToggle.svelte` — ikon kunci di header
      workspace, klik → toggle Bitable `private`. Tooltip jelaskan "When
      private, no state is published to Lark"

**Backend:**

- [ ] `commands/team_activity.rs` —
      `set_workspace_private(workspace_id, private)` command, trigger publisher
      cleanup kalau dari false → true

**Tests:**

- [ ] `privacy_toggle_flips_bitable_private_field`
- [ ] `privacy_toggle_clears_publisher_state_when_enabled`
- [ ] `default_private_setting_applies_to_new_workspaces`
- [ ] `multi_table_settings_persists`
- [ ] `removing_table_from_settings_stops_polling_its_rows`

---

### 3a-8 — Handoff full-context bundle (P0, ~5 hari)

**Why:** Manual A→B handoff tanpa kehilangan konteks. Engineer A pulang →
engineer B (di shift sore atau besok) lanjutin tanpa AI "lupa" history.

**Backend:**

- [ ] `commands/handoff.rs`:
  - `create_handoff_bundle(workspace_id, target_machine: Option<String>)` →
    `Result<HandoffBundle>`:
    1. Validate: branch != main/master/develop, no `private = true`
    2. `git add . && git commit -m "WIP: handoff @{ws_id}"` (kalau ada
       uncommitted)
    3. `git push origin {branch}` — fail kalau no remote
    4. Bundle ke tarball:
       - `messages.jsonl` (existing file)
       - `wip.patch` = `git diff HEAD~1 HEAD --binary` kalau ada WIP commit
       - `untracked.tar` (file yang gitignored / belum di-add)
       - `todos.json` (kalau Phase 4 sudah ship; skip kalau belum)
       - `state.json` = workspace metadata
    5. gzip → `handoff-{ws_id}-{timestamp}.tar.gz`
    6. Upload ke Lark drive sebagai attachment
    7. Update Bitable row:
       - `handoff_target = {target_machine | "*"}`
       - `handoff_bundle = file_token`
       - `handoff_at = now`
       - `ansambel_status = pending_handoff`
       - `assignee_machine = null`
  - `accept_handoff(workspace_id, source_record_id)` → `Result<()>`:
    1. Validate: repo_id ada di local Ansambel (kalau tidak, return error "add
       repo first")
    2. Download attachment `handoff_bundle` ke temp dir
    3. Unzip + untar
    4. Verify state.json schema valid
    5. Atomic apply (semua atau tidak ada):
       - `git fetch origin {branch}`
       - `git checkout {branch}` (create new local branch tracking remote)
       - `git reset --soft HEAD~1` (undo WIP commit, working tree dapet changes
         back)
       - Apply `wip.patch` kalau ada
       - Untar `untracked.tar`
       - Copy `messages.jsonl` ke `<app_data>/messages/{new_ws_id}.json`
       - Copy `todos.json` kalau ada
    6. Create new workspace entry (new ws_id, same branch + repo)
    7. Update Bitable: `assignee_machine = self`, `ansambel_status = running`,
       clear `handoff_target`, `handoff_bundle`, `handoff_at`
- [ ] Bundle size cap: 50 MB. Kalau melewati, surface "Bundle too large,
      consider committing more aggressively before handoff" — defer S3-ish
      fallback.
- [ ] Atomic safety: temp dir, `tmp + rename` di setiap step, rollback kalau
      mid-apply gagal

**Frontend:**

- [ ] `src/lib/components/workspace/HandoffDialog.svelte` — modal "Hand off to":
  - Picker: pilih engineer dari kandidat yang **also has the repo lokal**. Cara
    deteksi: scan Bitable rows untuk `repo_id == self.workspace.repo_id` dan
    `assignee_machine != self`, collect distinct `assignee_machine` values.
    Tampilkan sebagai dropdown.
  - Pilihan tambahan "Anyone with this repo" (`*`) — kalau target match engineer
    manapun yang `repo_id`-nya overlap.
  - Kalau tidak ada kandidat (engineer tunggal dengan repo ini): surface warning
    "No teammate has this repo yet. Bundle will be uploaded but no one can
    accept — share repo first."
  - Optional message: "Continue from … please"
  - Confirm button → progress indicator → success / error
- [ ] Sidebar Team Activity — task dengan status `pending_handoff` munculin
      "Accept handoff" button kalau:
  - Engineer punya repo (`row.repo_id ∈ self.local_repos` — sudah dijamin karena
    strict scope di 3a-4)
  - Dan `handoff_target` match self machine atau `*`
- [ ] `src/lib/stores/handoff.svelte.ts` — manage bundle download progress,
      apply state

**Tests:**

- [ ] `bundle_creation_includes_all_required_files`
- [ ] `bundle_creation_pushes_branch_first`
- [ ] `bundle_creation_rejects_main_master_develop_branch`
- [ ] `bundle_creation_rejects_when_workspace_private`
- [ ] `bundle_creation_handles_no_uncommitted_changes_path`
- [ ] `bundle_creation_handles_only_uncommitted_no_untracked`
- [ ] `bundle_creation_handles_only_untracked`
- [ ] `bundle_size_cap_enforced_at_50mb`
- [ ] `handoff_picker_lists_only_engineers_with_same_repo`
- [ ] `handoff_picker_warns_when_no_candidate_exists`
- [ ] `accept_rejects_when_repo_not_added_locally`
- [ ] `accept_rolls_back_on_mid_apply_failure`
- [ ] `accept_creates_new_workspace_with_history_intact`
- [ ] `accept_clears_handoff_fields_in_bitable`
- [ ] `accept_handles_target_anyone_correctly`
- [ ] `accept_button_hidden_when_repo_not_local`
- [ ] `e2e_round_trip_a_to_b_preserves_conversation`
- [ ] `e2e_b_can_continue_chat_after_accept`

---

## Risks & mitigations

1. **Lark API rate limit 200 req/min/app** — Phase 3a-3 (state publisher) adalah
   konsumen terbesar. Mitigasi: debounce 3 detik per workspace, batch updates
   kalau possible (Lark `batch_update` endpoint). Worst case, 30-50 active
   workspaces semua bersamaan = ~600/min — break limit. Solusi: tambah tier
   debounce lebih ketat saat detect 429 + queue dengan exponential backoff.
   Defensive testing dengan stress test mock.

2. **Conversation history bundle size** — sebagian task running 5+ jam ngacir
   bisa balon ke 30-100 MB di `messages.jsonl`. 50 MB cap di 3a-8 menahan, tapi
   bisa friction-ful kalau task besar. Defer: S3-ish fallback di Phase 3a-9
   (out-of-scope sini). Sementara: docs jelas explain "kalau bundle gede, commit
   dulu / squash dulu sebelum handoff".

3. **Privacy leakage di message preview** — regex sanitizer bukan bulletproof.
   Custom credentials format atau API key embedded di code review chat bisa
   lolos. Mitigasi lapis: per-workspace `private` toggle (default off), dan
   default-private mode opsional di setting. Doc jelas peringatkan.

4. **Repo scope = UX-only, bukan data-level enforcement** — strict client-side
   filter di 3a-4 menyembunyikan row dari engineer non-pemilik repo di sidebar
   Ansambel. Tapi **data masih ada di Bitable** — kalau engineer non-pemilik
   buka Lark web UI / Lark mobile, dia tetap bisa lihat row tersebut beserta
   `last_message_preview`, `pr_url`, dll. Mitigasi:
   - Asumsi: anggota Bitable table sudah di-trust di level workspace Lark.
   - Per-table membership di Settings (3a-7) memberi second-line defense —
     engineer cuma poll table yang dia config.
   - Per-workspace `private` toggle untuk workspace sensitif.
   - Kalau perlu data-level enforcement: defer ke "Phase 3a-9 Bitable view
     filter" (out-of-scope sekarang) atau split one Bitable per repo
     (operational overhead).

5. **Bitable schema drift** — kalau engineer manual ubah field di Bitable
   (rename, hapus), Ansambel error. Mitigasi: setup script `bun run setup-lark`
   yang validate schema on startup, surface specific error "field X missing".

6. **Multi-machine identity collision** — `assignee_machine` format
   `user@hostname`. Kalau user pakai laptop A pagi + desktop B sore, 2 identity.
   Bisa confuse Team Activity. Mitigasi: settings input "machine alias" untuk
   manual override. Default: `{whoami}@{hostname}`.

7. **Handoff race** — A bundle, B accept bersamaan dengan A masih ngebundle.
   Mitigasi: status `pending_handoff` jadi gating — A baru set status itu
   setelah upload sukses. Sebelum itu state masih `running` (assigned to A). B
   hanya lihat "Accept" button setelah status `pending_handoff` muncul.

8. **PR auto-target main bypassing review** — Phase 3a TIDAK ship auto-PR (defer
   ke Phase 7-mini). Tapi state publisher publish `pr_url` yang engineer create
   manual via `gh pr create`. Manual flow → review tetap masuk. Hard-block
   direct push ke main BISA implement di sini (3a-8 sudah reject branch == main
   untuk handoff bundle creation).

9. **`git push` error saat handoff** — engineer tanpa internet, push gagal,
   bundle creation gagal. Mitigasi: surface clear error "push failed, fix
   network and retry". State Bitable tidak di-update sebelum push sukses.

---

## Test strategy

- **Unit tests**: Mock Lark API HTTP layer. Tiap public function di
  `lark_client.rs`, `team_activity.rs`, `handoff.rs` punya ≥3 test (happy + ≥2
  edge).
- **Integration tests**: Test database harness yang spawn fake Lark server
  (axum) di tempat random port, simulate full task → state publish → mirror view
  round trip.
- **E2E**: Update tauri-shim untuk handle Lark API mock; tambah spec
  `tests/e2e/phase-3a/team-sync.spec.ts`:
  - Configure Lark credentials → tasks muncul di kanban dari Bitable
  - Move task → Bitable row updated
  - Spawn workspace → state appear di Bitable
  - Privacy toggle → state cleared
  - Handoff bundle round-trip (mock dua Ansambel instance)

Coverage target: ≥ 95% (gate yang sudah ada).

---

## Migration & rollout

- **Feature flag**: `task_provider` setting di `app_settings.json` — default
  `local` (existing behavior). Switch ke `lark` opt-in via Settings.
- **Local mode masih jalan**: engineer yang belum ready Lark bisa stay di local
  mode. Phase 3a-2 trait abstraction memastikan dua mode coexist.
- **Bitable schema setup**: dokumentasikan di
  `docs/superpowers/specs/lark-bitable-schema.md`. Provide CLI helper
  `bun run setup-lark` (Tauri command) yang create field via Lark schema API.
- **Backwards compat**: existing local tasks tidak auto-migrate ke Lark — user
  explicit "Import to Lark" button kalau mau. Cegah accidental dump.

---

## Effort estimate

| Sub-phase                                   | Effort | Priority |
| ------------------------------------------- | ------ | -------- |
| 3a-1 Lark API client + auth                 | 4 hari | P0       |
| 3a-2 TaskProvider abstraction + Lark plugin | 3 hari | P0       |
| 3a-3 Workspace state publisher              | 4 hari | P0       |
| 3a-4 Team Activity sidebar + watch view     | 5 hari | P0       |
| 3a-5 Task claim atomicity                   | 3 hari | P1       |
| 3a-6 Block notification via Lark IM         | 3 hari | P1       |
| 3a-7 Settings + privacy controls            | 3 hari | P0       |
| 3a-8 Handoff full-context bundle            | 5 hari | P0       |

**Total:** ~30 hari kerja = **6 minggu solo**, atau ~4 minggu kalau P1 (3a-5 +
3a-6) di-defer ke Phase 3a-revision.

**Critical path** (P0 only): ~24 hari = 5 minggu.

---

## Checklist (high level)

- [ ] 3a-1 Lark API client + auth shipped (≥ 95% coverage)
- [ ] 3a-2 TaskProvider abstraction + Lark plugin shipped
- [ ] 3a-3 Workspace state publisher shipped
- [ ] 3a-4 Team Activity sidebar + watch view shipped
- [ ] 3a-5 Task claim atomicity shipped (P1)
- [ ] 3a-6 Block notification shipped (P1, infra-only — full integration di
      Phase 4)
- [ ] 3a-7 Settings + privacy controls shipped
- [ ] 3a-8 Handoff bundle round-trip shipped
- [ ] Bitable schema setup script `bun run setup-lark` ships dengan docs
- [ ] E2E spec passes on Ubuntu + Windows runners
- [ ] Coverage on changed files ≥ 95%
- [ ] Journal entry describes the sub-phase
- [ ] PR opened against `main`
