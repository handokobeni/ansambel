<script lang="ts">
  import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';

  interface Props {
    workspaceId: string;
    onAdd?: () => void;
    onClose?: (id: string) => void;
  }
  const { workspaceId, onAdd, onClose }: Props = $props();

  const MAX = 6;
  const tabs = $derived(terminalTabs.list(workspaceId));
  const active = $derived(terminalTabs.activeId(workspaceId));

  function close(e: MouseEvent, id: string): void {
    e.stopPropagation();
    onClose?.(id);
  }
</script>

<div
  role="tablist"
  aria-label="Terminals"
  class="flex items-stretch gap-px border-b border-[var(--border)] bg-[var(--bg-sidebar)] text-xs overflow-x-auto"
  data-testid="terminal-tab-bar"
>
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      role="tab"
      aria-selected={active === tab.id}
      data-testid="terminal-tab"
      data-id={tab.id}
      onclick={() => terminalTabs.setActive(workspaceId, tab.id)}
      class="px-3 py-1.5 flex items-center gap-2 border-b-2 {active === tab.id
        ? 'border-b-[var(--accent)] text-[var(--text-primary)]'
        : 'border-transparent text-[var(--text-secondary)] hover:bg-[var(--bg-card)]'}"
    >
      <span class="truncate">{tab.label}</span>
      <span
        role="button"
        tabindex="0"
        aria-label="Close {tab.label}"
        data-testid="terminal-tab-close"
        data-id={tab.id}
        onclick={(e) => close(e, tab.id)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') close(e as unknown as MouseEvent, tab.id);
        }}
        class="ml-1 px-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
      >
        ✕
      </span>
    </button>
  {/each}
  <button
    type="button"
    aria-label="New terminal"
    data-testid="terminal-tab-add"
    disabled={tabs.length >= MAX}
    onclick={() => onAdd?.()}
    class="px-3 py-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] disabled:opacity-40 disabled:cursor-not-allowed"
  >
    +
  </button>
</div>
