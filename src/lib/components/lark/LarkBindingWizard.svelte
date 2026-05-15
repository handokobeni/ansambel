<!-- src/lib/components/lark/LarkBindingWizard.svelte -->
<script lang="ts">
  import { api } from '$lib/ipc';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type {
    BitableBinding,
    FieldMapping,
    ProposedMapping,
    StatusValueMapping,
    KanbanColumnLiteral,
  } from '$lib/types';

  const {
    repoId: _repoId,
    existing,
    onSave,
    onCancel,
  }: {
    repoId: string;
    existing: BitableBinding | null;
    onSave: (b: BitableBinding) => Promise<void>;
    onCancel: () => void;
  } = $props();

  type Step = 1 | 2 | 3;
  let step = $state<Step>(existing ? 2 : 1);
  let appToken = $state(existing?.app_token ?? '');
  let tableId = $state(existing?.table_id ?? '');
  let detecting = $state(false);
  let detectError = $state<string | null>(null);
  let proposal = $state<ProposedMapping | null>(null);

  let titleFieldId = $state<string>('');
  let descFieldId = $state<string>('');
  let statusFieldId = $state<string>('');
  let orderFieldId = $state<string>('');

  let valueMap = $state<Record<string, KanbanColumnLiteral>>({});
  let defaultColumn = $state<KanbanColumnLiteral>('todo');

  let saving = $state(false);

  async function handleDetect() {
    if (!appToken.trim() || !tableId.trim()) return;
    detecting = true;
    detectError = null;
    try {
      const p = await api.lark.detectSchema(appToken.trim(), tableId.trim());
      proposal = p;
      titleFieldId = p.suggested.title.field_id;
      descFieldId = p.suggested.description?.field_id ?? '';
      statusFieldId = p.suggested.status?.field_id ?? '';
      orderFieldId = p.suggested.order?.field_id ?? '';
      valueMap = { ...p.suggested_status_values.entries };
      defaultColumn = p.suggested_status_values.default_column;
      step = 2;
    } catch (err) {
      detectError = err instanceof Error ? err.message : String(err);
    } finally {
      detecting = false;
    }
  }

  const statusField = $derived(proposal?.fields.find((f) => f.field_id === statusFieldId) ?? null);
  const statusIsSingleSelect = $derived(statusField?.type === 3);

  function fieldRefOf(id: string) {
    if (!id || !proposal) return null;
    const f = proposal.fields.find((x) => x.field_id === id);
    return f ? { field_id: f.field_id, field_name: f.field_name } : null;
  }

  function handleContinueStep2() {
    if (!titleFieldId) return;
    if (statusIsSingleSelect && statusField?.property?.options) {
      step = 3;
    } else {
      handleSave();
    }
  }

  async function handleSave() {
    if (!proposal) return;
    saving = true;
    const titleRef = fieldRefOf(titleFieldId);
    if (!titleRef) {
      saving = false;
      return;
    }
    const binding: BitableBinding = {
      app_token: appToken.trim(),
      table_id: tableId.trim(),
      field_mapping: {
        title: titleRef,
        description: fieldRefOf(descFieldId),
        status: fieldRefOf(statusFieldId),
        order: fieldRefOf(orderFieldId),
      } satisfies FieldMapping,
      status_value_mapping: {
        entries: valueMap,
        default_column: defaultColumn,
      } satisfies StatusValueMapping,
      created_at: existing?.created_at ?? 0,
      updated_at: 0,
    };
    try {
      await onSave(binding);
    } catch (err) {
      addToast(`Save failed: ${err instanceof Error ? err.message : String(err)}`, 'error');
    } finally {
      saving = false;
    }
  }
</script>

