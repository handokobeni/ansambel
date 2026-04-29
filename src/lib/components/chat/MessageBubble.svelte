<script lang="ts">
  import type { Message } from '$lib/types';

  interface Props {
    message: Message;
  }

  const { message }: Props = $props();
</script>

<article
  class="flex flex-col gap-1 px-3 py-2 rounded text-sm break-words"
  class:bg-[var(--bg-card)]={message.role === 'user'}
  class:bg-[var(--bg-base)]={message.role !== 'user'}
  class:border={message.role !== 'user'}
  class:border-[var(--border-light)]={message.role !== 'user'}
  data-role={message.role}
  data-message-id={message.id}
>
  {#if message.tool_use}
    <div
      class="flex items-center gap-2 text-xs font-mono text-[var(--text-secondary)]"
      data-tool-use
    >
      <span class="text-[var(--accent)]">⚙</span>
      <span class="font-semibold text-[var(--text-primary)]">{message.tool_use.name}</span>
    </div>
  {/if}

  {#if message.tool_result}
    <pre
      class="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap overflow-x-auto"
      data-tool-result
      class:text-[var(--error)]={message.tool_result.is_error}>{message.tool_result.content}</pre>
  {/if}

  {#if message.text}
    <p class="whitespace-pre-wrap text-[var(--text-primary)]">{message.text}</p>
  {/if}

  {#if message.is_partial}
    <span class="text-xs text-[var(--text-muted)]" aria-label="streaming">▍</span>
  {/if}
</article>
