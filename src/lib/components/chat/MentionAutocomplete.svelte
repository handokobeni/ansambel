<script lang="ts">
  // Phase 2c — @-file mention autocomplete dropdown.
  //
  // Pure presentation. The parent (MessageInput) owns the
  // `highlighted` index and the keyboard handlers — keyboard events
  // fire on the textarea, not this dropdown, so the navigation state
  // has to live where the keys are captured. We render and report
  // click/hover; navigation is driven from above.

  interface Props {
    files: string[];
    query: string;
    highlighted: number;
    loading?: boolean;
    onSelect: (path: string) => void;
    onHighlight: (index: number) => void;
    onDismiss: () => void;
  }

  const {
    files,
    query,
    highlighted,
    loading = false,
    onSelect,
    onHighlight,
    onDismiss,
  }: Props = $props();
</script>

<svelte:window
  onclick={(e) => {
    const root = document.querySelector('[data-testid="mention-autocomplete"]');
    if (root && !root.contains(e.target as Node)) onDismiss();
  }}
/>

<div
  role="listbox"
  aria-label="File mention suggestions"
  data-testid="mention-autocomplete"
  class="absolute bottom-full left-0 right-0 mb-1 max-h-64 overflow-y-auto rounded border border-[var(--border)] bg-[var(--bg-sidebar)] shadow-lg text-sm"
>
  {#if loading}
    <div class="px-3 py-2 text-[var(--text-muted)]" data-testid="mention-loading" role="status">
      Loading files…
    </div>
  {:else if files.length === 0}
    <div class="px-3 py-2 text-[var(--text-muted)]" data-testid="mention-empty">
      No files match
      {#if query}<span class="text-[var(--text-secondary)]">"{query}"</span>{/if}
    </div>
  {:else}
    {#each files as path, i (path)}
      <button
        type="button"
        role="option"
        data-testid="mention-row"
        aria-selected={i === highlighted}
        onclick={() => onSelect(path)}
        onmouseenter={() => onHighlight(i)}
        class="block w-full text-left px-3 py-1.5 truncate {i === highlighted
          ? 'bg-[var(--bg-card)] text-[var(--text-primary)]'
          : 'text-[var(--text-secondary)] hover:bg-[var(--bg-card)] hover:text-[var(--text-primary)]'}"
      >
        {path}
      </button>
    {/each}
  {/if}
</div>
