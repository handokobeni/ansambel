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

  let columnItems = $state<Record<KanbanColumn, Task[]>>({
    todo: [],
    in_progress: [],
    review: [],
    done: [],
  });

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
    const rawOrder = e.detail.items.findIndex((t) => t.id === droppedId);
    const items = e.detail.items.filter(
      (t) => !(t as Task & Record<string, unknown>)[SHADOW_ITEM_MARKER_PROPERTY_NAME]
    );
    columnItems = { ...columnItems, [column]: items };
    const newOrder = rawOrder !== -1 ? rawOrder : 0;
    onMove(droppedId, column, newOrder);
  }
</script>

<div
  class="kanban-board grid grid-cols-4 gap-3 p-3 h-full overflow-x-auto min-w-0"
  data-repo={repoId}
>
  {#each COLUMNS as col (col.id)}
    <div
      class="kanban-column flex flex-col rounded bg-[var(--bg-card)] border border-[var(--border)] min-w-[220px] max-h-full overflow-hidden"
    >
      <div
        class="kanban-column__header flex items-center justify-between px-3 py-2 border-b border-[var(--border)]"
      >
        <span
          class="text-xs font-semibold uppercase tracking-wider text-[var(--text-muted)] truncate"
        >
          {col.label}
        </span>
        <span
          class="ml-2 inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 text-[10px] font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)]"
        >
          {columnItems[col.id].length}
        </span>
      </div>

      <div
        class="kanban-column__body flex-1 overflow-y-auto p-2 space-y-2 min-h-[80px]"
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
          <p class="text-xs text-[var(--text-muted)] text-center py-4 italic">No tasks</p>
        {/each}
      </div>

      {#if col.id === 'todo'}
        <button
          class="m-2 mt-0 px-2 py-1.5 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-active)] transition-colors cursor-pointer"
          onclick={onAddTask}
        >
          + Add task
        </button>
      {/if}
    </div>
  {/each}
</div>
