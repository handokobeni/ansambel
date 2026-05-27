<script lang="ts">
  import TerminalTabBar from './TerminalTabBar.svelte';
  import TerminalPane from './TerminalPane.svelte';
  import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';
  import { api } from '$lib/ipc';

  interface Props {
    workspaceId: string;
  }
  const { workspaceId }: Props = $props();

  const tabs = $derived(terminalTabs.list(workspaceId));
  const active = $derived(terminalTabs.activeId(workspaceId));

  // First-open: when this workspace has no terminals yet, create one so
  // opening the Terminal tab always shows a live shell (matches the old
  // single-terminal UX). Guard prevents a re-fire loop: it only adds when
  // the list is empty, and adding makes it non-empty.
  $effect(() => {
    if (terminalTabs.list(workspaceId).length === 0) {
      terminalTabs.add(workspaceId);
    }
  });

  function addTerminal(): void {
    terminalTabs.add(workspaceId); // the new pane spawns its PTY lazily on mount
  }

  function closeTerminal(id: string): void {
    // Kill the PTY first, then drop the tab.
    void api.terminal.kill(id);
    terminalTabs.close(workspaceId, id);
  }
</script>

<div class="flex flex-col h-full bg-[var(--bg-base)]" data-testid="terminal-view">
  {#if tabs.length === 0}
    <div class="flex-1 flex items-center justify-center text-sm text-[var(--text-muted)]">
      <button
        type="button"
        class="px-3 py-1.5 rounded border border-[var(--border)] hover:bg-[var(--bg-hover)]"
        onclick={addTerminal}
      >
        + New terminal
      </button>
    </div>
  {:else}
    <TerminalTabBar {workspaceId} onAdd={addTerminal} onClose={closeTerminal} />
    <div class="flex-1 relative overflow-hidden">
      {#each tabs as tab (tab.id)}
        <div
          class="absolute inset-0"
          class:hidden={active !== tab.id}
          data-testid="terminal-pane-host"
          data-id={tab.id}
        >
          <TerminalPane {workspaceId} terminalId={tab.id} />
        </div>
      {/each}
    </div>
  {/if}
</div>
