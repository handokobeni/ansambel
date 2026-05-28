# Journal — 2026-05-28 — Restore last-opened repo

## What shipped

On restart, Ansambel now reopens the repo the user last had selected — instead
of always picking the first repo in `repos.json` order.

## Root cause (was a never-wired feature, not a regression)

`AppSettings.selected_repo_id` was defined and `save_settings`/`load_settings`
were wired, but **nothing in production code ever wrote `selected_repo_id`** and
there was no settings IPC for the frontend. So the field stayed `None` forever,
and `App.svelte` cold-start did `repos.repos.keys().next().value` (= first repo
in `repos.json` insertion order). When the user opened "kelola" after
"top-assessment", the next restart still landed on "top-assessment".

## Backend

- New `commands/settings.rs`:
  - `set_selected_repo(repo_id: Option<String>)` — locks `AppState`, sets
    `st.settings.selected_repo_id`, **clones the new settings under the lock,
    drops the lock**, then `save_settings(&data_dir, &snapshot)` (immediate
    write per CLAUDE.md's "immediate for app_settings" rule; mutex discipline
    keeps the lock off the disk write).
  - `get_selected_repo() -> Option<String>` — locks, clones, returns.
- Registered both handlers in `tauri::generate_handler!` (silent-drop trap).

## Frontend

- `src/lib/ipc.ts`: new `settings` namespace with `getSelectedRepo` /
  `setSelectedRepo`.
- `src/lib/stores/repos.svelte.ts`:
  - `select(id)` is the single persistence choke point. Sets in-memory state,
    then fire-and-forgets the IPC with `.catch(console.error)`. Selection runs
    on every interaction; it MUST never throw to a synchronous caller, hence
    fire-and-forget.
  - `remove(id)` routes through `this.select(null)` when the removed repo was
    active, so deletion also clears the persisted id via the same choke point
    instead of bypassing it.
- `src/App.svelte` cold-start:
  `try { await api.settings.getSelectedRepo() } catch { … }` → if the saved id
  is non-null AND still in `repos.repos`, `repos.select(persisted)`; otherwise
  fall back to the existing first-repo pick. A settings read failure logs and
  falls through (never blocks startup).

## Decisions

- **Persist on `select`, not on `unload`.** `beforeunload` in a Tauri webview is
  flaky and we'd lose the value if the app crashed. Persisting on every
  selection is cheap (one atomic settings write) and means the persisted state
  always reflects the last user intent.
- **Fire-and-forget IPC inside `select`.** Keeps `select` synchronous (zero
  call-site churn) and makes a transient disk hiccup invisible to the user.
- **Validate on restore.** If the persisted id no longer exists in
  `repos.repos`, fall back to the first repo instead of trying to select a
  ghost. Repos can be removed between sessions.
- **Out of scope (YAGNI):** `selected_workspace_id`, `recent_repos` MRU list,
  generic settings UI, auto-selecting the just-added repo. `AppSettings` already
  has those fields too, but the bug report was specifically about the
  last-opened repo; the rest can follow if asked.

## Tests

- Rust (`commands/settings.rs`, 4 new): set+persist round-trip via
  `load_settings`; setting `None` clears the persisted id; get returns the
  current value; get on default returns `None`. **795 total cargo lib tests
  pass.**
- Frontend store (`repos.svelte.test.ts`, +4 → 15): `select(id)` calls
  `setSelectedRepo(id)`; `select(null)` calls `setSelectedRepo(null)`; `select`
  swallows an IPC rejection without throwing (spy on `console.error`); removing
  the active repo last calls `setSelectedRepo(null)`.
- Frontend startup (`App.test.ts`, +3 → 21): persisted id present AND in repos →
  restored; persisted is null → first-repo fallback; persisted is set but stale
  → first-repo fallback.
- **958 → 965 vitest pass total.** `bun run check`, lint, clippy, fmt all clean.

## Branch placement

Folded into `feat/terminal-multitab` per the standing precedent — when the
detect-default-branch offline failure surfaced during manual testing of
terminal-multitab, the user said "perbaiki bugfix detect-branch dulu gabungin
saja di branch terminal multi tab ini". This bug surfaced the same way (during
manual test of the same dev build), so it ships with the same branch + PR.
