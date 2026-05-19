# Phase 3a-3 — Workspace State Publisher Design

**Date:** 2026-05-19 **Supersedes scope of:** Original "3a-3 Workspace state
publisher" in
`docs/superpowers/plans/2026-05-09-ansambel-phase-3a-lark-team-sync.md`. The
original plan assumed a single Bitable table holding both task data AND
team-sync state. After the per-repo binding work (Phase 3a-3 in the merged
numbering, which is the renamed/re-scoped 3a-3-binding) and the filter-aware
3a-3.1, that conflation no longer fits. This spec re-frames the publisher around
a **separate, dedicated team-activity table**.

## Goal

Publish each active workspace's state to a shared Lark Bitable in near real-
time so team members can see "who is working on what right now" without
accessing the code or conversation history.

The reads (3a-4 Team Activity sidebar + watch view) ship in a follow-up phase.
This spec is publisher-only.

## Non-goals

- **Reading** activity (sidebar / mirror view): out of scope; Phase 3a-4.
- **Handoff bundles**: out of scope; Phase 3a-8.
- **Per-task Lark sync writeback**: the per-repo task bindings (3a-3) remain
  read-only; this spec adds a _separate_ write surface, it does not change the
  existing task binding behaviour.
- **Conflict resolution between concurrent publishers**: the row is keyed by the
  local `workspace_id` (`ws_*` ULID) which is globally unique per machine, so
  two machines can never write to the same row.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Lark Bitable: ansambel_team_activity (one table, configured globally        │
│ per Ansambel install at Settings → Team Activity)                            │
│                                                                              │
│ Columns (fixed schema, owned by Ansambel):                                   │
│  - workspace_id           Text (primary)                                     │
│  - repo_remote_url        Text       (canonical cross-machine identifier)    │
│  - repo_display_name      Text                                               │
│  - task_title             Text                                               │
│  - assignee_machine       Text       ("handoko@laptop-1")                    │
│  - ansambel_status        SingleSelect (idle/running/waiting/                │
│                                         blocked/pr_ready/done)               │
│  - last_activity_at       Datetime   (epoch ms)                              │
│  - last_message_preview   Text       (≤200 char, sanitised)                  │
│  - branch_name            Text                                               │
│  - diff_summary           Text       ("+45 -12 across 3 files")              │
│  - pr_url                 URL                                                │
│  - private                Checkbox                                           │
└──────────────────────────────────────────────────────────────────────────────┘
            ▲                       ▲                       ▲
            │                       │                       │
    ┌───────┴────────┐      ┌───────┴────────┐      ┌───────┴────────┐
    │ Engineer A     │      │ Engineer B     │      │ Engineer C     │
    │ Ansambel       │      │ Ansambel       │      │ Ansambel       │
    │ state_publisher│      │ state_publisher│      │ state_publisher│
    │ (this spec)    │      │                │      │                │
    └────────────────┘      └────────────────┘      └────────────────┘
