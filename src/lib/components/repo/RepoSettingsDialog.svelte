<!-- src/lib/components/repo/RepoSettingsDialog.svelte -->
<script lang="ts">
  import LarkBindingWizard from '$lib/components/lark/LarkBindingWizard.svelte';
  import { larkBindings } from '$lib/stores/lark-bindings.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type { BitableBinding } from '$lib/types';

  const {
    repoId,
    repoName,
    open,
    onClose,
  }: {
    repoId: string;
    repoName: string;
    open: boolean;
    onClose: () => void;
  } = $props();

  let editingBinding = $state(false);
  let confirmDisconnect = $state(false);

  const binding = $derived(larkBindings.get(repoId));
  const isConnected = $derived(!!binding);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (editingBinding) editingBinding = false;
      else if (confirmDisconnect) confirmDisconnect = false;
      else onClose();
    }
  }

  async function handleSaveBinding(b: BitableBinding) {
    await larkBindings.setBinding(repoId, b);
    editingBinding = false;
    addToast(`Bitable connected for ${repoName}`, 'success');
  }

  async function handleDisconnect() {
    try {
      await larkBindings.deleteBinding(repoId);
      confirmDisconnect = false;
    } catch {
      /* toast handled in store */
    }
  }
</script>

<svelte:window onkeydown={open ? handleKeydown : undefined} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/60 flex justify-center items-start pt-16 z-50"
    onclick={onClose}
    data-testid="repo-settings-backdrop"
  >
    <div
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      class="w-[560px] max-w-[90vw] bg-[var(--bg-card)] border border-[var(--border-light)] rounded-md"
      onclick={(e) => e.stopPropagation()}
    >
      <header
        class="flex justify-between items-center px-4 py-3 border-b border-[var(--border-light)]"
      >
        <h2 class="text-sm font-semibold text-[var(--text-primary)]">Settings — {repoName}</h2>
        <button
          onclick={onClose}
          aria-label="Close"
          class="flex items-center justify-center w-7 h-7 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
          data-testid="repo-settings-close"
        >
          <svg
            viewBox="0 0 24 24"
            width="14"
            height="14"
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
      </header>

      <section class="px-4 py-3" data-testid="lark-sync-section">
        <h3 class="text-xs font-semibold mb-2 text-[var(--text-primary)]">Lark Sync</h3>
        {#if editingBinding}
          <LarkBindingWizard
            {repoId}
            existing={binding ?? null}
            onSave={handleSaveBinding}
            onCancel={() => (editingBinding = false)}
          />
        {:else if isConnected && binding}
          <div class="flex flex-col gap-2 text-xs" data-testid="binding-summary">
            <div class="text-[var(--text-primary)]">✓ Connected: {binding.table_id}</div>
            <div class="text-[var(--text-muted)] text-[11px]">
              {binding.app_token.slice(0, 12)}… / {binding.table_id}
            </div>
            <div class="flex gap-2">
              <button
                type="button"
                onclick={() => (editingBinding = true)}
                class="px-2 py-1 rounded border border-[var(--border-light)] text-xs hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
                data-testid="edit-binding"
              >
                Edit mapping
              </button>
              <button
                type="button"
                onclick={() => (confirmDisconnect = true)}
                class="px-2 py-1 rounded border border-[var(--border-light)] text-xs hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
                data-testid="disconnect-binding"
              >
                Disconnect
              </button>
            </div>
          </div>
        {:else}
          <div class="flex flex-col gap-2 text-xs" data-testid="binding-empty">
            <p class="text-[var(--text-muted)]">Sync your kanban with a Lark Bitable.</p>
            <button
              type="button"
              onclick={() => (editingBinding = true)}
              class="px-2 py-1 rounded bg-[var(--accent)] text-white text-xs self-start disabled:opacity-50 cursor-pointer"
              data-testid="connect-binding"
            >
              Connect to Lark Bitable
            </button>
          </div>
        {/if}
      </section>
    </div>

    {#if confirmDisconnect}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="fixed inset-0 bg-black/70 flex justify-center items-center z-[60]"
        onclick={() => (confirmDisconnect = false)}
        data-testid="disconnect-confirm-backdrop"
      >
        <div
          role="dialog"
          aria-modal="true"
          tabindex="-1"
          class="bg-[var(--bg-card)] p-4 rounded-md w-[360px] max-w-[90vw] border border-[var(--border-light)]"
          onclick={(e) => e.stopPropagation()}
        >
          <p class="text-xs mb-3 text-[var(--text-primary)]">
            Disconnect from Lark? Tasks will revert to local tasks.json.
          </p>
          <div class="flex gap-2 justify-end">
            <button
              type="button"
              onclick={() => (confirmDisconnect = false)}
              class="px-2 py-1 rounded border border-[var(--border-light)] text-xs hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="button"
              onclick={handleDisconnect}
              class="px-2 py-1 rounded bg-[var(--accent-error,#f87171)] text-white text-xs hover:opacity-90 transition-opacity cursor-pointer"
              data-testid="disconnect-confirm"
            >
              Disconnect
            </button>
          </div>
        </div>
      </div>
    {/if}
  </div>
{/if}
