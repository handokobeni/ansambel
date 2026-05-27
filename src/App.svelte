<!-- src/App.svelte -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toasts from '$lib/components/Toasts.svelte';
  import KanbanBoard from '$lib/components/kanban/KanbanBoard.svelte';
  import NewTaskDialog from '$lib/components/kanban/NewTaskDialog.svelte';
  import WorkspaceView from '$lib/components/workspace/WorkspaceView.svelte';
  import SearchModal from '$lib/components/workspace/SearchModal.svelte';
  import TeamWorkspaceMirror from '$lib/components/team/TeamWorkspaceMirror.svelte';
  import { teamActivity } from '$lib/stores/team-activity.svelte';
  import { workspaceTabs } from '$lib/stores/workspace-tabs.svelte';
  import type { SearchMode } from '$lib/types';
  import { listen } from '@tauri-apps/api/event';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import { larkBindings } from '$lib/stores/lark-bindings.svelte';
  import { modeStore } from '$lib/stores/mode.svelte';
  import { theme } from '$lib/stores/theme.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import { ShortcutRegistry } from '$lib/keyboard';
  import type { KanbanColumn } from '$lib/types';

  let registry: ShortcutRegistry | undefined;
  let unlistenMigrated: (() => void) | null = null;
  let showNewTask = $state(false);
  let searchOpen = $state(false);
  let searchMode = $state<SearchMode>('filename');
  let highlightedFile = $state<string | null>(null);

  const selectedRepo = $derived(repos.getSelected());
  const selectedWorkspace = $derived(workspaces.getSelected());
  const boardTasks = $derived(selectedRepo ? tasks.listForRepo(selectedRepo.id) : []);

  onMount(async () => {
    // Apply CSS variables and start tracking system color-scheme changes so
    // a `system` color mode tracks the OS in real time. Must run before any
    // surface relies on theme tokens — keep this at the top of onMount.
    theme.initTheme();

    registry = new ShortcutRegistry();
    registry.register('ctrl+1', () => modeStore.set('plan'));
    registry.register('ctrl+2', () => modeStore.set('work'));
    registry.register('ctrl+shift+l', () => theme.toggleColorMode());
    registry.register('ctrl+n', () => {
      if (modeStore.mode === 'plan' && selectedRepo) showNewTask = true;
    });
    registry.register('ctrl+,', () => {
      // Settings — no-op until Phase 2
    });
    registry.register('ctrl+e', () => {
      // Focus repo dropdown — no-op until Phase 2
    });
    // Phase 2a search modal — Ctrl+P opens filename mode, Ctrl+Shift+F
    // opens content mode. Both require an active workspace; the no-op
    // branch below is intentional for the Plan-mode case.
    registry.register('ctrl+p', () => {
      if (!selectedWorkspace) return;
      searchMode = 'filename';
      searchOpen = true;
    });
    registry.register('ctrl+shift+f', () => {
      if (!selectedWorkspace) return;
      searchMode = 'content';
      searchOpen = true;
    });
    // Phase 2b: workspace tab shortcuts. ⌃1-3 are claimed by mode/plan
    // shortcuts above; tabs 4 (Editor) and 5 (Terminal) get keyboard
    // bindings here. Both are no-ops without a selected workspace.
    registry.register('ctrl+4', () => {
      if (selectedWorkspace) workspaceTabs.setActive(selectedWorkspace.id, 'editor');
    });
    registry.register('ctrl+5', () => {
      if (selectedWorkspace) workspaceTabs.setActive(selectedWorkspace.id, 'terminal');
    });

    await repos.load();
    await larkBindings.load();
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
      const repoId = repos.selectedRepoId;
      // If the repo has a Lark binding, use refresh() so the first paint
      // reflects the persisted filter (refresh calls the Lark provider which
      // applies binding.filters server-side). For local-only repos, loadForRepo
      // reads the AppState mirror which is populated on startup from disk.
      const taskLoad = larkBindings.has(repoId) ? tasks.refresh(repoId) : tasks.loadForRepo(repoId);
      await Promise.all([workspaces.loadForRepo(repoId), taskLoad]);
    }

    listen<string>('lark-migrated', () => {
      addToast('Lark config migrated. Review the mapping in repo settings.', 'info');
    }).then((u) => {
      unlistenMigrated = u;
    });
  });

  // Window-focus refresh: when the OS window regains focus and the active
  // task source is 'lark', debounce 2 s then pull fresh tasks from Bitable.
  onMount(() => {
    let focusDebounce: ReturnType<typeof setTimeout> | null = null;

    async function handleFocus() {
      const repo = repos.getSelected();
      if (!repo) return;
      if (!larkBindings.has(repo.id)) return; // local mode, no refresh
      if (focusDebounce) clearTimeout(focusDebounce);
      focusDebounce = setTimeout(() => {
        tasks.refresh(repo.id).catch(() => {});
      }, 2000);
    }

    window.addEventListener('focus', handleFocus);
    return () => {
      window.removeEventListener('focus', handleFocus);
      if (focusDebounce) clearTimeout(focusDebounce);
    };
  });

  onDestroy(() => {
    registry?.destroy();
    unlistenMigrated?.();
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
    grid-template-columns: auto 1fr;
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
    {#if teamActivity.selectedWorkspaceId}
      <TeamWorkspaceMirror />
    {:else if modeStore.mode === 'plan'}
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
      <WorkspaceView workspace={selectedWorkspace} {highlightedFile} />
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

  <SearchModal
    open={searchOpen}
    workspaceId={selectedWorkspace?.id ?? null}
    initialMode={searchMode}
    onClose={() => (searchOpen = false)}
    onJump={(path) => {
      if (selectedWorkspace) {
        // Switch to Files and stamp the path so the tree highlights and
        // expands ancestors. The user clicks the row to actually open it
        // in the Editor — the FileBrowser onOpen wire handles that.
        workspaceTabs.setActive(selectedWorkspace.id, 'files');
        highlightedFile = path;
      }
    }}
  />

  <Toasts />
</div>