```

Each Ansambel install runs one `state_publisher` async task that subscribes to
workspace events on the local machine and writes/updates a single row per
workspace in the shared team-activity table.

## Configuration

A new global config (NOT per-repo) lives at
`<data_dir>/team_activity_config.json`:

```json
{
  "app_token": "bascntest...",
  "table_id": "tblTeamActivity...",
  "machine_label": "handoko@laptop-1"
}
```

- **Credentials:** reuse the existing global `app_id` + `app_secret` from Phase
  3a-1 (`platform/keyring.rs` + `commands/lark_auth.rs`). No new auth.
- **app_token + table_id:** stored in the config file; Settings UI lets the user
  paste them.
- **machine_label:** auto-generated on first launch from `$USER@$(hostname)` (or
  `$USERNAME@COMPUTERNAME` on Windows); user can edit in Settings.

If the file is absent or `app_token` is empty, the publisher is **disabled** and
emits no traffic. This is the offline / solo-developer default.

## Canonical repo identifier

Original plan glossed over a cross-machine identity problem: each Ansambel
install generates its own local `repo_id` (`repo_*`) when the user adds a repo,
so engineer A's `repo_abc` and engineer B's `repo_xyz` for the same git repo do
not match. The team-activity table needs a stable identifier that agrees across
machines.

**Approach.** Use `git remote get-url origin` as the canonical key. For each
workspace, before publishing, the publisher runs
`git -C <repo_path> remote get-url origin`, normalises the URL (strip trailing
`.git`, lowercase host), and uses the result as `repo_remote_url`. Repos with no
`origin` remote publish with `repo_remote_url = ""` and are surfaced only on the
local engineer's machine.

Cached on the `Repo` struct after first lookup (refreshed when `repo.path`
changes).

The 3a-4 sidebar filter (next phase) will use this for the
`row.repo_remote_url ∈ self.local_repos.map(canonical)` filter.

## Event flow

```
                              broadcast::Sender<WorkspaceEvent>
                                      │
   ┌──────────────────┬──────────────────┬───────────────────┐
   │                  │                  │                   │
   ▼                  ▼                  ▼                   ▼
 agent core      file_io save      workspace status     pr commands
 (status flip,   (touch                                  (after create)
  message,       last_activity_at)
  thinking)
                                      │
                                      ▼
                         ┌──────────────────────────┐
                         │ state_publisher task     │
                         │  - debounce per workspace│
                         │  - sanitise preview      │
                         │  - upsert Bitable row    │
                         └──────────────────────────┘
                                      │
                                      ▼
                              LarkClient (existing,
                              new bitable_upsert method)
