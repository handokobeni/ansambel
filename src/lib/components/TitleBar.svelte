<!-- src/lib/components/TitleBar.svelte -->
<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { listen } from '@tauri-apps/api/event';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import { theme } from '$lib/stores/theme.svelte';
  import { viewMissing } from '$lib/stores/view-missing.svelte';
  import { tooltip } from '$lib/actions';
  import SettingsDialog from './SettingsDialog.svelte';
  import RepoSettingsDialog from '$lib/components/repo/RepoSettingsDialog.svelte';
  import type { Mode } from '$lib/types';

  // Reading `theme.colorMode` (a `$state` field) inside this $derived
  // makes the toggle icon reactive to mode changes anywhere in the app.
  const isDark = $derived(theme.colorMode !== 'light' && theme.isDark());

  const {
    mode = undefined,
    onModeChange = undefined,
  }: {
    mode?: Mode;
    onModeChange?: (next: Mode) => void;
  } = $props();

  let adding = $state(false);
  let settingsOpen = $state(false);
  let repoSettingsOpen = $state(false);

  const selectedRepo = $derived(repos.getSelected());

  // Subscribe to the backend `lark-view-missing` event so the store reflects
  // every fallback the provider emits. The $effect cleanup tears down the
  // listener on unmount; we capture the unlisten fn from the awaited promise
  // because `listen()` resolves AFTER mount.
  let unlistenViewMissing: (() => void) | null = null;
  $effect(() => {
    listen<{ repo_id: string; view_id: string }>('lark-view-missing', (event) =>
      viewMissing.report(event.payload.repo_id, event.payload.view_id)
    ).then((un) => (unlistenViewMissing = un));
    return () => {
      unlistenViewMissing?.();
      unlistenViewMissing = null;
    };
  });

  // Per-repo lookup so the banner only shows for the currently selected repo —
  // other repos may have a stale missing-view entry but should stay quiet
  // until the user navigates to them.
  const missingViewForSelected = $derived(
    selectedRepo ? (viewMissing.entries.get(selectedRepo.id) ?? null) : null
  );

  async function handleAddRepo() {
    if (adding) return;
    // Latch the in-flight flag BEFORE awaiting the dialog so a fast
    // double-click can't open two pickers / fire two backend calls. The
    // finally below clears it after the whole flow (cancel, success, or
    // error) settles.
    adding = true;
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== 'string' || !selected) return;
      const repo = await repos.add(selected);
      repos.select(repo.id);
      // Backend `add_repo` is idempotent on the canonical path — re-Add of
      // an existing folder returns the same RepoInfo. Hydrate both stores
      // so the kanban populates without waiting for a restart.
      await Promise.all([workspaces.loadForRepo(repo.id), tasks.loadForRepo(repo.id)]);
    } catch (err) {
      addToast(`Failed to add repo: ${err instanceof Error ? err.message : String(err)}`, 'error');
    } finally {
      adding = false;
    }
  }
</script>

