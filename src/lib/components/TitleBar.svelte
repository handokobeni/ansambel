<!-- src/lib/components/TitleBar.svelte -->
<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import type { Mode } from '$lib/types';

  const {
    mode = undefined,
    onModeChange = undefined,
  }: {
    mode?: Mode;
    onModeChange?: (next: Mode) => void;
  } = $props();

  let adding = $state(false);

  const selectedRepo = $derived(repos.getSelected());

  async function handleAddRepo() {
    if (adding) return;
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== 'string' || !selected) return;
    adding = true;
    try {
      const repo = await repos.add(selected);
      repos.select(repo.id);
      // Backend `add_repo` is idempotent on the canonical path — re-Add of
      // an existing folder returns the same RepoInfo. Hydrate both stores
      // so the kanban populates without waiting for a restart.
      await Promise.all([workspaces.loadForRepo(repo.id), tasks.loadForRepo(repo.id)]);
    } catch (err) {
      console.error('Failed to add repo:', err);
      alert(`Failed to add repo: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      adding = false;
    }
  }
</script>

<header
  class="titlebar flex items-center justify-between h-10 px-3 bg-[var(--bg-titlebar)] border-b border-[var(--border)] flex-shrink-0 select-none"
>
  <div class="flex items-center gap-2">
    <span
      class="text-sm font-semibold text-[var(--text-primary)] max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap"
    >
      {#if selectedRepo}
        {selectedRepo.name}
      {:else}
        <span class="text-[var(--text-muted)]">No repo selected</span>
      {/if}
    </span>
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
      class="flex items-center gap-1 px-2 py-1 text-xs font-semibold rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
      onclick={handleAddRepo}
      disabled={adding}
      aria-label="Add Repo"
    >
      {adding ? 'Adding…' : 'Add Repo'}
    </button>
  </div>
</header>
