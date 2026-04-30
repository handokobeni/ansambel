<script lang="ts">
  interface Props {
    onSend: (text: string) => void;
    disabled?: boolean;
  }

  const { onSend, disabled = false }: Props = $props();

  let value = $state('');
  let textareaEl: HTMLTextAreaElement | undefined;

  // Cap auto-grow at ~12 lines so a runaway paste doesn't swallow the
  // chat history. Beyond this the textarea scrolls internally.
  const MAX_TEXTAREA_HEIGHT_PX = 240;

  function autoResize() {
    if (!textareaEl) return;
    // Reset to auto so shrinking works (scrollHeight only grows otherwise).
    textareaEl.style.height = 'auto';
    const next = Math.min(textareaEl.scrollHeight, MAX_TEXTAREA_HEIGHT_PX);
    textareaEl.style.height = `${next}px`;
    textareaEl.style.overflowY =
      textareaEl.scrollHeight > MAX_TEXTAREA_HEIGHT_PX ? 'auto' : 'hidden';
  }

  // Resize whenever the bound value changes — covers typing, paste, and
  // programmatic resets (after send).
  $effect(() => {
    void value;
    autoResize();
  });

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
    bind:this={textareaEl}
    id="message-input"
    rows="1"
    class="w-full px-3 py-2 text-sm rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none min-h-[40px] leading-relaxed"
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
