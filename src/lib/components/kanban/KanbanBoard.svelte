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

  // Per-column local state managed primarily by svelte-dnd-action's
  // consider/finalize callbacks. The $effect below only re-hydrates from
  // the parent `tasks` prop when the SET of task IDs changes (add/remove),
  // so in-place column/order/workspace_id mutations (from tasks.move()) do
  // NOT clobber dnd-action's internal tracking mid-drag.
  let todoItems = $state<Task[]>([]);
  let inProgressItems = $state<Task[]>([]);
  let reviewItems = $state<Task[]>([]);
  let doneItems = $state<Task[]>([]);

  let lastIds = new Set<string>();

  function filterSort(col: KanbanColumn, ts: Task[]): Task[] {
    return ts.filter((t) => t.column === col).sort((a, b) => a.order - b.order);
  }

  function syncInPlace(arr: Task[], byId: Map<string, Task>): void {
    // Update each existing array element in place when the task in `tasks`
    // has newer display fields (workspace_id, title, description, updated_at).
    // Preserves the array reference so svelte-dnd-action doesn't lose
    // its internal drag state.
    for (let i = 0; i < arr.length; i++) {
      const incoming = byId.get(arr[i].id);
      if (!incoming) continue;
      const existing = arr[i];
      if (
        existing.workspace_id !== incoming.workspace_id ||
        existing.title !== incoming.title ||
        existing.description !== incoming.description ||
        existing.updated_at !== incoming.updated_at
      ) {
        arr[i] = { ...incoming, column: existing.column, order: existing.order };
      }
    }
  }

  $effect(() => {
    const nextIds = new Set(tasks.map((t) => t.id));
    const sameSet = nextIds.size === lastIds.size && [...nextIds].every((id) => lastIds.has(id));
    if (!sameSet) {
      lastIds = nextIds;
      todoItems = filterSort('todo', tasks);
      inProgressItems = filterSort('in_progress', tasks);
      reviewItems = filterSort('review', tasks);
      doneItems = filterSort('done', tasks);
      return;
    }
    // Same ID set — propagate in-place field updates (e.g., workspace_id
    // populated by backend after a move) without replacing array refs.
    const byId = new Map(tasks.map((t) => [t.id, t]));
    syncInPlace(todoItems, byId);
    syncInPlace(inProgressItems, byId);
    syncInPlace(reviewItems, byId);
    syncInPlace(doneItems, byId);
  });

  function itemsFor(col: KanbanColumn): Task[] {
    switch (col) {
      case 'todo':
        return todoItems;
      case 'in_progress':
        return inProgressItems;
      case 'review':
        return reviewItems;
      case 'done':
        return doneItems;
    }
  }

  function setItems(col: KanbanColumn, next: Task[]): void {
    switch (col) {
      case 'todo':
        todoItems = next;
        break;
      case 'in_progress':
        inProgressItems = next;
        break;
      case 'review':
        reviewItems = next;
        break;
      case 'done':
        doneItems = next;
        break;
    }
  }

  function handleConsider(column: KanbanColumn, e: CustomEvent<{ items: Task[] }>) {
    setItems(column, e.detail.items);
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
    setItems(column, items);
    // Only fire onMove when this column is actually the item's destination
    // — svelte-dnd-action fires finalize on the source zone too (with the
    // shadow item removed), which we must ignore so we don't double-call.
    if (rawOrder !== -1) {
      onMove(droppedId, column, rawOrder);
    }
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
          {itemsFor(col.id).length}
        </span>
      </div>

      <div
        class="kanban-column__body flex-1 overflow-y-auto p-2 space-y-2 min-h-[80px]"
        data-column={col.id}
        role="list"
        aria-label="{col.label} tasks"
        use:dndzone={{ items: itemsFor(col.id), flipDurationMs: 150 }}
        onconsider={(e) => handleConsider(col.id, e)}
        onfinalize={(e) => handleFinalize(col.id, e)}
      >
        {#each itemsFor(col.id) as task (task.id)}
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
