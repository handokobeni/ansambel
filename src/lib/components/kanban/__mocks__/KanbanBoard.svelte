<!-- Stub KanbanBoard used in tests: renders column labels (so existing
     assertions that check for column header text keep passing) and exposes
     trigger buttons so handleMove can be exercised without svelte-dnd-action. -->
<script lang="ts">
  import type { KanbanColumn } from '$lib/types';

  const {
    onMove,
  }: {
    repoId?: string;
    tasks?: unknown[];
    onMove: (taskId: string, column: KanbanColumn, order: number) => void;
    onAddTask?: () => void;
    onRemoveTask?: (taskId: string) => void;
  } = $props();
</script>

<div data-testid="mock-kanban">
  <!-- Column labels preserved so existing tests that query for them still pass -->
  <span>Todo</span>
  <span>In Progress</span>
  <span>Review</span>
  <span>Done</span>
  <!-- Trigger buttons let tests invoke onMove directly -->
  <button data-testid="trigger-move-todo" onclick={() => onMove('task_trigger', 'todo', 0)}>
    Trigger move to todo
  </button>
  <button
    data-testid="trigger-move-in-progress"
    onclick={() => onMove('task_trigger', 'in_progress', 0)}
  >
    Trigger move to in_progress
  </button>
</div>
