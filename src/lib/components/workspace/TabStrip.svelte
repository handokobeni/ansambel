<script lang="ts">
  import type { WorkspaceTabId } from '$lib/types';

  interface Props {
    active: WorkspaceTabId;
    onSelect: (tab: WorkspaceTabId) => void;
  }

  const { active, onSelect }: Props = $props();

  const TABS: { id: WorkspaceTabId; label: string; shortcut: string }[] = [
    { id: 'chat', label: 'Chat', shortcut: '1' },
    { id: 'diff', label: 'Diff', shortcut: '2' },
    { id: 'files', label: 'Files', shortcut: '3' },
  ];
</script>

<div
  role="tablist"
  aria-label="Workspace tabs"
  class="flex items-stretch gap-px border-b border-[var(--border)] bg-[var(--bg-sidebar)] text-sm"
  data-testid="tab-strip"
>
  {#each TABS as tab (tab.id)}
    <button
      type="button"
      role="tab"
      data-testid="tab-{tab.id}"
      data-tab-id={tab.id}
      aria-selected={active === tab.id}
      aria-controls="tabpanel-{tab.id}"
      tabindex={active === tab.id ? 0 : -1}
      onclick={() => onSelect(tab.id)}
      class="px-4 py-2 border-b-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
      class:active={active === tab.id}
      class:border-transparent={active !== tab.id}
      class:border-b-[var(--accent)]={active === tab.id}
      class:text-[var(--text-primary)]={active === tab.id}
      class:text-[var(--text-secondary)]={active !== tab.id}
      class:hover:text-[var(--text-primary)]={active !== tab.id}
      class:hover:bg-[var(--bg-card)]={active !== tab.id}
    >
      {tab.label}
      <span class="ml-2 text-xs text-[var(--text-muted)]" aria-hidden="true">⌃{tab.shortcut}</span>
    </button>
  {/each}
</div>
