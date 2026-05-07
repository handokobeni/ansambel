<!-- src/lib/components/Toasts.svelte -->
<script lang="ts">
  import { getToasts, removeToast, type Toast } from '$lib/stores/toasts.svelte';

  const toasts = getToasts();
  // Newest first so freshly-added toasts appear at the top of the stack.
  const list = $derived([...toasts.values()].reverse());

  let copiedId = $state<string | null>(null);

  function colorClass(type: Toast['type']): string {
    if (type === 'error') return 'text-[var(--error)]';
    if (type === 'success') return 'text-[var(--status-ok)]';
    return 'text-[var(--text-primary)]';
  }

  async function copyMessage(toast: Toast): Promise<void> {
    try {
      await navigator.clipboard.writeText(toast.message);
      copiedId = toast.id;
      setTimeout(() => {
        if (copiedId === toast.id) copiedId = null;
      }, 1200);
    } catch {
      // Clipboard access can be denied (e.g. unfocused window). Silently
      // ignore — the toast itself already carries the message text.
    }
  }
</script>

{#if list.length > 0}
  <div
    class="toast-container fixed bottom-8 right-3 z-[9999] flex flex-col gap-2 max-w-[380px] pointer-events-none"
    role="status"
    aria-live="polite"
  >
    {#each list as toast (toast.id)}
      <div
        class="toast pointer-events-auto flex items-center gap-2.5 px-3.5 py-2.5 text-[0.8rem] leading-snug rounded-[10px] bg-[var(--toast-bg)] border border-[var(--toast-border)] shadow-[0_2px_12px_rgba(0,0,0,0.35)] backdrop-blur-xl backdrop-saturate-[1.8] break-words toast-{toast.type} {colorClass(
          toast.type
        )}"
      >
        <span class="flex-1 min-w-0">{toast.message}</span>
        <button
          type="button"
          class="shrink-0 inline-flex items-center justify-center bg-transparent border-none p-0 leading-none text-[var(--text-muted)] hover:text-[var(--text-secondary)] cursor-pointer"
          onclick={() => copyMessage(toast)}
          aria-label="Copy"
        >
          {#if copiedId === toast.id}
            <svg
              viewBox="0 0 24 24"
              width="13"
              height="13"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <polyline points="20 6 9 17 4 12" />
            </svg>
          {:else}
            <svg
              viewBox="0 0 24 24"
              width="13"
              height="13"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          {/if}
        </button>
        <button
          type="button"
          class="shrink-0 inline-flex items-center justify-center bg-transparent border-none p-0 leading-none text-[var(--text-muted)] hover:text-[var(--text-secondary)] cursor-pointer"
          onclick={() => removeToast(toast.id)}
          aria-label="Dismiss"
        >
          <svg
            viewBox="0 0 24 24"
            width="13"
            height="13"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    {/each}
  </div>
{/if}
