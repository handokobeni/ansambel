<!-- src/lib/components/App.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { ShortcutRegistry } from '$lib/keyboard';
  import { modeStore } from '$lib/stores/mode.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import { repos } from '$lib/stores/repos.svelte';
  import TitleBar from './TitleBar.svelte';
  import KanbanBoard from './kanban/KanbanBoard.svelte';
  import NewTaskDialog from './kanban/NewTaskDialog.svelte';
  import type { KanbanColumn } from '$lib/types';

  let registry: ShortcutRegistry;
  let showNewTask = $state(false);

  const selectedRepo = $derived(repos.getSelected());

  const boardTasks = $derived(selectedRepo ? tasks.listForRepo(selectedRepo.id) : []);

  onMount(async () => {
    registry = new ShortcutRegistry();
    registry.register('ctrl+1', () => modeStore.set('plan'));
    registry.register('ctrl+2', () => modeStore.set('work'));
    registry.register('ctrl+n', () => {
      if (modeStore.mode === 'plan') showNewTask = true;
    });
    registry.register('ctrl+,', () => {
      // Settings — no-op until Phase 2
    });
    registry.register('ctrl+e', () => {
      // Focus repo dropdown — no-op until Phase 2
    });

    await repos.load();
    if (repos.selectedRepoId) {
      await tasks.loadForRepo(repos.selectedRepoId);
    }
  });

  onDestroy(() => {
    registry?.destroy();
  });

  async function handleMove(taskId: string, column: KanbanColumn, order: number) {
    await tasks.move(taskId, column, order);
  }

  async function handleAddTask(data: { title: string; description: string }) {
    if (!selectedRepo) return;
    await tasks.add({
      repoId: selectedRepo.id,
      title: data.title,
      description: data.description,
      column: 'todo',
    });
    showNewTask = false;
  }

  async function handleRemoveTask(taskId: string) {
    await tasks.remove(taskId);
  }
</script>

<div class="app">
  <TitleBar mode={modeStore.mode} onModeChange={(next) => modeStore.set(next)} />

  <main class="app__main">
    {#if modeStore.mode === 'plan'}
      {#if selectedRepo}
        <KanbanBoard
          repoId={selectedRepo.id}
          tasks={boardTasks}
          onMove={handleMove}
          onAddTask={() => (showNewTask = true)}
          onRemoveTask={handleRemoveTask}
        />
      {:else}
        <div class="app__empty">
          <p>No repository selected. Add a repo to get started.</p>
        </div>
      {/if}
    {:else}
      <section class="work-placeholder">
        <p>Work mode — chat panel coming in Phase 1c.</p>
      </section>
    {/if}
  </main>

  <NewTaskDialog
    open={showNewTask}
    onSubmit={handleAddTask}
    onCancel={() => (showNewTask = false)}
  />
</div>
