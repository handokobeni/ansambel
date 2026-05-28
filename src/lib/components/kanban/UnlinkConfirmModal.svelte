<!-- src/lib/components/kanban/UnlinkConfirmModal.svelte -->
<script lang="ts">
  interface Props {
    open: boolean;
    workspaceTitle: string;
    onConfirm: () => void;
    onCancel: () => void;
  }
  const { open, workspaceTitle, onConfirm, onCancel }: Props = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    onclick={onCancel}
    role="presentation"
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Confirm unlink"
      tabindex="-1"
      class="unlink-confirm-modal relative w-[400px] max-w-[90vw] p-4 rounded-lg bg-[var(--bg-card)] border border-[var(--border-light)] shadow-2xl text-[var(--text-primary)]"
      onclick={(e) => e.stopPropagation()}
    >
      <p class="text-sm" data-testid="unlink-modal-text">
        This is the only card linked to <strong>«{workspaceTitle}»</strong>. The workspace will be
        removed because it is empty. Continue?
      </p>
      <div class="mt-3 flex justify-end gap-2">
        <button
          type="button"
          data-testid="unlink-modal-cancel"
          onclick={onCancel}
          class="px-2 py-1 text-xs rounded hover:bg-[var(--bg-hover)]">Cancel</button
        >
        <button
          type="button"
          data-testid="unlink-modal-confirm"
          onclick={onConfirm}
          class="px-2 py-1 text-xs bg-[var(--bg-hover)] rounded hover:bg-[var(--accent)] hover:text-white"
          >Continue</button
        >
      </div>
    </div>
  </div>
{/if}
