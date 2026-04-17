<!-- src/lib/components/kanban/KanbanBoard.svelte -->
<script lang="ts">
  import type { Task, KanbanColumn } from '$lib/types';
  import TaskCard from './TaskCard.svelte';

  const {
    repoId,
    tasks,
    onMove: _onMove,
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

  function tasksForColumn(column: KanbanColumn): Task[] {
    return tasks.filter((t) => t.column === column).sort((a, b) => a.order - b.order);
  }
</script>

<div class="kanban-board" data-repo={repoId}>
  {#each COLUMNS as col (col.id)}
    <div class="kanban-column" data-column={col.id}>
      <div class="kanban-column__header">
        <span class="kanban-column__title">{col.label}</span>
        <span class="kanban-column__count">{tasksForColumn(col.id).length}</span>
      </div>

      <div class="kanban-column__body" role="list" aria-label="{col.label} tasks">
        {#each tasksForColumn(col.id) as task (task.id)}
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
