<script lang="ts">
  import { tick } from 'svelte';
  import { messages } from '$lib/stores/messages.svelte';
  import MessageBubble from './MessageBubble.svelte';
  import MessageInput from './MessageInput.svelte';
  import type { Message } from '$lib/types';

  interface Props {
    workspaceId: string;
    onSend: (text: string) => void;
    /** Optional loader for older messages. Receives the id of the oldest
     * message currently in view; should return the next page in
     * chronological order (oldest first). An empty result marks the
     * history exhausted. */
    onLoadEarlier?: (beforeId: string) => Promise<Message[]>;
    /** Distance from the top (in px) at which lazy load fires. Exposed
     * for tests; production code uses the default. */
    loadEarlierThreshold?: number;
  }

  const { workspaceId, onSend, onLoadEarlier, loadEarlierThreshold = 80 }: Props = $props();

  const list = $derived(messages.listForWorkspace(workspaceId));
  const status = $derived(messages.statusFor(workspaceId));
  const inputDisabled = $derived(status === 'error' || status === 'stopped');

  let loading = $state(false);
  let exhausted = $state(false);
  let scrollEl: HTMLDivElement | undefined;

  async function loadEarlier(): Promise<void> {
    if (!onLoadEarlier || loading || exhausted) return;
    if (list.length === 0) return;
    loading = true;
    const beforeId = list[0].id;
    const previousScrollHeight = scrollEl?.scrollHeight ?? 0;
    try {
      const batch = await onLoadEarlier(beforeId);
      if (batch.length === 0) {
        exhausted = true;
      } else {
        messages.hydrate(workspaceId, batch);
        // Preserve scroll position so the user stays anchored to the
        // message they were reading instead of jumping to the new top.
        await tick();
        if (scrollEl) {
          const delta = scrollEl.scrollHeight - previousScrollHeight;
          scrollEl.scrollTop = scrollEl.scrollTop + delta;
        }
      }
    } catch (err) {
      // Surface the failure but keep the loading flag clear so the user
      // can retry. The messages store already exposes `error` for the
      // workspace-level error pill rendered upstream.
      messages.apply({ type: 'error', message: String(err) }, workspaceId);
    } finally {
      loading = false;
    }
  }

  function handleScroll(): void {
    if (!scrollEl) return;
    if (scrollEl.scrollTop <= loadEarlierThreshold) {
      void loadEarlier();
    }
  }
</script>

<section class="flex flex-col h-full bg-[var(--bg-base)]">
  <div
    bind:this={scrollEl}
    onscroll={handleScroll}
    data-testid="chat-scroll"
    class="flex-1 overflow-y-auto px-3 py-3 flex flex-col gap-2"
  >
    {#if onLoadEarlier && list.length > 0}
      <div class="flex justify-center py-1 text-xs text-[var(--text-muted)]">
        {#if loading}
          <span data-testid="loading-earlier">Loading earlier…</span>
        {:else if exhausted}
          <span data-testid="history-exhausted">No more history.</span>
        {:else}
          <button
            type="button"
            class="hover:text-[var(--text-secondary)]"
            onclick={loadEarlier}
            data-testid="load-earlier-button"
          >
            Load earlier
          </button>
        {/if}
      </div>
    {/if}

    {#if list.length === 0}
      <div class="flex-1 flex items-center justify-center text-sm text-[var(--text-muted)]">
        Start the conversation — type a message below.
      </div>
    {:else}
      {#each list as msg (msg.id)}
        <MessageBubble message={msg} />
      {/each}
    {/if}
  </div>

  <MessageInput {onSend} disabled={inputDisabled} />
</section>
