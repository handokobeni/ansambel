<!-- src/lib/components/kanban/NewTaskDialog.svelte -->
<script lang="ts">
  const {
    open,
    onSubmit,
    onCancel,
  }: {
    open: boolean;
    onSubmit: (data: { title: string; description: string }) => void;
    onCancel: () => void;
  } = $props();

  let title = $state('');
  let description = $state('');

  const canSubmit = $derived(title.trim().length > 0);

  function handleSubmit(e: Event) {
    e.preventDefault();
    if (!canSubmit) return;
    onSubmit({ title: title.trim(), description: description.trim() });
    title = '';
    description = '';
  }

  function handleCancel() {
    title = '';
    description = '';
    onCancel();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="dialog-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    onclick={handleCancel}
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="new-task-title"
      tabindex="-1"
      class="new-task-dialog relative w-[420px] max-w-[90vw] p-4 rounded-lg bg-[var(--bg-card)] border border-[var(--border-light)] shadow-2xl text-[var(--text-primary)]"
      onclick={(e) => e.stopPropagation()}
    >
      <h2 id="new-task-title" class="text-sm font-semibold mb-3 text-[var(--text-primary)]">
        New Task
      </h2>

      <form class="flex flex-col gap-3" onsubmit={handleSubmit}>
        <div class="flex flex-col gap-1">
          <label
            class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
            for="task-title"
          >
            Title
          </label>
          <input
            id="task-title"
            type="text"
            bind:value={title}
            placeholder="Task title"
            required
            class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label
            class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
            for="task-description"
          >
            Description
          </label>
          <textarea
            id="task-description"
            bind:value={description}
            placeholder="Optional description"
            rows={3}
            class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none"
          ></textarea>
        </div>

        <div class="flex justify-end gap-2 mt-1">
          <button
            type="button"
            class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
            onclick={handleCancel}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            disabled={!canSubmit}
          >
            Add task
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