```

### Events

```rust
pub enum WorkspaceEvent {
    StatusChanged { workspace_id, new_status },
    MessageAppended { workspace_id, role, text_preview },
    FileTouched { workspace_id },
    PrCreated { workspace_id, url },
    BranchChanged { workspace_id, branch_name },
    DiffSummaryUpdated { workspace_id, summary },
    PrivacyChanged { workspace_id, is_private },
}
```

### Emission points

Existing handlers add `event_tx.send(...)` calls. No new infrastructure beyond a
`broadcast::Sender<WorkspaceEvent>` registered in `AppState`.

- `commands/agent_core.rs`:
  - status transitions (`Running`/`Waiting`/`Error`/`Stopped`) → `StatusChanged`
  - new assistant message persisted → `MessageAppended`
- `commands/file_io.rs` (or wherever the editor writes go):
  - on `file_write` succeeding → `FileTouched`
- `commands/workspace.rs`:
  - `pr_create` success → `PrCreated`
  - branch rename → `BranchChanged`
- `commands/git.rs`:
  - after a commit/checkout → `DiffSummaryUpdated`
- Settings toggle in `TeamActivitySettings.svelte`:
  - private flag flips → `PrivacyChanged`

### Publisher loop

```rust
loop {
    let event = rx.recv().await?;
    let ws_id = event.workspace_id();
    let pending = aggregated_state.entry(ws_id).or_default();
    pending.merge(event);
    let now = Instant::now();
    let last_sent = last_sent_at.get(&ws_id).copied().unwrap_or(EPOCH);
    if now.duration_since(last_sent) >= Duration::from_secs(3) {
        publisher.upsert(pending.snapshot()).await?;
        last_sent_at.insert(ws_id, now);
        aggregated_state.remove(&ws_id);
    } else {
        schedule_flush(ws_id, last_sent + 3s);
    }
}
```

Per-workspace 3-second debounce. Aggregates multiple events into a single upsert
so a 50-message-burst conversation only fires one Bitable write.

## Sanitisation

`last_message_preview` runs through a sanitiser before publish:

- `sk-[A-Za-z0-9]{20,}` → `[REDACTED-API-KEY]`
- `Bearer [A-Za-z0-9._\-]+` → `Bearer [REDACTED]`
- `eyJ[A-Za-z0-9._\-]{20,}` → `[REDACTED-JWT]`
- `(?i)(api[_-]?key|secret|token)\s*[:=]\s*\S+` → `$1: [REDACTED]`
- After redaction, truncate to 200 chars + ellipsis if longer.

The sanitiser also runs frontend-side (in `src/lib/sanitize.ts`) before any
preview lands in `messages.jsonl`, so the backend regex is a second line of
defence. (Two-layer redaction is intentional — the user gave explicit feedback
during 3a-1 that redaction failure should be impossible to silently slip past,
so each layer is auditable.)

## Privacy escape

`Repo` gains a `team_activity_private: bool` field (default false). When the
user toggles "private" for a workspace, the publisher:

1. Sends a `PrivacyChanged { is_private: true }` event.
2. Issues one upsert that **clears** sensitive columns (`assignee_machine`,
   `ansambel_status`, `last_message_preview`, `branch_name`, `diff_summary`,
   `pr_url` → null) but leaves the row intact with `private = true`.
3. Stops publishing further events for that workspace.

Toggling private off resumes publishing on the next event.

## Rate limit awareness

Lark Bitable: 200 req/min per app. With per-workspace 3-second debounce and a
conservative upper bound of 10 simultaneously active workspaces per engineer,
peak throughput is 10 × 20 req/min = 200 req/min — exactly at the ceiling. The
publisher implements:

- 429 handling: parse `Retry-After`, sleep, retry once. After retry failure,
  log + drop the publish (next event triggers another attempt).
- Token bucket gate in front of the upsert call (already exists in
  `LarkClient`).

## Backend module layout

```
src-tauri/src/commands/team_activity.rs        # this spec's main module
src-tauri/src/state.rs                         # + event_tx, + team_activity_config
src-tauri/src/persistence/team_activity_config.rs   # atomic config rw
src-tauri/src/platform/lark_client.rs          # + bitable_upsert_row (extend)
src-tauri/src/sanitize.rs                      # message preview redactor (new)
src/lib/components/lark/TeamActivitySettings.svelte # settings UI (new)
src/lib/sanitize.ts                            # frontend mirror redactor
```

## Test plan

Unit (Rust):

- `publisher_publishes_status_change_to_bitable`
- `publisher_debounces_rapid_updates_to_one_call_per_3s`
- `publisher_truncates_long_messages_to_200_chars`
- `publisher_redacts_api_key_pattern`
- `publisher_redacts_bearer_token_pattern`
- `publisher_redacts_jwt_pattern`
- `publisher_redacts_named_credential_patterns`
- `publisher_skips_publish_when_workspace_private`
- `publisher_clears_fields_on_private_toggle`
- `publisher_retries_on_429_with_retry_after`
- `publisher_drops_publish_after_retry_failure`
- `publisher_handles_disabled_config_silently`
- `canonical_repo_url_strips_dot_git`
- `canonical_repo_url_lowercases_host`
- `canonical_repo_url_returns_empty_when_no_origin`

Frontend (vitest):

- `TeamActivitySettings_renders_disabled_state_when_config_missing`
- `TeamActivitySettings_saves_app_token_table_id_machine_label`
- `TeamActivitySettings_private_toggle_emits_privacy_event`
- `sanitize_redacts_api_key_in_message_preview`

E2E: defer to 3a-4 (when sidebar exists to assert against).

## Risks & open questions

- **Schema migration on Bitable.** If we change the table schema later, every
  team member's table is out of sync. Mitigation: ship a
  `bun run setup-lark --team-activity` script that idempotently `ensure`s the
  columns exist via Lark schema API. Document the manual fallback in the
  settings UI.
- **Clock skew between machines.** `last_activity_at` is set to local epoch ms.
  If a machine's clock is wildly off, the row's "5m ago" sidebar tooltip in 3a-4
  will read wrong. Acceptable: this is descriptive, not security.
- **Machine label collisions.** Two engineers named "handoko" on machines both
  called "laptop" would collide on `assignee_machine`. The label is
  user-editable in settings; collisions are surfaced when the user notices two
  rows with the same label.

## Out-of-scope follow-ups

- Sidebar + watch view (Phase 3a-4)
- Block notification via Lark IM (Phase 3a-6)
- Settings UI for per-table membership (already implicit in this spec's config
  file; a richer multi-table UI is Phase 3a-7)
- Handoff bundles (Phase 3a-8)
