<!-- src/lib/components/SettingsDialog.svelte -->
<!--
  App-level Settings modal. Hosts the Lark credential panel today;
  future integrations (Jira credentials, Claude binary override, etc)
  drop in here as additional <section>s without changing the entry
  surface in TitleBar.

  Close UX matches NewTaskDialog: backdrop click, ESC key, or the
  explicit close button — whichever the user reaches for. Children
  components own their own state, so closing+reopening preserves form
  contents within the same mount; we re-key on open so the form
  re-runs `onMount` and refreshes status from the backend.
-->
<script lang="ts">
  import LarkSettings from './lark/LarkGlobalSettings.svelte';
  import { api } from '$lib/ipc';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type { TaskSource } from '$lib/types';

  const {
    open,
    onClose,
  }: {
    open: boolean;
    onClose: () => void;
  } = $props();

  let source = $state<TaskSource>('local');
  let saving = $state(false);

  $effect(() => {
    if (open) {
      api.task
        .getSource()
        .then((s) => (source = s))
        .catch(() => {});
    }
  });

  async function handleSourceChange(next: TaskSource) {
    if (next === source || saving) return;
    saving = true;
    const prev = source;
    source = next; // optimistic
    try {
      await api.task.setSource(next);
    } catch (e) {
      source = prev;
      addToast(`Cannot switch task source: ${e instanceof Error ? e.message : String(e)}`, 'error');
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={open ? handleKeydown : undefined} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="settings-backdrop fixed inset-0 z-50 flex items-start justify-center pt-16 bg-black/60 backdrop-blur-sm overflow-y-auto"
    onclick={onClose}
    data-testid="settings-backdrop"
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-dialog-title"
      tabindex="-1"
      class="settings-dialog relative w-[520px] max-w-[90vw] rounded-lg bg-[var(--bg-card)] border border-[var(--border-light)] shadow-2xl text-[var(--text-primary)] my-8"
      onclick={(e) => e.stopPropagation()}
    >
      <header
        class="flex items-center justify-between px-4 py-3 border-b border-[var(--border-light)]"
      >
        <h2 id="settings-dialog-title" class="text-sm font-semibold text-[var(--text-primary)]">
          Settings
        </h2>
        <button
          type="button"
          class="flex items-center justify-center w-7 h-7 rounded text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
          onclick={onClose}
          aria-label="Close settings"
          data-testid="settings-close"
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

      <section class="px-4 py-3 border-b border-[var(--border-light)]">
        <h3 class="text-sm font-semibold mb-1">Task source</h3>
        <p class="text-[11px] text-[var(--text-muted)] mb-2">
          Where Ansambel stores and syncs your kanban.
        </p>
        <label class="flex items-center gap-2 text-xs mb-1 cursor-pointer">
          <input
            type="radio"
            name="task-source"
            value="local"
            checked={source === 'local'}
            onchange={() => handleSourceChange('local')}
            disabled={saving}
            data-testid="task-source-local"
          />
          Local (tasks.json on this machine)
        </label>
        <label class="flex items-center gap-2 text-xs cursor-pointer">
          <input
            type="radio"
            name="task-source"
            value="lark"
            checked={source === 'lark'}
            onchange={() => handleSourceChange('lark')}
            disabled={saving}
            data-testid="task-source-lark"
          />
          Lark Bitable (shared with team)
        </label>
      </section>

      <div class="divide-y divide-[var(--border-light)]">
        {#key open}
          <LarkSettings />
        {/key}
      </div>
    </div>
  </div>
{/if}
