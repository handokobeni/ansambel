# Phase 3a-4 — Team Activity Sidebar + Watch View Design

**Date:** 2026-05-19 **Supersedes scope of:** Original "3a-4 Team Activity
sidebar + watch view" in
`docs/superpowers/plans/2026-05-09-ansambel-phase-3a-lark-team-sync.md`. The
original plan was sketched before Phase 3a-3 settled on a separate team-
activity Bitable table and before Phase 3a-3.1 introduced canonical
`repo_remote_url` as the cross-machine key. This spec re-frames the read side
around the schema 3a-3 actually publishes, and re-uses the FilterSpec
infrastructure 3a-3.1 already built.

## Goal

Surface "who on the team is working on what right now" inside Ansambel so a team
member can sense team activity without leaving the app — without giving up the
strict per-repo privacy boundary established in 3a-3.

Two surfaces:

- **Sidebar panel**: always-visible collapsible section below WORKSPACES that
  lists active team workspaces for repos the user has locally.
- **Mirror view (watch view)**: read-only detail screen for one teammate's
  workspace — status, branch, last message preview, GitHub branch link, PR link
  when ready.

## Non-goals

- **Notifications** when teammate status changes (`blocked`, `pr_ready`) — Phase
  3a-6 (Lark IM ping).
- **Bi-directional state** (e.g., commenting on a teammate's workspace, pinging
  them, claiming a workspace) — out of scope; the surface is strictly read-only.
- **Conversation history mirror** — only the latest `last_message_preview` (200
  chars, sanitised) is shown. Phase 3a-8 (handoff bundles) is the right place
  for full conversation visibility.
- **Real-time push** — Lark Bitable has no webhook, so polling is the only
  option. A future Phase that swaps the data layer (e.g., a backend that exposes
  its own websocket) could replace polling without changing this spec's UI
  contract.
- **Configurable polling interval in UI** — fixed 10s with visibility-aware
  pause. Manual Refresh button covers the urgency case.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Lark Bitable: ansambel_team_activity (same table 3a-3 publisher writes to)   │
└─────────────────────────────────┬────────────────────────────────────────────┘
                                  │  POST /records/search
                                  │  every 10s when document.visibilityState === 'visible'
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ Rust: api.teamActivity.fetchRows() IPC (no args)                             │
│   - builds FilterSpec internally from AppState.repos + config.machine_label  │
│     (canonical remote URL cache reused from 3a-3 enricher)                   │
│   - thin wrapper around platform::lark_client::bitable_search_records        │
│     (existing, from 3a-3.1)                                                  │
│   - returns FetchResult enum:                                                │
│       Disabled | MachineLabelEmpty | NoOverlapRepos | Rows { rows }          │
└─────────────────────────────────┬────────────────────────────────────────────┘
                                  │  FetchResult
                                  ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ Frontend: src/lib/stores/team-activity.svelte.ts                             │
│   - SvelteMap<workspace_id, TeamActivityRow>                                 │
│   - poll loop: setInterval(10_000) + visibilitychange listener               │
│   - status: 'idle' | 'loading' | 'error' | 'disabled' |                      │
│             'machine_label_empty' | 'no_overlap_repos'                       │
│   - on tick: call fetchRows(), match FetchResult, reconcile                  │
│   - selectedWorkspaceId state for mirror view routing                        │
└──────┬─────────────────────────────────────────────────┬─────────────────────┘
       │                                                 │
       ▼                                                 ▼
