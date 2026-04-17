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
  <div class="dialog-backdrop" onclick={handleCancel}>
    <dialog class="new-task-dialog" open onclick={(e) => e.stopPropagation()}>
      <h2 class="new-task-dialog__title">New Task</h2>

      <form onsubmit={handleSubmit}>
        <div class="form-field">
          <label for="task-title">Title</label>
          <input id="task-title" type="text" bind:value={title} placeholder="Task title" required />
        </div>

        <div class="form-field">
          <label for="task-description">Description</label>
          <textarea
            id="task-description"
            bind:value={description}
            placeholder="Optional description"
            rows={3}
          ></textarea>
        </div>

        <div class="new-task-dialog__actions">
          <button type="button" class="btn-secondary" onclick={handleCancel}> Cancel </button>
          <button type="submit" class="btn-primary" disabled={!canSubmit}> Add task </button>
        </div>
      </form>
    </dialog>
  </div>
{/if}
