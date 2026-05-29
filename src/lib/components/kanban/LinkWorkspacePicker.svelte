<!-- src/lib/components/kanban/LinkWorkspacePicker.svelte -->
<script lang="ts">
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';

  interface Props {
    taskId: string;
    repoId: string;
    open: boolean;
    onClose: () => void;
    /** Optional: when set, the picker's pick handler first unlinks the
     *  card from this workspace (force=true, which triggers cleanup of
     *  the auto-created empty workspace) before linking to the chosen
     *  one. Used by the Task 12 auto-create undo toast's
     *  "Link to existing instead" affordance. */
    cleanupWorkspaceOnPick?: string | null;
  }

  const { taskId, repoId, open, onClose, cleanupWorkspaceOnPick = null }: Props = $props();

  const rows = $derived(
    [...workspaces.listForRepo(repoId)].sort((a, b) => b.updated_at - a.updated_at)
  );

  async function pick(wsId: string): Promise<void> {
    if (cleanupWorkspaceOnPick) {
      // Drop the auto-created empty workspace first so the link below
      // doesn't go through an atomic-switch that re-runs cleanup logic
      // already accounted for by the toast flow.
      await tasks.unlink(taskId, true, repoId);
    }
    await tasks.link(taskId, wsId, repoId);
    onClose();
  }

  function relativeTime(unixSec: number): string {
    const diff = Date.now() / 1000 - unixSec;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86_400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86_400)}d ago`;
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="dialog-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    onclick={onClose}
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Link to workspace"
      tabindex="-1"
      class="link-workspace-picker relative w-[440px] max-w-[90vw] p-4 rounded-lg bg-[var(--bg-card)] border border-[var(--border-light)] shadow-2xl text-[var(--text-primary)]"
      onclick={(e) => e.stopPropagation()}
    >
      <h3 class="text-sm font-semibold mb-2">Link to workspace</h3>
      {#if rows.length === 0}
        <p class="text-xs text-[var(--text-muted)]" data-testid="link-picker-empty">
          No workspaces in this repo yet. Move a card to In Progress to create one.
        </p>
      {:else}
        <ul class="space-y-1 max-h-[360px] overflow-y-auto">
          {#each rows as ws (ws.id)}
            <li>
              <button
                type="button"
                class="w-full text-left px-2 py-1.5 rounded hover:bg-[var(--bg-hover)] flex items-center gap-2 text-xs"
                data-testid="link-picker-row"
                onclick={() => pick(ws.id)}
              >
                <span class="font-medium truncate text-[var(--text-primary)] max-w-[140px]"
                  >{ws.title}</span
                >
                <span class="text-[var(--text-muted)] font-mono">{ws.branch}</span>
                <span class="text-[var(--text-muted)]"
                  >· {ws.task_ids.length} card{ws.task_ids.length === 1 ? '' : 's'}</span
                >
                <span class="ml-auto text-[var(--text-muted)]">{relativeTime(ws.updated_at)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
{/if}
