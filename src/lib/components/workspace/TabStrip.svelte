<script lang="ts">
  import type { WorkspaceTabId } from '$lib/types';

  interface Props {
    active: WorkspaceTabId;
    onSelect: (tab: WorkspaceTabId) => void;
  }

  const { active, onSelect }: Props = $props();

  const BASE =
    'px-4 py-2 border-b-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]';
  const ACTIVE = 'border-b-[var(--accent)] text-[var(--text-primary)]';
  const INACTIVE =
    'border-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-card)]';

  function classFor(id: WorkspaceTabId): string {
    return `${BASE} ${active === id ? ACTIVE : INACTIVE}`;
  }
</script>

<div
  role="tablist"
  aria-label="Workspace tabs"
  class="flex items-stretch gap-px border-b border-[var(--border)] bg-[var(--bg-sidebar)] text-sm"
  data-testid="tab-strip"
>
  <button
    type="button"
    role="tab"
    data-testid="tab-chat"
    aria-selected={active === 'chat'}
    aria-controls="tabpanel-chat"
    tabindex={active === 'chat' ? 0 : -1}
    onclick={() => onSelect('chat')}
    class={classFor('chat')}
  >
    Chat <span class="ml-2 text-xs text-[var(--text-muted)]" aria-hidden="true">⌃1</span>
  </button>
  <button
    type="button"
    role="tab"
    data-testid="tab-diff"
    aria-selected={active === 'diff'}
    aria-controls="tabpanel-diff"
    tabindex={active === 'diff' ? 0 : -1}
    onclick={() => onSelect('diff')}
    class={classFor('diff')}
  >
    Diff <span class="ml-2 text-xs text-[var(--text-muted)]" aria-hidden="true">⌃2</span>
  </button>
  <button
    type="button"
    role="tab"
    data-testid="tab-files"
    aria-selected={active === 'files'}
    aria-controls="tabpanel-files"
    tabindex={active === 'files' ? 0 : -1}
    onclick={() => onSelect('files')}
    class={classFor('files')}
  >
    Files <span class="ml-2 text-xs text-[var(--text-muted)]" aria-hidden="true">⌃3</span>
  </button>
</div>
