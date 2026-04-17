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

<div class="task-card" role="listitem" aria-label={task.title}>
  <div class="task-card__header">
    <span class="task-card__title">{task.title}</span>
    <button class="task-card__remove" aria-label="Remove task" onclick={() => onRemove(task.id)}>
      ×
    </button>
  </div>

  {#if task.description}
    <p class="task-card__description" data-testid="task-description">
      {truncatedDescription}
    </p>
  {/if}

  <div class="task-card__footer">
    {#if task.workspace_id}
      <span class="task-card__branch-badge" data-testid="branch-badge"> branch </span>
    {/if}
    <span class="task-card__date">{relativeDate()}</span>
  </div>
</div>
