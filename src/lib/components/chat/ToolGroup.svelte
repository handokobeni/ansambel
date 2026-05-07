<!-- src/lib/components/chat/ToolGroup.svelte -->
<script lang="ts">
  import { untrack } from 'svelte';
  import MessageBubble from './MessageBubble.svelte';
  import type { Message } from '$lib/types';

  interface Props {
    messages: Message[];
    /** Whether the group starts expanded. Defaults to expanded so the
     *  user can see what the agent did at a glance — the chevron is the
     *  affordance to compress the run when reviewing later. */
    initialExpanded?: boolean;
  }

  const { messages, initialExpanded = true }: Props = $props();

  // Snapshot the initial value: subsequent changes to the prop should
  // not yank the user's collapse state. `untrack` makes that intent
  // explicit to the compiler so it doesn't warn about a stale closure.
  let expanded = $state(untrack(() => initialExpanded));

  function toggle(): void {
    expanded = !expanded;
  }
</script>

<div
  class="flex flex-col gap-1 rounded border border-[var(--border)] bg-[var(--bg-card)]/50 px-2 py-1"
  data-tool-group
  data-tool-group-count={messages.length}
>
  <button
    type="button"
    class="flex items-center justify-between gap-2 px-1 py-0.5 text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
    data-tool-group-toggle
    data-tool-group-header
    aria-expanded={expanded}
    onclick={toggle}
  >
    <span class="flex items-center gap-1.5">
      <svg
        viewBox="0 0 24 24"
        width="9"
        height="9"
        fill="none"
        stroke="currentColor"
        stroke-width="3"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
        class="transition-transform {expanded ? '' : '-rotate-90'}"
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>
      <span class="font-mono">{messages.length} tool calls</span>
    </span>
    <span class="text-[var(--text-muted)] text-[0.7rem] font-mono">
      {expanded ? 'collapse' : 'expand'}
    </span>
  </button>

  {#if expanded}
    <div class="flex flex-col gap-1 pt-1">
      {#each messages as message (message.id)}
        <MessageBubble {message} />
      {/each}
    </div>
  {/if}
</div>
