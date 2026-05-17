<!-- src/lib/components/kanban/FilterBar.svelte -->
<script lang="ts">
  import type { FilterSpec, BitableField } from '$lib/types';
  import { api } from '$lib/ipc';

  type Props = {
    repoId: string;
    appToken: string;
    tableId: string;
    filters: FilterSpec;
  };

  let { repoId, appToken, tableId, filters }: Props = $props();

  let fields = $state<BitableField[]>([]);
  let pickerOpen = $state(false);

  async function openPicker() {
    pickerOpen = true;
    if (fields.length === 0) {
      try {
        fields = await api.lark.listFields(appToken, tableId);
      } catch {
        fields = [];
      }
    }
  }
</script>

<div
  class="flex gap-2 items-center px-3 py-2 border-b border-[var(--border)]"
  data-repo-id={repoId}
>
  {#if filters.conditions.length >= 2}
    <select
      aria-label="conjunction"
      class="px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent)]"
    >
      <option value="and" selected={filters.conjunction === 'and'}>all</option>
      <option value="or" selected={filters.conjunction === 'or'}>any</option>
    </select>
  {/if}

  <button
    type="button"
    onclick={openPicker}
    class="px-2 py-1 text-xs rounded border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
  >
    + Add filter
  </button>

  {#if pickerOpen}
    <div
      role="dialog"
      aria-label="Pick column"
      class="absolute z-10 mt-1 flex flex-col rounded border border-[var(--border)] bg-[var(--bg-card)] shadow-lg"
    >
      {#each fields as field (field.field_id)}
        <button
          type="button"
          class="px-3 py-1.5 text-xs text-left hover:bg-[var(--bg-hover)] text-[var(--text-primary)]"
        >
          {field.field_name}
        </button>
      {/each}
    </div>
  {/if}
</div>
