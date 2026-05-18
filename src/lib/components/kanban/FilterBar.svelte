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

  // Inline type for select/multiselect option property
  type FieldProperty = { options?: { id: string; name: string }[] };

  let fields = $state<BitableField[]>([]);
  let fieldsLoaded = $state(false);
  let open = $state(false);

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

  const SUPPORTED_TYPES = new Set(Object.keys(OPS_BY_TYPE).map(Number));
  const UNARY: ReadonlyArray<FilterOperator> = ['isEmpty', 'isNotEmpty'];

  function isSupported(t: number): boolean {
    return SUPPORTED_TYPES.has(t);
  }

  function supportedFields(): BitableField[] {
    return fields.filter((f) => isSupported(f.type));
  }

  function fieldById(fieldId: string): BitableField | undefined {
    return fields.find((f) => f.field_id === fieldId);
  }

  function fieldTypeForCondition(cond: FilterCondition): number | undefined {
    return fieldById(cond.field_id)?.type;
  }

  function getOptions(field: BitableField): { id: string; name: string }[] {
    return (field.property as FieldProperty | null)?.options ?? [];
  }

  async function openPopover() {
    open = true;
    if (!fieldsLoaded) {
      try {
        fields = await api.lark.listFields(appToken, tableId);
      } catch {
        fields = [];
      }
      fieldsLoaded = true;
    }
  }

  function closePopover() {
    open = false;
  }

  async function setConjunction(c: 'and' | 'or') {
    await filterStore.update(repoId, { ...filters, conjunction: c });
  }

  async function addCondition() {
    const supported = supportedFields();
    const firstField = supported[0];
    if (!firstField) {
      // No supported fields yet; add a placeholder with empty id
      const next: FilterSpec = {
        conjunction: filters.conjunction,
        conditions: [
          ...filters.conditions,
          { field_id: '', field_name: '', operator: 'is', value: [''] },
        ],
      };
      await filterStore.update(repoId, next);
      return;
    }
    const ops = OPS_BY_TYPE[firstField.type];
    const op = ops[0];
    const value = UNARY.includes(op) ? [] : [''];
    const cond: FilterCondition = {
      field_id: firstField.field_id,
      field_name: firstField.field_name,
      operator: op,
      value,
    };
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: [...filters.conditions, cond],
    };
    await filterStore.update(repoId, next);
  }

  async function removeCondition(idx: number) {
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.filter((_, i) => i !== idx),
    };
    await filterStore.update(repoId, next);
  }

  async function changeField(idx: number, fieldId: string) {
    const field = fieldById(fieldId);
    if (!field) return;
    const ops = OPS_BY_TYPE[field.type];
    const op = ops[0];
    const value = UNARY.includes(op) ? [] : [''];
    const updated: FilterCondition = {
      field_id: field.field_id,
      field_name: field.field_name,
      operator: op,
      value,
    };
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.map((c, i) => (i === idx ? updated : c)),
    };
    await filterStore.update(repoId, next);
  }

  async function changeOperator(idx: number, op: FilterOperator) {
    const cond = filters.conditions[idx];
    const newValue = UNARY.includes(op) ? [] : cond.value.length > 0 ? cond.value : [''];
    const updated: FilterCondition = { ...cond, operator: op, value: newValue };
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.map((c, i) => (i === idx ? updated : c)),
    };
    await filterStore.update(repoId, next);
  }

  async function changeValue(idx: number, value: string) {
    const next: FilterSpec = {
      conjunction: filters.conjunction,
      conditions: filters.conditions.map((c, i) => (i === idx ? { ...c, value: [value] } : c)),
    };
    await filterStore.update(repoId, next);
  }

  // Close on Escape key
  $effect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        closePopover();
      }
    }
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
    };
  });

  const conditionCount = $derived(filters.conditions.length);
  const triggerLabel = $derived(conditionCount > 0 ? `Filter (${conditionCount})` : 'Filter');
  const isActive = $derived(conditionCount > 0);
</script>

