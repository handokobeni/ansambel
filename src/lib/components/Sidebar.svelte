<!-- src/lib/components/Sidebar.svelte -->
<script lang="ts">
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import type { WorkspaceStatus } from '$lib/types';

  const selectedRepo = $derived(repos.getSelected());
  const workspaceList = $derived(selectedRepo ? workspaces.listForRepo(selectedRepo.id) : []);

  let showForm = $state(false);
  let formTitle = $state('');
  let formDescription = $state('');
  let formBranch = $state('');
  let formSubmitting = $state(false);

  function statusDotClass(status: WorkspaceStatus): string {
    if (status === 'running') return 'bg-amber-400';
    if (status === 'waiting') return 'bg-[var(--status-ok)]';
    return 'bg-[var(--text-muted)]';
  }

  function handleSelectWorkspace(id: string) {
    workspaces.select(id);
  }

  async function handleRemoveWorkspace(e: MouseEvent, wsId: string, repoId: string) {
    e.stopPropagation();
    if (!window.confirm('Remove this workspace? The git worktree will be deleted.')) return;
    try {
      await workspaces.remove(wsId, repoId);
    } catch (err) {
      console.error('Failed to remove workspace:', err);
    }
  }

  async function handleCreateSubmit(e: Event) {
    e.preventDefault();
    if (!selectedRepo || !formTitle.trim()) return;
    formSubmitting = true;
    try {
      await workspaces.create({
        repoId: selectedRepo.id,
        title: formTitle.trim(),
        description: formDescription.trim(),
        branchName: formBranch.trim() || undefined,
      });
      formTitle = '';
      formDescription = '';
      formBranch = '';
      showForm = false;
    } catch (err) {
      console.error('Failed to create workspace:', err);
    } finally {
      formSubmitting = false;
    }
  }

  function handleCancelForm() {
    showForm = false;
    formTitle = '';
    formDescription = '';
    formBranch = '';
  }
</script>

<aside
  class="flex flex-col h-full w-full bg-[var(--bg-sidebar)] border-r border-[var(--border)] overflow-hidden"
>
  <div class="flex items-center justify-between px-3 py-2 border-b border-[var(--border)]">
    <span class="text-xs font-semibold uppercase tracking-wider text-[var(--text-muted)]">
      Workspaces
    </span>
    <button
      class="text-xs font-semibold px-2 py-0.5 rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
      onclick={() => {
        showForm = !showForm;
      }}
      aria-label="New Workspace"
    >
      + New Workspace
    </button>
  </div>

  {#if showForm}
    <!-- Inline new workspace form -->
    <form
      class="flex flex-col gap-2 px-3 py-2 border-b border-[var(--border)] bg-[var(--bg-card)]"
      onsubmit={handleCreateSubmit}
    >
      <input
        class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        type="text"
        placeholder="Workspace title"
        bind:value={formTitle}
        required
      />
      <textarea
        class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none"
        placeholder="Description (optional)"
        rows={2}
        bind:value={formDescription}
      ></textarea>
      <input
        class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        type="text"
        placeholder="Branch name (optional)"
        bind:value={formBranch}
      />
      <div class="flex gap-2">
        <button
          type="submit"
          class="flex-1 py-1 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 cursor-pointer"
          disabled={formSubmitting || !formTitle.trim()}
          aria-label="Create"
        >
          {formSubmitting ? 'Creating…' : 'Create'}
        </button>
        <button
          type="button"
          class="py-1 px-2 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          onclick={handleCancelForm}
        >
          Cancel
        </button>
      </div>
    </form>
  {/if}

  <ul class="flex-1 overflow-y-auto py-1">
    {#each workspaceList as ws (ws.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <li
        class="group flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
        class:bg-[var(--bg-active)]={workspaces.selectedWorkspaceId === ws.id}
        onclick={() => handleSelectWorkspace(ws.id)}
      >
        <!-- Status dot -->
        <span
          class="w-2 h-2 rounded-full flex-shrink-0 {statusDotClass(ws.status)}"
          data-status-dot
          data-status={ws.status}
          aria-label="Status: {ws.status}"
        ></span>

        <!-- Title -->
        <span
          class="flex-1 text-xs text-[var(--text-secondary)] overflow-hidden text-ellipsis whitespace-nowrap group-hover:text-[var(--text-primary)] transition-colors"
        >
          {ws.title}
        </span>

        <!-- Remove button -->
        <button
          class="opacity-0 group-hover:opacity-100 flex items-center justify-center w-4 h-4 rounded text-[var(--text-muted)] hover:text-[var(--error)] hover:bg-[var(--error-bg)] transition-all cursor-pointer"
          onclick={(e) => handleRemoveWorkspace(e, ws.id, ws.repo_id)}
          aria-label="Remove workspace"
          title="Remove workspace"
        >
          ×
        </button>
      </li>
    {:else}
      <li class="px-3 py-3 text-xs text-[var(--text-muted)] text-center">
        {#if selectedRepo}
          No workspaces yet
        {:else}
          Select a repo to see workspaces
        {/if}
      </li>
    {/each}
  </ul>
</aside>
