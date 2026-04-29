<script lang="ts">
  import { messages } from '$lib/stores/messages.svelte';
  import MessageBubble from './MessageBubble.svelte';
  import MessageInput from './MessageInput.svelte';

  interface Props {
    workspaceId: string;
    onSend: (text: string) => void;
  }

  const { workspaceId, onSend }: Props = $props();

  const list = $derived(messages.listForWorkspace(workspaceId));
  const status = $derived(messages.statusFor(workspaceId));
  const inputDisabled = $derived(status === 'error' || status === 'stopped');
</script>

<section class="flex flex-col h-full bg-[var(--bg-base)]">
  <div class="flex-1 overflow-y-auto px-3 py-3 flex flex-col gap-2">
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
