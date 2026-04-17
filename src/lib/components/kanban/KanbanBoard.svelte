<!-- src/lib/components/kanban/KanbanBoard.svelte -->
<script lang="ts">
  import { dndzone, SHADOW_ITEM_MARKER_PROPERTY_NAME } from 'svelte-dnd-action';
  import type { Task, KanbanColumn } from '$lib/types';
  import TaskCard from './TaskCard.svelte';

  const {
    repoId,
    tasks,
    onMove,
    onAddTask,
    onRemoveTask,
  }: {
    repoId: string;
    tasks: Task[];
    onMove: (taskId: string, column: KanbanColumn, order: number) => void;
    onAddTask: () => void;
    onRemoveTask: (taskId: string) => void;
  } = $props();

  type Column = {
    id: KanbanColumn;
    label: string;
  };

  const COLUMNS: Column[] = [
    { id: 'todo', label: 'Todo' },
    { id: 'in_progress', label: 'In Progress' },
    { id: 'review', label: 'Review' },
    { id: 'done', label: 'Done' },
  ];

  // Local mutable copies per column for dnd-action to manipulate during drag
  let columnItems = $state<Record<KanbanColumn, Task[]>>({
    todo: [],
    in_progress: [],
    review: [],
    done: [],
  });

  // Sync columnItems whenever parent tasks prop changes
  $effect(() => {
    const next: Record<KanbanColumn, Task[]> = {
      todo: [],
      in_progress: [],
      review: [],
      done: [],
    };
    for (const t of tasks) {
      next[t.column].push(t);
    }
    for (const col of COLUMNS) {
      next[col.id].sort((a, b) => a.order - b.order);
    }
    columnItems = next;
  });

  function handleConsider(column: KanbanColumn, e: CustomEvent<{ items: Task[] }>) {
    columnItems = { ...columnItems, [column]: e.detail.items };
  }

  function handleFinalize(
    column: KanbanColumn,
    e: CustomEvent<{ items: Task[]; info: { id: string } }>
  ) {
    const droppedId = e.detail.info.id;
    // Find new order from raw items (before filtering shadows) so position is accurate
    const rawOrder = e.detail.items.findIndex((t) => t.id === droppedId);
    const items = e.detail.items.filter(
      (t) => !(t as Task & Record<string, unknown>)[SHADOW_ITEM_MARKER_PROPERTY_NAME]
    );
    columnItems = { ...columnItems, [column]: items };
    const newOrder = rawOrder !== -1 ? rawOrder : 0;
    onMove(droppedId, column, newOrder);
  }
</script>

<div class="kanban-board" data-repo={repoId}>
  {#each COLUMNS as col (col.id)}
    <div class="kanban-column">
      <div class="kanban-column__header">
        <span class="kanban-column__title">{col.label}</span>
        <span class="kanban-column__count">{columnItems[col.id].length}</span>
      </div>

      <div
        class="kanban-column__body"
        data-column={col.id}
        role="list"
        aria-label="{col.label} tasks"
        use:dndzone={{ items: columnItems[col.id], flipDurationMs: 150 }}
        onconsider={(e) => handleConsider(col.id, e)}
        onfinalize={(e) => handleFinalize(col.id, e)}
      >
        {#each columnItems[col.id] as task (task.id)}
          <TaskCard {task} onRemove={onRemoveTask} />
        {:else}
          <p class="kanban-column__empty">No tasks</p>
        {/each}
      </div>

      {#if col.id === 'todo'}
        <button class="kanban-column__add-btn" onclick={onAddTask}> + Add task </button>
      {/if}
    </div>
  {/each}
</div>
