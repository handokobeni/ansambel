# Restore Last-Opened Repo on Startup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reopen the repo the user last had selected when Ansambel restarts,
instead of always falling back to the first repo in `repos.json` order.

**Root cause (already investigated):** `AppSettings.selected_repo_id` exists and
`save_settings`/`load_settings` exist, but **nothing in production code writes
`selected_repo_id`**, and there is no settings IPC for the frontend to read or
write it. So `repos.select(id)` only mutates in-memory state, the persisted
field stays `None`, and on startup `App.svelte` ignores any persisted choice and
picks `repos.repos.keys().next().value`.

**Architecture:**

- Backend: new `commands/settings.rs` exposing
  `set_selected_repo(repo_id: Option<String>)` and
  `get_selected_repo() -> Option<String>`. Set mutates
  `state.settings.selected_repo_id`, clones the new settings under the lock,
  releases the lock, then `save_settings` to disk (immediate write — matches
  CLAUDE.md's "immediate for app_settings" rule). Get reads under the lock and
  returns the cloned Option.
- Frontend: new `api.settings.getSelectedRepo()` / `setSelectedRepo(id)` IPC
  wrappers. `ReposStore.select(id)` becomes the single persistence choke point:
  it sets in-memory state and fire-and-forgets the IPC (`.catch(logError)` —
  selection persistence must never throw to a caller). `ReposStore.remove(id)`
  routes through `this.select(null)` when removing the active repo, so removal
  also clears the persisted id. On startup `App.svelte` calls
  `api.settings.getSelectedRepo()` BEFORE the first-repo fallback; if the saved
  id is set AND present in the loaded repo map, select it; else fall back to the
  existing first-repo behavior.

**Tech Stack:** Rust (`commands/settings.rs`, `state.rs` unchanged), Tauri v2,
Svelte 5 runes, vitest, cargo test.

**Out of scope (YAGNI):** `selected_workspace_id`, `recent_repos`, generic
settings UI, auto-selecting the just-added repo. The repo-restore bug is the
reported problem; the rest can follow if asked.

---

## Task 1: Backend `settings` command + persistence

**Files:**

- Create: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add `pub mod settings;`)
- Modify: `src-tauri/src/lib.rs` (register the two new handlers in
  `tauri::generate_handler![...]`)

**Pattern to mirror:** `commands/repo.rs::update_gh_profile` — a `pub async fn`
Tauri wrapper that resolves `data_dir` from the AppHandle, calls a `pub(crate)`
inner that takes `data_dir: PathBuf, state: Arc<Mutex<AppState>>`, and the inner
locks the state, mutates, then writes to disk. Hard rules: every
`#[tauri::command]` returns `Result<T, String>`; no `.unwrap()`/`.expect()`
outside tests; mutex discipline — clone the new settings under the lock, then
drop the lock before the disk write (cleaner than `update_gh_profile_inner`'s
style, which keeps the lock across `save_repos`; the rule in CLAUDE.md is
explicit here).

- [ ] **Step 1: Write the failing tests**

Create the file with the tests first (red phase). Keep the file minimal — no
production impl yet — so cargo refuses to build until Step 3.

