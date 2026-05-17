<!-- src/lib/components/kanban/FilterBar.svelte -->
<script lang="ts">
  import type { FilterSpec, FilterCondition, FilterOperator, BitableField } from '$lib/types';
  import { api } from '$lib/ipc';
  import { filterStore } from '$lib/stores/lark-binding-filters.svelte';

  type Props = {
    repoId: string;
    appToken: string;
    tableId: string;
    filters: FilterSpec;
  };
  let { repoId, appToken, tableId, filters }: Props = $props();

  let fields = $state<BitableField[]>([]);
  let pickerOpen = $state(false);
  let pickedField = $state<BitableField | null>(null);

  const OPS_BY_TYPE: Record<number, FilterOperator[]> = {
    1: ['is', 'isNot', 'contains', 'doesNotContain', 'isEmpty', 'isNotEmpty'],
    2: [
      'is',
      'isNot',
      'isGreater',
      'isGreaterEqual',
      'isLess',
      'isLessEqual',
      'isEmpty',
      'isNotEmpty',
    ],
    3: ['is', 'isNot', 'isEmpty', 'isNotEmpty'],
    4: ['contains', 'doesNotContain', 'isEmpty', 'isNotEmpty'],
    5: [
      'is',
      'isNot',
      'isGreater',
      'isGreaterEqual',
      'isLess',
      'isLessEqual',
      'isEmpty',
      'isNotEmpty',
    ],
    11: ['is', 'isNot', 'contains', 'doesNotContain', 'isEmpty', 'isNotEmpty'],
  };

  const UNARY: ReadonlyArray<FilterOperator> = ['isEmpty', 'isNotEmpty'];

  function isSupported(t: number): boolean {
    return t in OPS_BY_TYPE;
  }

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

  function pickField(f: BitableField) {
    if (!isSupported(f.type)) return;
    pickedField = f;
  }

  async function addCondition(op: FilterOperator) {
    /* v8 ignore next -- pickedField is always set before the operator listbox renders */
    if (!pickedField) return;
    const cond: FilterCondition = {
      field_id: pickedField.field_id,
      field_name: pickedField.field_name,
      operator: op,
      value: UNARY.includes(op) ? [] : [''],
    };
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: [...filters.conditions, cond],
    };
    pickerOpen = false;
    pickedField = null;
    await filterStore.update(repoId, next);
  }

  async function removeCondition(idx: number) {
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.filter((_, i) => i !== idx),
    };
    await filterStore.update(repoId, next);
  }

  async function setConjunction(c: 'and' | 'or') {
    await filterStore.update(repoId, { ...filters, conjunction: c });
  }

  async function setValue(idx: number, value: string) {
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.map((c, i) => (i === idx ? { ...c, value: [value] } : c)),
    };
    await filterStore.update(repoId, next);
  }
</script>

<div
  class="flex flex-wrap gap-2 items-center px-3 py-2 border-b border-[var(--border)]"
  data-repo-id={repoId}
>
  {#if filters.conditions.length >= 2}
    <select
      aria-label="conjunction"
      value={filters.conjunction}
      onchange={(e) => setConjunction(e.currentTarget.value as 'and' | 'or')}
      class="px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent)]"
    >
      <option value="and">all</option>
      <option value="or">any</option>
    </select>
    <span class="text-xs text-[var(--text-dim)]">of the conditions</span>
  {/if}

  {#each filters.conditions as cond, idx (cond.field_id + idx)}
    <span
      class="inline-flex items-center gap-1 px-2 py-1 text-xs rounded-full bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-primary)]"
    >
      <span class="font-medium">{cond.field_name}</span>
      <span class="text-[var(--text-dim)]">{cond.operator}</span>
      {#if !UNARY.includes(cond.operator)}
        <input
          type="text"
          value={cond.value[0] ?? ''}
          oninput={(e) => setValue(idx, e.currentTarget.value)}
          class="w-20 px-1 py-0 text-xs bg-transparent border-b border-[var(--border-light)] focus:outline-none focus:border-[var(--accent)] text-[var(--text-primary)]"
        />
      {/if}
      <button
        type="button"
        aria-label="remove condition"
        onclick={() => removeCondition(idx)}
        class="ml-0.5 text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
        >×</button
      >
    </span>
  {/each}

  <button
    type="button"
    onclick={openPicker}
    class="px-2 py-1 text-xs rounded border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
  >
    + Add filter
  </button>

  {#if pickerOpen && !pickedField}
    <div
      role="dialog"
      aria-label="Pick column"
      class="absolute z-10 mt-1 flex flex-col rounded border border-[var(--border)] bg-[var(--bg-card)] shadow-lg"
    >
      {#each fields as field (field.field_id)}
        <button
          type="button"
          disabled={!isSupported(field.type)}
          onclick={() => pickField(field)}
          class="px-3 py-1.5 text-xs text-left hover:bg-[var(--bg-hover)] text-[var(--text-primary)] disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {field.field_name}{#if !isSupported(field.type)}
            (not supported){/if}
        </button>
      {/each}
    </div>
  {/if}

  {#if pickedField}
    <div
      role="listbox"
      aria-label="Pick operator"
      class="absolute z-10 mt-1 flex flex-col rounded border border-[var(--border)] bg-[var(--bg-card)] shadow-lg"
    >
      {#each OPS_BY_TYPE[pickedField.type] as op (op)}
        <button
          type="button"
          role="option"
          aria-selected="false"
          onclick={() => addCondition(op)}
          class="px-3 py-1.5 text-xs text-left hover:bg-[var(--bg-hover)] text-[var(--text-primary)]"
          >{op}</button
        >
      {/each}
    </div>
  {/if}
</div>
