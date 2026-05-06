<!-- src/App.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import KanbanBoard from '$lib/components/kanban/KanbanBoard.svelte';
  import NewTaskDialog from '$lib/components/kanban/NewTaskDialog.svelte';
  import WorkspaceView from '$lib/components/workspace/WorkspaceView.svelte';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import { modeStore } from '$lib/stores/mode.svelte';
  import { ShortcutRegistry } from '$lib/keyboard';
  import type { KanbanColumn } from '$lib/types';

  let registry: ShortcutRegistry | undefined;
  let showNewTask = $state(false);

  const selectedRepo = $derived(repos.getSelected());
  const selectedWorkspace = $derived(workspaces.getSelected());
  const boardTasks = $derived(selectedRepo ? tasks.listForRepo(selectedRepo.id) : []);

  onMount(async () => {
    registry = new ShortcutRegistry();
    registry.register('ctrl+1', () => modeStore.set('plan'));
    registry.register('ctrl+2', () => modeStore.set('work'));
    registry.register('ctrl+n', () => {
      if (modeStore.mode === 'plan' && selectedRepo) showNewTask = true;
    });
    registry.register('ctrl+,', () => {
      // Settings — no-op until Phase 2
    });
    registry.register('ctrl+e', () => {
      // Focus repo dropdown — no-op until Phase 2
    });

    await repos.load();
    // Cold-start auto-select: selectedRepoId is in-memory only, so on every
    // restart it lands as null. Without this fallback the kanban renders
    // "Add a repo to start" even when tasks.json/workspaces.json on disk
    // have content — the user has to re-Add the repo to repopulate the
    // board. Pick the first repo when nothing is selected and the list is
    // non-empty; the existing if-block then hydrates tasks + workspaces.
    if (!repos.selectedRepoId) {
      const firstRepoId = repos.repos.keys().next().value;
      if (firstRepoId) {
        repos.select(firstRepoId);
      }
    }
    if (repos.selectedRepoId) {
      await Promise.all([
        workspaces.loadForRepo(repos.selectedRepoId),
        tasks.loadForRepo(repos.selectedRepoId),
      ]);
    }
  });

  onDestroy(() => {
    registry?.destroy();
  });

  async function handleMove(taskId: string, column: KanbanColumn, order: number) {
    await tasks.move(taskId, column, order);
    // After a move, workspaces may have been auto-created by the backend; re-sync.
    if (selectedRepo) {
      await workspaces.loadForRepo(selectedRepo.id);
    }
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

<div
  class="app-shell"
  style="
    display: grid;
    grid-template-rows: auto 1fr;
    grid-template-columns: 260px 1fr;
    height: 100vh;
    overflow: hidden;
  "
>
  <!-- TitleBar: spans both columns -->
  <div style="grid-column: 1 / -1; grid-row: 1;">
    <TitleBar mode={modeStore.mode} onModeChange={(next) => modeStore.set(next)} />
  </div>

  <!-- Sidebar: bottom-left -->
  <div style="grid-column: 1; grid-row: 2; overflow: hidden;">
    <Sidebar />
  </div>

  <!-- Main: bottom-right -->
  <main class="bg-[var(--bg-base)] overflow-auto" style="grid-column: 2; grid-row: 2;">
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
        <div class="h-full flex items-center justify-center text-sm text-[var(--text-muted)]">
          Add a repo to start managing tasks.
        </div>
      {/if}
    {:else if selectedWorkspace}
      <WorkspaceView workspace={selectedWorkspace} />
    {:else}
      <div class="h-full flex items-center justify-center text-sm text-[var(--text-muted)]">
        Select or create a workspace
      </div>
    {/if}
  </main>

  <NewTaskDialog
    open={showNewTask}
    onSubmit={handleAddTask}
    onCancel={() => (showNewTask = false)}
  />
</div>