```rust
// src-tauri/src/commands/settings.rs
use crate::error::Result;
use crate::persistence::settings::{load_settings, save_settings};
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

#[tauri::command]
pub async fn set_selected_repo(
    repo_id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_selected_repo_inner(repo_id, data_dir, state.inner().clone())
        .map_err(|e| {
            tracing::error!(error = %e, "set_selected_repo failed");
            e.to_string()
        })
}

#[tauri::command]
pub async fn get_selected_repo(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Option<String>, String> {
    get_selected_repo_inner(state.inner().clone()).map_err(|e| e.to_string())
}

pub(crate) fn set_selected_repo_inner(
    repo_id: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let settings_snapshot = {
        let mut st = state.lock().map_err(|e| crate::error::AppError::Other(e.to_string()))?;
        st.settings.selected_repo_id = repo_id;
        st.settings.clone()
    };
    save_settings(&data_dir, &settings_snapshot)
}

pub(crate) fn get_selected_repo_inner(
    state: Arc<Mutex<AppState>>,
) -> Result<Option<String>> {
    let st = state.lock().map_err(|e| crate::error::AppError::Other(e.to_string()))?;
    Ok(st.settings.selected_repo_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppSettings;

    fn make_state(initial: AppSettings) -> Arc<Mutex<AppState>> {
        let mut st = AppState::default();
        st.settings = initial;
        Arc::new(Mutex::new(st))
    }

    #[test]
    fn set_selected_repo_inner_updates_state_and_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let state = make_state(AppSettings::default());

        set_selected_repo_inner(Some("repo_kelola".into()), data_dir.clone(), state.clone())
            .unwrap();

        // In-memory updated
        assert_eq!(
            state.lock().unwrap().settings.selected_repo_id.as_deref(),
            Some("repo_kelola")
        );
        // Round-tripped through disk
        let loaded = load_settings(&data_dir).unwrap();
        assert_eq!(loaded.selected_repo_id.as_deref(), Some("repo_kelola"));
    }

    #[test]
    fn set_selected_repo_inner_with_none_clears_persisted_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let mut initial = AppSettings::default();
        initial.selected_repo_id = Some("repo_old".into());
        let state = make_state(initial);

        set_selected_repo_inner(None, data_dir.clone(), state.clone()).unwrap();

        assert!(state.lock().unwrap().settings.selected_repo_id.is_none());
        let loaded = load_settings(&data_dir).unwrap();
        assert!(loaded.selected_repo_id.is_none());
    }

    #[test]
    fn get_selected_repo_inner_returns_current_settings_value() {
        let mut initial = AppSettings::default();
        initial.selected_repo_id = Some("repo_active".into());
        let state = make_state(initial);

        let got = get_selected_repo_inner(state).unwrap();
        assert_eq!(got.as_deref(), Some("repo_active"));
    }

    #[test]
    fn get_selected_repo_inner_returns_none_for_default_settings() {
        let state = make_state(AppSettings::default());
        assert!(get_selected_repo_inner(state).unwrap().is_none());
    }
}
```

