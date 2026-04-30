<script lang="ts">
  import type { Message } from '$lib/types';
  import { formatToolUse } from '$lib/tools/format';

  interface Props {
    message: Message;
  }

  const { message }: Props = $props();

  // Compact tool_result preview: a chat bubble shouldn't swallow the screen
  // when a Bash command dumps 5 KB of output. Show a head + ellipsis hint.
  const RESULT_PREVIEW_CHARS = 600;

  const tool = $derived(message.tool_use ? formatToolUse(message.tool_use) : null);
  const resultPreview = $derived(
    message.tool_result && message.tool_result.content.length > RESULT_PREVIEW_CHARS
      ? `${message.tool_result.content.slice(0, RESULT_PREVIEW_CHARS)}\n…(${
          message.tool_result.content.length - RESULT_PREVIEW_CHARS
        } more chars)`
      : (message.tool_result?.content ?? '')
  );
</script>

{#if message.role === 'system'}
  <!-- System markers (e.g. compact_boundary) sit in the message stream as
       thin centered notices rather than chat bubbles. -->
  <div
    class="flex items-center justify-center py-1 text-xs text-[var(--text-muted)] italic"
    data-role="system"
    data-message-id={message.id}
  >
    <span class="px-2">{message.text}</span>
  </div>
{:else}
  <article
    class="flex flex-col gap-1 px-3 py-2 rounded text-sm break-words"
    class:bg-[var(--bg-card)]={message.role === 'user'}
    class:bg-[var(--bg-base)]={message.role !== 'user'}
    class:border={message.role !== 'user'}
    class:border-[var(--border-light)]={message.role !== 'user'}
    data-role={message.role}
    data-message-id={message.id}
  >
    {#if tool}
      <div
        class="flex items-center gap-2 text-xs font-mono text-[var(--text-secondary)]"
        data-tool-use
        data-tool-name={tool.label}
      >
        <span class="text-[var(--accent)]">{tool.icon}</span>
        <span class="font-semibold text-[var(--text-primary)]">{tool.label}</span>
        {#if tool.detail}
          <span class="text-[var(--text-secondary)] truncate" data-tool-detail>{tool.detail}</span>
        {/if}
      </div>
    {/if}

    {#if message.tool_result}
      <pre
        class="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap overflow-x-auto"
        data-tool-result
        class:text-[var(--error)]={message.tool_result.is_error}>{resultPreview}</pre>
    {/if}

    {#if message.text}
      <p class="whitespace-pre-wrap text-[var(--text-primary)]">{message.text}</p>
    {/if}

    {#if message.is_partial}
      <span class="text-xs text-[var(--text-muted)]" aria-label="streaming">▍</span>
    {/if}
  </article>
{/if}