{#if missingViewForSelected}
  <div
    class="flex items-center gap-2 px-3 py-1.5 bg-[var(--bg-warning,#3b3000)] border-b border-[var(--border-warning,#a87900)] text-[11px] text-[var(--text-warning,#fcd34d)]"
    data-testid="view-missing-banner"
  >
    <span>
      The Lark view bound to {selectedRepo?.name ?? 'this repo'} no longer exists (id:
      {missingViewForSelected}). Showing all records.
    </span>
    <button
      type="button"
      onclick={() => (repoSettingsOpen = true)}
      class="px-1.5 py-0.5 rounded border border-[var(--border-warning,#a87900)]"
      data-testid="view-missing-reconfigure"
    >
      Reconfigure
    </button>
    <button
      type="button"
      onclick={() => selectedRepo && viewMissing.dismiss(selectedRepo.id)}
      class="px-1.5 py-0.5 rounded border border-[var(--border-warning,#a87900)]"
      data-testid="view-missing-dismiss"
    >
      Dismiss
    </button>
  </div>
{/if}

<header
  class="titlebar flex items-center justify-between h-10 px-3 bg-[var(--bg-titlebar)] border-b border-[var(--border)] flex-shrink-0 select-none"
>
  <div class="flex items-center gap-1.5">
    {#if selectedRepo}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span
        class="text-sm font-semibold text-[var(--text-primary)] max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap"
        data-testid={`repo-row-${selectedRepo.id}`}
        oncontextmenu={(e) => {
          e.preventDefault();
          repoSettingsOpen = true;
        }}
      >
        {selectedRepo.name}
      </span>
      <button
        type="button"
        class="flex items-center justify-center w-6 h-6 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
        onclick={() => (repoSettingsOpen = true)}
        aria-label="Repo settings"
        data-testid="open-repo-settings"
        use:tooltip={{ text: 'Repo settings (Lark sync, …)' }}
      >
        <svg
          viewBox="0 0 24 24"
          width="13"
          height="13"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="3" />
          <path
            d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
          />
        </svg>
      </button>
    {:else}
      <span
        class="text-sm font-semibold text-[var(--text-muted)] max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap"
      >
        No repo selected
      </span>
    {/if}
  </div>

  {#if mode !== undefined && onModeChange !== undefined}
    <div
      class="flex overflow-hidden rounded-md border border-[var(--border)] bg-[var(--bg-card)]"
      role="group"
      aria-label="View mode"
    >
      <button
        class="mode-btn px-3.5 py-1 text-[0.8125rem] font-medium cursor-pointer border-none transition-colors"
        class:active={mode === 'plan'}
        style={mode === 'plan'
          ? 'background: var(--accent, #6366f1); color: #fff;'
          : 'background: none; color: var(--text-muted);'}
        onclick={() => onModeChange('plan')}
        aria-pressed={mode === 'plan'}
      >
        Plan
      </button>
      <button
        class="mode-btn px-3.5 py-1 text-[0.8125rem] font-medium cursor-pointer border-none transition-colors"
        class:active={mode === 'work'}
        style={mode === 'work'
          ? 'background: var(--accent, #6366f1); color: #fff;'
          : 'background: none; color: var(--text-muted);'}
        onclick={() => onModeChange('work')}
        aria-pressed={mode === 'work'}
      >
        Work
      </button>
    </div>
  {/if}

  <div class="flex items-center gap-2">
    <button
      type="button"
      class="flex items-center justify-center w-7 h-7 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
      onclick={() => theme.toggleColorMode()}
      aria-label={isDark ? 'Switch to light theme' : 'Switch to dark theme'}
      data-theme-toggle
      data-mode={isDark ? 'dark' : 'light'}
      use:tooltip={{
        text: isDark ? 'Switch to light theme' : 'Switch to dark theme',
        shortcut: '⌃⇧L',
      }}
    >
      {#if isDark}
        <!-- Sun icon: shown in dark mode (clicking flips to light) -->
        <svg
          viewBox="0 0 24 24"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="4" />
          <line x1="12" y1="2" x2="12" y2="4" />
          <line x1="12" y1="20" x2="12" y2="22" />
          <line x1="4.93" y1="4.93" x2="6.34" y2="6.34" />
          <line x1="17.66" y1="17.66" x2="19.07" y2="19.07" />
          <line x1="2" y1="12" x2="4" y2="12" />
          <line x1="20" y1="12" x2="22" y2="12" />
          <line x1="4.93" y1="19.07" x2="6.34" y2="17.66" />
          <line x1="17.66" y1="6.34" x2="19.07" y2="4.93" />
        </svg>
      {:else}
        <!-- Moon icon: shown in light mode (clicking flips to dark) -->
        <svg
          viewBox="0 0 24 24"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      {/if}
    </button>
    <button
      type="button"
      class="flex items-center justify-center w-7 h-7 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
      onclick={() => (settingsOpen = true)}
      aria-label="Open settings"
      data-testid="open-settings"
      use:tooltip={{ text: 'Settings' }}
    >
      <svg
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="3" />
        <path
          d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
        />
      </svg>
    </button>
    <button
      class="flex items-center gap-1 px-2 py-1 text-xs font-semibold rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
      onclick={handleAddRepo}
      disabled={adding}
      aria-label="Add Repo"
    >
      {adding ? 'Adding…' : 'Add Repo'}
    </button>
  </div>
</header>

<SettingsDialog open={settingsOpen} onClose={() => (settingsOpen = false)} />

{#if selectedRepo && repoSettingsOpen}
  <RepoSettingsDialog
    repoId={selectedRepo.id}
    repoName={selectedRepo.name}
    open={true}
    onClose={() => (repoSettingsOpen = false)}
  />
{/if}
