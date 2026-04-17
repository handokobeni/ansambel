<!-- src/lib/components/TitleBar.svelte -->
<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { repos } from '$lib/stores/repos.svelte';

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
    } catch (err) {
      console.error('Failed to add repo:', err);
    } finally {
      adding = false;
    }
  }
</script>

<header
  class="flex items-center justify-between h-10 px-3 bg-[var(--bg-titlebar)] border-b border-[var(--border)] flex-shrink-0 select-none"
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