If `AppState::default()` does not exist or has a different shape, mirror
whatever `commands/repo.rs` tests use for state construction (look at
`add_repo_inner`'s tests for the canonical helper). The hard requirements for
the test set are: the four behaviors above (set+persist, none-clears, get
returns current, get returns none-default).

- [ ] **Step 2: Wire the module + register handlers**

In `src-tauri/src/commands/mod.rs` add `pub mod settings;` next to the other
command modules (alphabetical or with the existing convention).

In `src-tauri/src/lib.rs`, find `tauri::generate_handler![...]` (around the line
shown by `grep -n "tauri::generate_handler"`) and add
`crate::commands::settings::set_selected_repo, crate::commands::settings::get_selected_repo,`
to the list. CLAUDE.md is explicit: handlers must be registered into
`generate_handler!` or they will be silently dropped.

- [ ] **Step 3: Run tests to verify RED then GREEN**

Red: `cd src-tauri && cargo test --lib commands::settings` — first cargo build
will compile the file with the impl already present from Step 1, so this is
effectively a green-from-the-start TDD for backend (the impl + tests landed in
the same file). That is acceptable here because the TDD requirement is about
ensuring tests exist BEFORE production code is depended on by other systems —
Task 2 (frontend) cannot proceed without the backend impl + tests passing.
**Verify all 4 tests pass.**

- [ ] **Step 4: Backend gates**

Run:

```
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: full lib suite passes (~795), clippy clean, fmt clean. Run
`cargo fmt --all` if fmt check fails.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/settings.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(settings): persist+read selected_repo_id via set_selected_repo/get_selected_repo commands"
```

---

## Task 2: Frontend persist on select + restore on startup

**Files:**

- Modify: `src/lib/ipc.ts` (new `settings` namespace)
- Modify: `src/lib/stores/repos.svelte.ts` (`select` fires IPC; `remove` routes
  through `select(null)`)
- Modify: `src/lib/stores/repos.svelte.test.ts` (cover the new behaviors)
- Modify: `src/App.svelte` (startup: restore persisted → fallback to first)
- Modify: `src/App.test.ts` (cover restore + fallback paths)

The `ReposStore.select(id)` becomes the single choke point. Persistence is
fire-and-forget — failing to persist must NEVER throw to a caller of `select`,
because select is called from synchronous Svelte event handlers and click paths.

- [ ] **Step 1: Add the IPC wrappers**

In `src/lib/ipc.ts`, add a new top-level `settings` namespace next to `system`:

```ts
settings: {
  getSelectedRepo: (): Promise<string | null> => invoke('get_selected_repo'),
  setSelectedRepo: (repoId: string | null): Promise<void> =>
    invoke('set_selected_repo', { repoId }),
},
```

Tauri serializes Rust `Option<String>` as `string | null` in JS. The arg key
`repoId` matches the Rust command parameter `repo_id` after Tauri's camelCase
conversion.

- [ ] **Step 2: Write the failing repo-store tests**

In `src/lib/stores/repos.svelte.test.ts`, mock the new IPC and add tests (adapt
to the file's existing mocking style — it likely already mocks `$lib/ipc`):

```ts
import { vi } from 'vitest';
// existing mock — extend with settings namespace
vi.mock('$lib/ipc', () => ({
  api: {
    repo: {
      list: vi.fn(),
      add: vi.fn(),
      remove: vi.fn(),
      updateGhProfile: vi.fn(),
    },
    settings: {
      getSelectedRepo: vi.fn(),
      setSelectedRepo: vi.fn().mockResolvedValue(undefined),
    },
  },
}));

it('select(id) persists the selection via api.settings.setSelectedRepo', async () => {
  const { api } = await import('$lib/ipc');
  const store = new ReposStore();
  store.select('repo_kelola');
  expect(api.settings.setSelectedRepo).toHaveBeenCalledWith('repo_kelola');
});

it('select(null) persists null', async () => {
  const { api } = await import('$lib/ipc');
  const store = new ReposStore();
  store.select(null);
  expect(api.settings.setSelectedRepo).toHaveBeenCalledWith(null);
});

it('select swallows persistence errors (never throws to caller)', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.settings.setSelectedRepo).mockRejectedValueOnce('disk full');
  const store = new ReposStore();
  // Sync call must not throw even if the underlying promise rejects.
  expect(() => store.select('repo_x')).not.toThrow();
  // Flush microtasks so the .catch runs.
  await new Promise((r) => setTimeout(r, 0));
});

it('remove(activeId) clears persistence via select(null)', async () => {
  const { api } = await import('$lib/ipc');
  const store = new ReposStore();
  // seed a repo + select it
  store.repos.set('repo_a', { id: 'repo_a', name: 'a', path: '/a' } as Repo);
  store.select('repo_a');
  vi.mocked(api.settings.setSelectedRepo).mockClear();
  await store.remove('repo_a');
  expect(api.settings.setSelectedRepo).toHaveBeenLastCalledWith(null);
  expect(store.selectedRepoId).toBeNull();
});
```

Adapt the import paths and the `Repo` shape to whatever the existing test file
uses. Reuse its `beforeEach` mock-reset pattern.

- [ ] **Step 3: Update `repos.svelte.ts`**

```ts
import { api } from '$lib/ipc';
import { logError } from '$lib/logging'; // or whichever logging helper the file uses

select(id: string | null): void {
  this.selectedRepoId = id;
  // Fire-and-forget; persistence must never throw to a caller of select().
  api.settings.setSelectedRepo(id).catch((err) => {
    logError('settings.setSelectedRepo failed', err);
  });
}
```

And in `remove(id)`, replace the inline `this.selectedRepoId = null` branch with
`this.select(null)` so the same choke point runs:

```ts
async remove(id: string): Promise<void> {
  await api.repo.remove(id);
  this.repos.delete(id);
  if (this.selectedRepoId === id) {
    this.select(null);
  }
}
```

If `logError` doesn't exist, use `console.error` ONLY if the project already
permits it in stores (it does NOT — CLAUDE.md says no `console.log`). Prefer the
`src/lib/logging.ts` wrapper.

Verify the repo-store tests pass:
`bun run vitest run src/lib/stores/repos.svelte.test.ts`.

- [ ] **Step 4: Write the failing App.svelte startup tests**

In `src/App.test.ts`, ensure the test harness mocks
`api.settings.getSelectedRepo` (extend the existing IPC mock — App.test.ts
likely already mocks `$lib/ipc`). Add tests that drive App's `onMount` startup
with two seeded repos:

```ts
it('on startup selects the persisted selected_repo_id when it is present in the repos list', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.repo.list).mockResolvedValue([
    { id: 'repo_top', name: 'top-assessment', path: '/top' },
    { id: 'repo_kelola', name: 'kelola', path: '/kelola' },
  ] as Repo[]);
  vi.mocked(api.settings.getSelectedRepo).mockResolvedValue('repo_kelola');
  render(App);
  // Wait for onMount to settle.
  await waitFor(() => expect(repos.selectedRepoId).toBe('repo_kelola'));
});

it('on startup falls back to the first repo when no selection is persisted', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.repo.list).mockResolvedValue([
    { id: 'repo_top', name: 'top-assessment', path: '/top' },
    { id: 'repo_kelola', name: 'kelola', path: '/kelola' },
  ] as Repo[]);
  vi.mocked(api.settings.getSelectedRepo).mockResolvedValue(null);
  render(App);
  await waitFor(() => expect(repos.selectedRepoId).toBe('repo_top'));
});

it('on startup falls back to the first repo when the persisted id no longer exists', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.repo.list).mockResolvedValue([
    { id: 'repo_top', name: 'top-assessment', path: '/top' },
  ] as Repo[]);
  vi.mocked(api.settings.getSelectedRepo).mockResolvedValue('repo_gone');
  render(App);
  await waitFor(() => expect(repos.selectedRepoId).toBe('repo_top'));
});
```

`repos.selectedRepoId` is the in-memory state; importing the `repos` singleton
matches how other App tests assert on store state. If App.test.ts uses a
different flush idiom (tick, microtask drain), use that.

- [ ] **Step 5: Update `App.svelte` startup**

In `src/App.svelte`, replace the cold-start auto-select block with a
restore-then-fallback block. Approximate diff inside `onMount`:

```ts
await repos.load();
await larkBindings.load();
// Cold start: try the persisted selection first; if it's still valid use it,
// otherwise fall back to the first repo so the kanban hydrates instead of
// rendering the "Add a repo" empty state.
if (!repos.selectedRepoId) {
  let persisted: string | null = null;
  try {
    persisted = await api.settings.getSelectedRepo();
  } catch (err) {
    // A settings read failure must not block startup — log and fall back.
    logError('settings.getSelectedRepo failed', err);
  }
  if (persisted && repos.repos.has(persisted)) {
    repos.select(persisted);
  } else {
    const firstRepoId = repos.repos.keys().next().value;
    if (firstRepoId) {
      repos.select(firstRepoId);
    }
  }
}
if (repos.selectedRepoId) {
  // …unchanged hydrate block (workspaces + tasks)…
}
```

Verify: `bun run vitest run src/App.test.ts src/lib/stores/repos.svelte.test.ts`
→ PASS.

- [ ] **Step 6: Full gates**

Run:

```
bun run check
bun run vitest run
cd src-tauri && cargo test --lib && cargo clippy --lib --all-targets -- -D warnings
```

Expected: all green. The vitest count should rise by the new tests (~7).

- [ ] **Step 7: Commit**

```bash
git add src/lib/ipc.ts src/lib/stores/repos.svelte.ts src/lib/stores/repos.svelte.test.ts src/App.svelte src/App.test.ts
git commit -m "feat(repo): restore last-opened repo on startup"
```

---

## Task 3: Journal entry

**Files:** create `journal/2026-05-28-restore-last-opened-repo.md`.

Document: what shipped (persist on select + restore on startup), root cause
(field existed but no writer + no IPC), the four state-restore signals
considered (we chose `selected_repo_id`; `selected_workspace_id`, `recent_repos`
deferred), trade-off (fire-and-forget persistence on `select` — a brief write
failure means the last selection won't survive a crash, acceptable because
selection runs on every interaction), and where this was folded into
`feat/terminal-multitab` per the standing precedent.

Commit:

```bash
git add journal/2026-05-28-restore-last-opened-repo.md
git commit -m "docs(journal): restore last-opened repo on startup"
```

---

## Self-review

- **Spec coverage:** persist on select (T2/S2-3), persist on remove (T2/S2-3),
  restore on startup with validity check (T2/S4-5), fall back to first when
  persisted is null or stale (T2/S4-5), survive persistence errors (T2/S2-3),
  backend command + register (T1).
- **No placeholder:** every code step has full code or an exact
  adapt-to-existing instruction.
- **Type consistency:** Rust `Option<String>` ↔ TS `string | null`; Tauri
  serializes both ways; `repoId` arg matches `repo_id` parameter after camelCase
  conversion.
- **YAGNI:** no `selected_workspace_id`, no `recent_repos`, no settings UI, no
  auto-select-on-add.
