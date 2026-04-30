<script lang="ts">
  interface Props {
    onSend: (text: string) => void;
    disabled?: boolean;
  }

  const { onSend, disabled = false }: Props = $props();

  let value = $state('');

  function handleSend() {
    const trimmed = value.trim();
    if (!trimmed) return;
    onSend(trimmed);
    value = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<form
  class="flex flex-col gap-2 p-3 border-t border-[var(--border)] bg-[var(--bg-base)]"
  onsubmit={(e) => {
    e.preventDefault();
    handleSend();
  }}
>
  <label class="sr-only" for="message-input">Message</label>
  <textarea
    id="message-input"
    class="w-full px-3 py-2 text-sm rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none min-h-[60px]"
    placeholder="Ask Claude…"
    bind:value
    onkeydown={handleKeydown}
    {disabled}
  ></textarea>
  <div class="flex justify-end">
    <button
      type="submit"
      class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 cursor-pointer"
      disabled={disabled || !value.trim()}
      aria-label="Send"
    >
      Send
    </button>
  </div>
</form>