<div class="lark-binding-wizard flex flex-col gap-3 p-3" data-testid="lark-binding-wizard">
  {#if step === 1}
    <section class="flex flex-col gap-3" data-testid="wizard-step-1">
      <h3 class="text-xs font-semibold">Connect to Lark Bitable (1 of 3)</h3>
      <label class="flex flex-col gap-1 text-[11px]">
        App Token
        <input
          type="text"
          bind:value={appToken}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent)] disabled:opacity-50"
          data-testid="wizard-app-token"
          disabled={detecting}
        />
      </label>
      <label class="flex flex-col gap-1 text-[11px]">
        Table ID
        <input
          type="text"
          bind:value={tableId}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent)] disabled:opacity-50"
          data-testid="wizard-table-id"
          disabled={detecting}
        />
      </label>
      <p class="text-[11px] text-[var(--text-muted)]">
        App ID & secret use global config in Settings.
      </p>
      {#if detectError}
        <div
          class="p-2 border border-[var(--accent-error,#f87171)] text-[var(--accent-error,#f87171)] text-[11px] rounded"
          data-testid="wizard-detect-error"
        >
          {detectError}
        </div>
      {/if}
      <div class="flex gap-2 justify-end">
        <button
          type="button"
          onclick={onCancel}
          disabled={detecting}
          class="px-2 py-1 text-xs rounded border border-[var(--border-light)] disabled:opacity-50"
          >Cancel</button
        >
        <button
          type="button"
          onclick={handleDetect}
          disabled={!appToken.trim() || !tableId.trim() || detecting}
          class="px-2 py-1 text-xs rounded bg-[var(--accent)] text-white disabled:opacity-50"
          data-testid="wizard-detect"
        >
          {detecting ? 'Detecting…' : 'Detect →'}
        </button>
      </div>
    </section>
  {:else if step === 2}
    <section class="flex flex-col gap-3" data-testid="wizard-step-2">
      <h3 class="text-xs font-semibold">Map your fields (2 of 3)</h3>
      <label class="flex flex-col gap-1 text-[11px]">
        Title* required
        <select
          bind:value={titleFieldId}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none"
          data-testid="wizard-title-field"
        >
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <label class="flex flex-col gap-1 text-[11px]">
        Description
        <select
          bind:value={descFieldId}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none"
          data-testid="wizard-desc-field"
        >
          <option value="">(none)</option>
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <label class="flex flex-col gap-1 text-[11px]">
        Status
        <select
          bind:value={statusFieldId}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none"
          data-testid="wizard-status-field"
        >
          <option value="">(none — default Todo)</option>
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <label class="flex flex-col gap-1 text-[11px]">
        Order
        <select
          bind:value={orderFieldId}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none"
          data-testid="wizard-order-field"
        >
          <option value="">(none — sort by created time)</option>
          {#each proposal?.fields ?? [] as f (f.field_id)}
            <option value={f.field_id}>{f.field_name}</option>
          {/each}
        </select>
      </label>
      <div class="flex gap-2 justify-end">
        <button
          type="button"
          onclick={() => (step = 1)}
          class="px-2 py-1 text-xs rounded border border-[var(--border-light)]">← Back</button
        >
        <button
          type="button"
          onclick={handleContinueStep2}
          disabled={!titleFieldId || saving}
          class="px-2 py-1 text-xs rounded bg-[var(--accent)] text-white disabled:opacity-50"
          data-testid="wizard-continue"
        >
          {statusIsSingleSelect ? 'Continue →' : 'Save & Sync'}
        </button>
      </div>
    </section>
  {:else}
    <section class="flex flex-col gap-3" data-testid="wizard-step-3">
      <h3 class="text-xs font-semibold">Map status options (3 of 3)</h3>
      {#each statusField?.property?.options ?? [] as opt (opt.id)}
        <label class="flex flex-col gap-1 text-[11px]">
          "{opt.name}"
          <select
            bind:value={valueMap[opt.id]}
            class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none"
            data-testid={`wizard-option-${opt.id}`}
          >
            <option value="todo">Todo</option>
            <option value="in_progress">In Progress</option>
            <option value="review">Review</option>
            <option value="done">Done</option>
          </select>
        </label>
      {/each}
      <label class="flex flex-col gap-1 text-[11px]">
        Default for unmapped values
        <select
          bind:value={defaultColumn}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] focus:outline-none"
          data-testid="wizard-default-column"
        >
          <option value="todo">Todo</option>
          <option value="in_progress">In Progress</option>
          <option value="review">Review</option>
          <option value="done">Done</option>
        </select>
      </label>
      <div class="flex gap-2 justify-end">
        <button
          type="button"
          onclick={() => (step = 2)}
          class="px-2 py-1 text-xs rounded border border-[var(--border-light)]">← Back</button
        >
        <button
          type="button"
          onclick={handleSave}
          disabled={saving}
          class="px-2 py-1 text-xs rounded bg-[var(--accent)] text-white disabled:opacity-50"
          data-testid="wizard-save"
        >
          {saving ? 'Saving…' : 'Save & Sync'}
        </button>
      </div>
    </section>
  {/if}
</div>