<div class="relative" data-repo-id={repoId}>
  <!-- Trigger button -->
  <button
    type="button"
    onclick={openPopover}
    class="px-3 py-1.5 text-xs rounded border transition-colors cursor-pointer
      {isActive
      ? 'border-[var(--accent)] bg-[var(--bg-active)] text-[var(--text-primary)] font-medium'
      : 'border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]'}"
  >
    {triggerLabel}
  </button>

  {#if open}
    <!-- Backdrop: transparent overlay that captures outside clicks -->
    <div
      class="fixed inset-0 z-40 bg-black/0"
      data-testid="filter-backdrop"
      onclick={closePopover}
      onkeydown={() => {}}
      role="presentation"
    ></div>

    <!-- Popover panel -->
    <div
      role="dialog"
      aria-label="Filter records"
      class="absolute left-0 top-full mt-1 z-50 w-[640px] max-h-96 overflow-y-auto rounded-lg border border-[var(--border)] bg-[var(--bg-card)] shadow-xl"
      onwheel={(e) => e.stopPropagation()}
    >
      <!-- Header row -->
      <div
        class="flex items-center justify-between px-4 py-3 border-b border-[var(--border)] text-sm"
      >
        <span class="font-medium text-[var(--text-primary)]">Filter records</span>
        <div class="flex items-center gap-1.5 text-[var(--text-secondary)]">
          <span>Meeting</span>
          <select
            aria-label="conjunction"
            value={filters.conjunction}
            onchange={(e) => setConjunction(e.currentTarget.value as 'and' | 'or')}
            class="px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] cursor-pointer"
          >
            <option value="and">all</option>
            <option value="or">any</option>
          </select>
          <span>of the conditions</span>
        </div>
      </div>

      <!-- Conditions list -->
      <div class="flex flex-col divide-y divide-[var(--border)]">
        {#each filters.conditions as cond, idx (cond.field_id + idx)}
          {@const fieldType = fieldTypeForCondition(cond)}
          {@const opsForRow =
            fieldType !== undefined ? (OPS_BY_TYPE[fieldType] ?? ['is', 'isNot']) : ['is', 'isNot']}
          {@const isUnary = UNARY.includes(cond.operator)}

          <div class="flex items-center gap-2 px-4 py-2.5">
            <!-- Field selector -->
            <select
              aria-label="field"
              value={cond.field_id}
              onchange={(e) => changeField(idx, e.currentTarget.value)}
              class="flex-1 min-w-0 px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] cursor-pointer"
            >
              {#if fields.length === 0}
                <!-- No fields loaded yet — show current name as placeholder -->
                <option value={cond.field_id}>{cond.field_name || cond.field_id}</option>
              {:else}
                {#each supportedFields() as field (field.field_id)}
                  <option value={field.field_id}>{field.field_name}</option>
                {/each}
              {/if}
            </select>

            <!-- Operator selector -->
            <select
              aria-label="operator"
              value={cond.operator}
              onchange={(e) => changeOperator(idx, e.currentTarget.value as FilterOperator)}
              class="flex-1 min-w-0 px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] cursor-pointer"
            >
              {#each opsForRow as op (op)}
                <option value={op}>{op}</option>
              {/each}
            </select>

            <!-- Value picker (hidden for unary ops) -->
            {#if !isUnary}
              {#if fieldType === 3 || fieldType === 4}
                <!-- SingleSelect / MultiSelect → <select> with options from field.property -->
                {@const field = fieldById(cond.field_id)}
                {@const opts = field ? getOptions(field) : []}
                <select
                  aria-label="value"
                  value={cond.value[0] ?? ''}
                  onchange={(e) => changeValue(idx, e.currentTarget.value)}
                  class="flex-1 min-w-0 px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)] cursor-pointer"
                >
                  {#each opts as opt (opt.id)}
                    <option value={opt.name}>{opt.name}</option>
                  {/each}
                </select>
              {:else if fieldType === 5}
                <!-- DateTime → date input -->
                <input
                  type="date"
                  aria-label="value"
                  value={cond.value[0] ?? ''}
                  onchange={(e) => changeValue(idx, e.currentTarget.value)}
                  class="flex-1 min-w-0 min-w-[10rem] px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              {:else if fieldType === 2}
                <!-- Number -->
                <input
                  type="number"
                  aria-label="value"
                  value={cond.value[0] ?? ''}
                  onchange={(e) => changeValue(idx, e.currentTarget.value)}
                  class="flex-1 min-w-0 min-w-[10rem] px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              {:else}
                <!-- Text (type 1) or Person (type 11) or unknown -->
                <input
                  type="text"
                  aria-label="value"
                  value={cond.value[0] ?? ''}
                  onchange={(e) => changeValue(idx, e.currentTarget.value)}
                  class="flex-1 min-w-0 min-w-[10rem] px-2 py-1 text-xs rounded-md border border-[var(--border-light)] bg-[var(--bg-hover)] text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              {/if}
            {:else}
              <!-- Unary: empty spacer to keep layout consistent -->
              <div class="flex-1 min-w-0"></div>
            {/if}

            <!-- Remove button -->
            <button
              type="button"
              aria-label="remove condition"
              onclick={() => removeCondition(idx)}
              class="flex-none text-[var(--text-muted)] hover:text-red-500 transition-colors cursor-pointer text-base leading-none px-1"
            >
              ×
            </button>
          </div>
        {/each}
      </div>

      <!-- Footer -->
      <div class="px-4 py-2.5 border-t border-[var(--border)]">
        <button
          type="button"
          onclick={addCondition}
          class="text-xs text-[var(--accent)] hover:underline cursor-pointer font-medium"
        >
          + Add Condition
        </button>
      </div>
    </div>
  {/if}
</div>
