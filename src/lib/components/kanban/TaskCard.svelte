<!-- src/lib/components/kanban/TaskCard.svelte -->
<script lang="ts">
  import type { Task } from '$lib/types';

  const { task, onRemove }: { task: Task; onRemove: (id: string) => void } = $props();

  const truncatedDescription = $derived(
    task.description.length > 80 ? task.description.slice(0, 80) + '...' : task.description
  );

  const relativeDate = $derived(() => {
    const diff = Date.now() / 1000 - task.created_at;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  });
</script>

<div
  class="task-card group flex flex-col gap-1.5 p-2 rounded bg-[var(--bg-base)] border border-[var(--border-light)] hover:border-[var(--accent)] cursor-grab active:cursor-grabbing transition-colors"
  role="listitem"
  aria-label={task.title}
>
  <div class="flex items-start justify-between gap-2">
    <span class="text-xs font-semibold text-[var(--text-primary)] flex-1 leading-snug">
      {task.title}
    </span>
    <button
      class="opacity-0 group-hover:opacity-100 flex items-center justify-center w-5 h-5 rounded text-sm leading-none text-[var(--text-muted)] hover:text-[var(--error)] hover:bg-[var(--error-bg)] transition-all cursor-pointer flex-shrink-0"
      aria-label="Remove task"
      onclick={() => onRemove(task.id)}
    >
      ×
    </button>
  </div>

  {#if task.description}
    <p class="text-[11px] text-[var(--text-secondary)] leading-snug" data-testid="task-description">
      {truncatedDescription}
    </p>
  {/if}

  <div class="flex items-center justify-between gap-2 mt-0.5">
    {#if task.workspace_id}
      <span
        class="inline-flex items-center px-1.5 py-0.5 text-[10px] font-mono rounded bg-[var(--bg-hover)] text-[var(--accent)]"
        data-testid="branch-badge"
      >
        branch
      </span>
    {:else}
      <span></span>
    {/if}
    <span class="text-[10px] text-[var(--text-muted)]">{relativeDate()}</span>
  </div>
</div>
