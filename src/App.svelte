<!-- src/App.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';

  const selectedWorkspace = $derived(workspaces.getSelected());

  onMount(async () => {
    await repos.load();
    const selected = repos.getSelected();
    if (selected) {
      await workspaces.loadForRepo(selected.id);
    }
  });
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
    <TitleBar />
  </div>

  <!-- Sidebar: bottom-left -->
  <div style="grid-column: 1; grid-row: 2; overflow: hidden;">
    <Sidebar />
  </div>

  <!-- Main: bottom-right -->
  <main
    class="bg-[var(--bg-base)] overflow-auto flex items-center justify-center"
    style="grid-column: 2; grid-row: 2;"
  >
    {#if selectedWorkspace}
      <section class="flex flex-col items-center gap-2 text-[var(--text-secondary)]">
        <p class="text-base font-semibold text-[var(--text-primary)]">
          Workspace: {selectedWorkspace.title}
        </p>
        <p class="text-xs text-[var(--text-muted)]">
          Branch: {selectedWorkspace.branch}
        </p>
        <!-- Phase 1b/c will replace this placeholder -->
      </section>
    {:else}
      <p class="text-sm text-[var(--text-muted)]">Select or create a workspace</p>
    {/if}
  </main>
</div>
