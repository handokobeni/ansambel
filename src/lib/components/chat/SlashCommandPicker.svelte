<script lang="ts">
  // Phase 3/slash-autocomplete — inline popover anchored to the chat textarea.
  // No overlay backdrop; the parent (ChatInput) decides when to open/close
  // based on the textarea trigger regex. Keyboard nav is owned by this
  // component via a document-level listener while `open`.
  import { onDestroy, onMount } from 'svelte';
  import { slashCommands } from '$lib/stores/slash-commands.svelte';
  import type { SlashCommand } from '$lib/types';

  interface Props {
    open: boolean;
    filterText: string;
    anchorRect: DOMRect;
    onSelect: (commandName: string) => void;
    onClose: () => void;
  }
  const { open, filterText, anchorRect, onSelect, onClose }: Props = $props();

  const rows = $derived<SlashCommand[]>(slashCommands.filtered(filterText));
  let highlightIndex = $state(0);

  // Clamp highlight whenever the filtered list changes (e.g. user typed more).
  $effect(() => {
    if (rows.length === 0) {
      highlightIndex = 0;
    } else if (highlightIndex >= rows.length) {
      highlightIndex = rows.length - 1;
    }
  });

  function sourceBadge(s: SlashCommand['source']): string {
    if (s.kind === 'plugin') return s.plugin;
    if (s.kind === 'user') return 'user';
    return 'built-in';
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (rows.length === 0) return;
      highlightIndex = (highlightIndex + 1) % rows.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (rows.length === 0) return;
      highlightIndex = (highlightIndex - 1 + rows.length) % rows.length;
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      if (rows.length === 0) return;
      e.preventDefault();
      onSelect(rows[highlightIndex].name);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKey, true);
  });
  onDestroy(() => {
    document.removeEventListener('keydown', handleKey, true);
  });
</script>

{#if open}
  <!-- Position above the textarea anchor; flip to below if not enough room.
       For the first cut we always render above the anchor (top-bias matches
       most chat UIs). -->
  <div
    class="slash-picker absolute z-50 bg-[var(--bg-panel)] border border-[var(--border)] rounded shadow-lg overflow-y-auto"
    style:bottom={`${window.innerHeight - anchorRect.top + 4}px`}
    style:left={`${anchorRect.left}px`}
    style:max-height="240px"
    style:min-width={`${Math.max(anchorRect.width, 320)}px`}
    role="listbox"
    aria-label="Slash command picker"
  >
    {#if rows.length === 0}
      <div class="px-3 py-2 text-xs text-[var(--text-muted)]" data-testid="slash-picker-empty">
        No slash commands match. Try clearing the filter.
      </div>
    {:else}
      <ul class="py-1">
        {#each rows as cmd, i (cmd.name)}
          <li>
            <button
              type="button"
              class="w-full text-left px-2 py-1 flex items-center gap-2 hover:bg-[var(--bg-hover)] {i ===
              highlightIndex
                ? 'bg-[var(--bg-hover)]'
                : ''}"
              data-testid="slash-picker-row"
              onmouseenter={() => (highlightIndex = i)}
              onclick={() => onSelect(cmd.name)}
            >
              <span class="font-mono text-xs">/{cmd.name}</span>
              <span class="text-[10px] uppercase tracking-wide text-[var(--text-muted)]"
                >{sourceBadge(cmd.source)}</span
              >
              <span class="text-xs text-[var(--text-muted)] truncate flex-1">{cmd.description}</span
              >
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}