┌────────────────────────────┐                ┌─────────────────────────────────┐
│ TeamActivityPanel.svelte   │                │ TeamWorkspaceMirror.svelte      │
│  - Collapsible Sidebar     │                │  - Replaces main content area   │
│    section below WORKSPACES│                │    when selectedWorkspaceId set │
│  - Group by repo, status   │                │  - Header + Code state +        │
│    dot, relative time      │                │    Latest activity sections     │
│  - Refresh + empty/error   │                │  - GitHub branch link + PR link │
└────────────────────────────┘                └─────────────────────────────────┘
```

Each Ansambel install runs one read-side poll loop in the frontend store,
mirroring the publisher's write-side loop in the backend.

## Configuration

Reuses the existing global `team_activity_config.json` (from 3a-3) without
changes. The publisher and the reader both consume it:

- `app_token` + `table_id` — which Bitable table to read from.
- `machine_label` — used in the `assignee_machine != self` filter condition.

If the file is absent or `app_token` is empty, the reader is disabled (no
polling, panel renders a "configure in Settings" hint). Same offline / solo-
developer fallback as the publisher.

## Filter construction

Filter construction lives in the backend, not the frontend. Two reasons:

1. The canonical `repo_remote_url` for each repo is computed by Rust
   (`platform::repo_identity::read_origin_url` + `canonicalise_remote_url`). The
   3a-3 publisher's enricher already caches these per repo. Surfacing the
   canonical URL onto the frontend `Repo` type just to rebuild the filter in JS
   would mean either (a) adding a serialised field that trails the cache, or (b)
   an extra IPC round-trip per poll. Both are strictly worse than letting the
   command consult AppState directly.
2. The IPC contract stays simple: frontend calls `api.teamActivity.fetchRows()`
   with no arguments. Backend handles "what counts as my workspace" + "what
   counts as my repos" itself.

```rust
// Pseudocode for fetch_team_activity_rows
async fn fetch_team_activity_rows(state, lark_client) -> Result<FetchResult> {
    let cfg = load_team_activity_config(...)?;
    let cfg = match cfg {
        Some(c) if !c.app_token.is_empty() => c,
        _ => return Ok(FetchResult::Disabled),
    };
    if cfg.machine_label.is_empty() {
        return Ok(FetchResult::MachineLabelEmpty);
    }

    let remote_urls: Vec<String> = state.lock()
        .repos.values()
        .filter_map(|repo| read_origin_url(&repo.path).ok())
        .map(|raw| canonicalise_remote_url(&raw))
        .filter(|url| !url.is_empty())
        .collect();
    if remote_urls.is_empty() {
        return Ok(FetchResult::NoOverlapRepos);
    }

    let filter = FilterSpec {
        conjunction: And,
        conditions: vec![
            FilterCondition { field_name: "assignee_machine", operator: IsNotEmpty, value: vec![] },
            FilterCondition { field_name: "assignee_machine", operator: IsNot,      value: vec![cfg.machine_label] },
            FilterCondition { field_name: "repo_remote_url",  operator: Is,         value: remote_urls },
        ],
    };
    let records = lark_client.bitable_search_records(&cfg.app_token, &cfg.table_id, &filter).await?;
    Ok(FetchResult::Rows(records.into_iter().map(parse_record_to_row).collect()))
}
```

Three filter conditions, all using operators that already work in the 3a-3.1
FilterSpec wire path:

- `assignee_machine isNotEmpty` excludes private rows implicitly. The 3a-3
  publisher's privacy escape clears `assignee_machine` (and other sensitive
  columns) when the user toggles a workspace private, so an empty
  `assignee_machine` is the canonical "should not appear in sidebar" signal. We
  avoid a direct `private isNot true` checkbox filter because Lark's filter
  operator for boolean checkboxes is documented inconsistently; routing through
  the always-cleared text field is more robust.
- `assignee_machine isNot <self>` excludes the user's own workspaces.
- `repo_remote_url is [...remoteUrls]` is Lark's IN-style match: with `is`
  - multi-value the filter is satisfied if the cell matches any value
    (OR-of-values). The canonicalisation pipeline matches what the publisher
    writes, so the equality holds across machines.

`field_id` left empty because the reader uses field names verbatim (the wire DTO
already strips `field_id` for the publisher's writes; the reader's filter takes
the same path).

The Rust command returns a `FetchResult` enum:

```rust
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FetchResult {
    Disabled,             // team_activity_config absent or app_token empty
    MachineLabelEmpty,    // config set but machine_label not configured yet
    NoOverlapRepos,       // user has no local repos with an origin remote
    Rows { rows: Vec<TeamActivityRow> },  // success (rows may be empty)
}
```

Tagged so the frontend can pattern-match without extra ceremony. The store maps
`Disabled` → `status='disabled'`, `MachineLabelEmpty` → distinct hint state,
`NoOverlapRepos` → empty-state copy, `Rows` → reconcile.

## Polling lifecycle

```
Sidebar mount → teamActivity.start()
  - immediate first fetch (don't wait 10s on app open)
  - setInterval(10_000, tick)
  - addEventListener('visibilitychange', onVisible)

tick():
  - if document.visibilityState !== 'visible' → return (no-op, keep timer)
  - if inflight → return (skip overlapping ticks)
  - inflight = true
  - try:  result = await api.teamActivity.fetchRows()
      switch (result.kind):
        'disabled':              status = 'disabled';            reconcile([])
        'machine_label_empty':   status = 'machine_label_empty'; reconcile([])
        'no_overlap_repos':      status = 'no_overlap_repos';    reconcile([])
        'rows':                  status = 'idle';                reconcile(result.rows)
  - catch 429: backoff Retry-After (default 2s), retry once
  - catch network/other: log warn, keep prior rows, status = 'error'
  - finally: inflight = false

onVisible():
  - if document.visibilityState === 'visible': trigger immediate tick (don't wait
    for next interval — surfaces stale data fast when user returns)

Sidebar unmount → teamActivity.stop()
  - clearInterval
  - removeEventListener('visibilitychange', ...)
```

Reconciliation:

```typescript
function reconcile(rows: TeamActivityRow[]): void {
  const newIds = new SvelteSet(rows.map((r) => r.workspace_id));
  for (const r of rows) store.rows.set(r.workspace_id, r);
  for (const id of [...store.rows.keys()]) {
    if (!newIds.has(id)) store.rows.delete(id);
  }
  // If the user's selection just vanished (teammate went private or finished),
  // auto-close the mirror view with a toast.
  if (store.selectedWorkspaceId && !newIds.has(store.selectedWorkspaceId)) {
    store.selectedWorkspaceId = null;
    addToast('Team workspace closed by teammate', 'info', 4000);
  }
}
```

## Components

### Backend (Rust) — `commands/team_activity.rs` (extend)

```rust
#[tauri::command]
pub async fn fetch_team_activity_rows(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    data_dir: ...,           // resolved via app handle
) -> Result<FetchResult, String> {
    // Load team_activity_config (same loader the publisher uses at startup).
    // If config disabled → return FetchResult::Disabled
    // If machine_label empty → return FetchResult::MachineLabelEmpty
    // Build remote_urls from state.repos via read_origin_url + canonicalise.
    // If remote_urls empty → return FetchResult::NoOverlapRepos
    // Construct FilterSpec, call bitable_search_records.
    // Map BitableRecord → TeamActivityRow.
    // Return FetchResult::Rows { rows }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FetchResult {
    Disabled,
    MachineLabelEmpty,
    NoOverlapRepos,
    Rows { rows: Vec<TeamActivityRow> },
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub struct TeamActivityRow {
    pub workspace_id: String,
    pub repo_remote_url: String,
    pub repo_display_name: String,
    pub task_title: String,
    pub assignee_machine: String,
    pub ansambel_status: String,
    pub last_activity_at: i64,          // epoch ms, 0 if missing
    pub last_message_preview: String,
    pub branch_name: String,
    pub diff_summary: String,
    pub pr_url: String,
    pub private: bool,
}

fn parse_record_to_row(record: BitableRecord) -> TeamActivityRow {
    // Extract fields by key, coerce types, default missing values to
    // empty string / 0 / false.
}
```

What's reused:

- `platform::lark_client::bitable_search_records` (3a-3.1)
- `platform::lark_client::LarkClient` construction (publisher loader path)
- `state::FilterSpec` (3a-3.1)
- `platform::repo_identity::{read_origin_url, canonicalise_remote_url}` (3a-3)

What's new:

- 1 IPC command + 1 parsing helper + 1 FetchResult enum. ~120 lines + tests.

**Canonical URL caching.** The publisher's enricher already caches per-repo
canonical URLs to avoid shelling out to `git remote get-url origin` on every
flush. The reader hits the same operation every 10s, so the same cache shape
applies. The reader maintains its OWN cache (separate from the publisher's
enricher) — this avoids entangling read and write paths. The implementation
keeps `Arc<Mutex<HashMap<RepoId, String>>>` lazy in the command module; an empty
cache on first call is fine (one shell-out per repo, then cached for the process
lifetime, refreshed if a repo's path changes).

### Frontend (Svelte 5)

```
src/lib/stores/team-activity.svelte.ts                 (NEW)
  class TeamActivityStore {
    rows: SvelteMap<string, TeamActivityRow>          // keyed by workspace_id
    status: 'idle' | 'loading' | 'error' | 'disabled'
                  | 'machine_label_empty' | 'no_overlap_repos'
    error: string | null
    selectedWorkspaceId: string | null

    start(): void               // mount: immediate fetch + setInterval + visibility listener
    stop(): void                // unmount: clearInterval + remove listener
    refresh(): Promise<void>    // manual refresh button
    select(workspaceId: string | null): void

    private tick(): Promise<void>            // visibility + fetch + reconcile
    private reconcile(rows: TeamActivityRow[]): void
  }
  export const teamActivity = new TeamActivityStore();

src/lib/ipc.ts                                          (extend)
  api.teamActivity.fetchRows(): Promise<FetchResult>

src/lib/components/sidebar/TeamActivityPanel.svelte    (NEW)
  - Collapsible section in Sidebar.svelte below the WORKSPACES section.
  - Header: section title + collapse caret + Refresh icon button.
  - Body when status === 'disabled': "Configure Team Activity in Settings →"
    link that opens SettingsDialog focused on the Team Activity panel.
  - Body when status === 'error': red banner + Retry button (calls refresh()).
  - Body when filter === null OR rows empty: "No team activity in your
    repos right now."
  - Body otherwise: rows grouped by repo_display_name (alphabetical), each
    row renders status dot (colour per ansambel_status), assignee_machine,
    truncated task_title, relative "5m ago" time. Click row →
    teamActivity.select(workspaceId).
  - Collapse state persisted via localStorage (same pattern as Sidebar width).

src/lib/components/team/TeamWorkspaceMirror.svelte    (NEW)
  - Reads `row = teamActivity.rows.get(teamActivity.selectedWorkspaceId)`
    reactively. Auto-close (back to prior mode) handled by store's reconcile.
  - Header: task_title (h1) + assignee_machine + status badge + relative
    last_activity_at + small "Last synced: 5s ago" + Refresh button.
  - "Code state" section:
      branch_name as badge.
      "Open branch on GitHub" link (constructed from repo_remote_url +
      branch_name — see GitHub URL construction below).
      diff_summary text when present; "Not yet published" placeholder
      otherwise (cite 3a-3 deferred status for honesty).
  - "Latest activity" section:
      last_message_preview rendered in <pre> for monospaced wrap (already
      sanitised server-side by publisher; treat as untrusted plain text).
      If ansambel_status === 'pr_ready' AND pr_url: prominent "Open PR"
      button (opens via tauri-plugin-opener).

src/lib/components/TitleBar.svelte                     (extend, ~20 lines)
  - When teamActivity.selectedWorkspaceId !== null:
      replace mode toggle with "Watching: {assignee} @ {short_title}"
      label + back arrow button (calls teamActivity.select(null)).
  - When null: normal Plan / Work mode toggle behaviour unchanged.

src/App.svelte                                          (extend, ~5 lines)
  - {#if teamActivity.selectedWorkspaceId}
      <TeamWorkspaceMirror />
    {:else if modeStore.mode === 'plan'}
      <KanbanBoard ... />
    {:else if selectedWorkspace}
      <WorkspaceView ... />
    {:else}
      "Select or create a workspace"
    {/if}
```

### GitHub URL construction

```typescript
function githubBranchUrl(remoteUrl: string, branch: string): string | null {
  if (!remoteUrl || !branch) return null;
  // Accept https:// and git@ ssh-style. Convert ssh to https for browser.
  // git@github.com:Foo/Bar  →  https://github.com/Foo/Bar
  let httpsBase: string;
  if (remoteUrl.startsWith('git@')) {
    const m = remoteUrl.match(/^git@([^:]+):(.+)$/);
    if (!m) return null;
    httpsBase = `https://${m[1]}/${m[2]}`;
  } else if (remoteUrl.startsWith('https://')) {
    httpsBase = remoteUrl;
  } else {
    return null; // unknown scheme; hide the link
  }
  return `${httpsBase}/tree/${encodeURIComponent(branch)}`;
}
```

This handles the two canonicalised shapes the publisher writes
(`https://github.com/x/y` or `git@github.com:x/y` — the publisher's
canonicaliser strips `.git`). For self-hosted git remotes that aren't GitHub,
the link still constructs but the `/tree/` path may 404; the link is
best-effort.

## Status colour palette

Mirror existing status colours from `WorkspaceView` for visual continuity:

| ansambel_status  | dot colour | reason                     |
| ---------------- | ---------- | -------------------------- |
| `running`        | green      | active turn                |
| `waiting`        | yellow     | agent idle, awaiting input |
| `blocked`        | red        | user attention needed      |
| `pr_ready`       | purple     | PR open, awaiting merge    |
| `done`           | grey       | completed                  |
| `idle` / unknown | grey-light | catch-all                  |

## Error handling matrix

| Scenario                                                | Source                                                         | Store behavior                                                  | UI                                                 |
| ------------------------------------------------------- | -------------------------------------------------------------- | --------------------------------------------------------------- | -------------------------------------------------- |
| `team_activity_config.json` absent or `app_token` empty | Rust returns `FetchResult::Disabled`                           | `status='disabled'`, polling continues (cheap no-op on backend) | Panel: "Configure Team Activity in Settings →"     |
| `machine_label` empty in config                         | Rust returns `FetchResult::MachineLabelEmpty`                  | `status='machine_label_empty'`                                  | Panel: "Set your machine label in Settings →"      |
| Zero local repos with origin remote                     | Rust returns `FetchResult::NoOverlapRepos`                     | `status='no_overlap_repos'`                                     | Panel: "Add a repo to see team activity."          |
| Lark returns 0 rows                                     | Rust returns `FetchResult::Rows { rows: [] }`                  | `status='idle'`, `rows` cleared via reconcile                   | Panel: "No team activity in your repos right now." |
| Lark 401/403                                            | Rust returns `Err(...)`                                        | `status='error'`, pause polling until manual refresh            | Panel: red banner + Retry button                   |
| Lark 429                                                | Rust retries once with `Retry-After`; second 429 returns `Err` | `status='error'`, toast 1× per minute "Throttled"               | Panel: previous data, subtle "Throttled" indicator |
| Network offline (IPC reject)                            | JS catch                                                       | Log warn, keep prior data, `status='error'`                     | Panel: "Offline — last synced X ago"               |
| Selected mirror row removed mid-view                    | reconcile detects                                              | `selectedWorkspaceId = null` + toast                            | Mirror closes, returns to prior mode               |
| Two ticks overlap                                       | JS `inflight` guard                                            | Skip new tick                                                   | (silent)                                           |
| Repos store changes mid-fetch                           | Backend builds fresh filter every call                         | Next tick reflects new repos                                    | Old result still applies — ~10s staleness          |

## Lifecycle hooks

```
App startup:
  Sidebar mounts → teamActivity.start()
    if config disabled → status='disabled', no interval
    else → first fetch + setInterval(10s)

Mode switch (Plan ↔ Work):
  Sidebar stays mounted; polling continues. Mirror view is independent
  of mode and triggered by sidebar click.

Mirror view enter:
  teamActivity.select(workspaceId) → selectedWorkspaceId set →
  App.svelte's {#if} routes to TeamWorkspaceMirror. TitleBar swaps in
  "Watching:" label.

Mirror view exit:
  Back button → teamActivity.select(null) → App.svelte falls back to
  prior mode. TitleBar restores normal label.

Config change at runtime (Settings → Save):
  No explicit wiring needed. Backend loads the config on every IPC call
  (cheap — JSON read + cache check). The next poll (≤10s later) picks up
  the new config automatically:
    - If app_token / table_id changed → new Lark target on next poll.
    - If machine_label changed → filter rebuild on next poll uses new label.
    - If config deleted → FetchResult::Disabled on next poll → store
      transitions to disabled state, panel updates.
  The publisher (write side) still needs an app restart per 3a-3's
  deferred followup; the read side trivially picks up changes because
  it has no long-lived state to swap.

App teardown:
  Sidebar unmounts → teamActivity.stop().
```

## Test plan

### Rust (`team_activity.rs`)

```
fetch_team_activity_rows_returns_disabled_when_config_absent
fetch_team_activity_rows_returns_disabled_when_app_token_empty
fetch_team_activity_rows_returns_machine_label_empty_when_label_blank
fetch_team_activity_rows_returns_no_overlap_when_no_repos_have_remote
fetch_team_activity_rows_builds_filter_from_appstate_repos
fetch_team_activity_rows_excludes_self_machine_in_filter
fetch_team_activity_rows_returns_parsed_rows_on_lark_response
fetch_team_activity_rows_propagates_lark_429_as_app_error
fetch_team_activity_rows_propagates_lark_auth_error
parse_record_to_row_handles_missing_optional_fields
parse_record_to_row_handles_private_true_value
parse_record_to_row_coerces_datetime_epoch_ms_to_i64
parse_record_to_row_handles_malformed_record_gracefully
parse_record_to_row_defaults_missing_strings_to_empty
canonical_remote_url_cached_after_first_call_per_repo
```

Pattern matches existing `bitable_search_records` tests (wiremock).

### Frontend (vitest)

`stores/team-activity.svelte.test.ts`:

```
TeamActivityStore_skips_tick_when_document_hidden
TeamActivityStore_fetches_immediately_on_visibilitychange_to_visible
TeamActivityStore_sets_status_disabled_on_FetchResult_disabled
TeamActivityStore_sets_status_machine_label_empty_on_FetchResult_machine_label_empty
TeamActivityStore_sets_status_no_overlap_repos_on_FetchResult_no_overlap_repos
TeamActivityStore_reconciles_new_rows_into_map
TeamActivityStore_removes_rows_dropped_from_server_response
TeamActivityStore_updates_existing_rows_in_place
TeamActivityStore_skips_overlapping_ticks_when_request_inflight
TeamActivityStore_retries_once_on_429_with_retry_after_delay
TeamActivityStore_keeps_prior_data_on_network_error
TeamActivityStore_clears_rows_when_disabled_received_after_rows
TeamActivityStore_select_sets_selectedWorkspaceId
TeamActivityStore_auto_clears_selection_when_selected_row_removed
TeamActivityStore_start_idempotent_when_called_twice
TeamActivityStore_stop_clears_interval_and_listener
TeamActivityStore_refresh_triggers_immediate_fetch
```

`components/sidebar/TeamActivityPanel.svelte.test.ts`:

```
TeamActivityPanel_renders_disabled_state_when_status_disabled
TeamActivityPanel_renders_machine_label_hint_when_status_machine_label_empty
TeamActivityPanel_renders_add_repo_hint_when_status_no_overlap_repos
TeamActivityPanel_renders_no_activity_when_idle_with_zero_rows
TeamActivityPanel_groups_rows_by_repo_display_name_alphabetical
TeamActivityPanel_renders_status_dot_with_color_per_status
TeamActivityPanel_renders_relative_last_activity_time
TeamActivityPanel_click_row_calls_teamActivity_select
TeamActivityPanel_refresh_button_calls_teamActivity_refresh
TeamActivityPanel_renders_error_banner_with_retry_button
TeamActivityPanel_collapse_state_persists_via_localStorage
```

`components/team/TeamWorkspaceMirror.svelte.test.ts`:

```
TeamWorkspaceMirror_renders_task_title_assignee_status_in_header
TeamWorkspaceMirror_constructs_github_branch_url_from_https_remote
TeamWorkspaceMirror_constructs_github_branch_url_from_ssh_remote
TeamWorkspaceMirror_hides_branch_link_when_remote_url_unknown_scheme
TeamWorkspaceMirror_renders_not_yet_published_placeholder_when_diff_empty
TeamWorkspaceMirror_renders_open_pr_button_only_when_pr_ready_and_pr_url_set
TeamWorkspaceMirror_back_button_clears_selected_workspace
TeamWorkspaceMirror_auto_closes_when_underlying_row_removed
TeamWorkspaceMirror_renders_last_synced_relative_time
TeamWorkspaceMirror_renders_sanitized_message_preview_verbatim
TeamWorkspaceMirror_handles_no_branch_name_gracefully
```

`ipc.test.ts` (extend):

```
api_teamActivity_fetchRows_invokes_fetch_team_activity_rows
api_teamActivity_fetchRows_passes_filter_payload_unchanged
```

### E2E (`tests/e2e/phase-3a-4/`)

Env-gated `ANSAMBEL_LARK_FIXTURE=1` (same pattern as 3a-3 publisher E2E):

```
sidebar_shows_team_rows_for_overlapping_repos
clicking_row_opens_mirror_view_with_github_branch_link
back_button_returns_to_prior_mode
mirror_view_auto_closes_when_row_disappears_from_polled_response
panel_renders_disabled_state_when_config_absent
```

### Coverage target

Per CLAUDE.md hard rule: **95% lines/statements/functions, 93% branches** on
changed files (the branches threshold already accommodates Svelte template
branch artefacts per the FilterBar precedent).

Estimated total: ~10 Rust + ~37 vitest + ~5 E2E = ~52 new test cases.

## Risks & open questions

- **Polling churn on team with many engineers**: 10 engineers × 6 req/min = 60
  req/min read traffic toward Lark. Publisher peak adds up to 200 req/min write.
  Combined headroom is tight. Mitigation: visibility-aware pause already cuts
  inactive engineers to 0 req/min. If we hit 429s in practice, raise interval to
  15s.
- **Sticky stale data after network failure**: If Lark is unreachable for hours,
  the sidebar shows an "offline" subtitle but the rows themselves are frozen
  from the last successful poll — which might display teammates as "running"
  long after they finished. Acceptable for the awareness use case; the staleness
  indicator covers user expectations.
- **GitHub URL heuristic**: only handles GitHub-shaped remotes (https or
  git@github.com:). Self-hosted GitLab / Bitbucket / Forgejo will get a link
  that 404s. Defer to follow-up Phase that adds provider abstraction. In the
  meantime, hide the link when the heuristic can't match.
- **Mirror view scope creep**: easy temptation to grow this surface into full
  chat history / live updates / commenting. Resist — anything beyond read-only
  awareness belongs in Phase 3a-6 (Lark IM) or 3a-8 (handoff bundles). The
  mirror view is intentionally minimal.

## Out-of-scope followups

- Phase 3a-5/6: notifications + diff_summary + pr_url emission (closes the "Not
  yet published" placeholders in the mirror view automatically).
- Phase 3a-6: Lark IM ping when teammate status flips to `blocked` or
  `pr_ready`.
- Phase 3a-7: per-table membership UI for engineers who belong to multiple
  team-activity tables.
- Phase 3a-8: handoff bundles (conversation + uncommitted changes archive). The
  mirror view's "Latest activity" section is the natural jumping-off point for
  the "Accept handoff" CTA in that phase.
