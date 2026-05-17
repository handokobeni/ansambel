import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import LarkBindingWizard from './LarkBindingWizard.svelte';
import type { BitableBinding, ProposedMapping } from '$lib/types';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      detectSchema: vi.fn(),
      listViews: vi.fn(),
    },
  },
}));

vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
}));

import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';

const proposalFixture: ProposedMapping = {
  fields: [
    { field_id: 'fld_pri', field_name: 'Task name', type: 1, is_primary: true },
    {
      field_id: 'fld_s',
      field_name: 'Task Status',
      type: 3,
      is_primary: false,
      property: {
        options: [
          { id: 'opt_a', name: 'To Do' },
          { id: 'opt_b', name: 'Done' },
        ],
      },
    },
  ],
  suggested: {
    title: { field_id: 'fld_pri', field_name: 'Task name' },
    description: null,
    status: { field_id: 'fld_s', field_name: 'Task Status' },
    order: null,
  },
  status_options: [
    { id: 'opt_a', name: 'To Do' },
    { id: 'opt_b', name: 'Done' },
  ],
  suggested_status_values: {
    entries: { opt_a: 'todo', opt_b: 'done' },
    default_column: 'todo',
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.lark.listViews).mockResolvedValue([]);
});

describe('LarkBindingWizard', () => {
  it('starts at Step 1 with Detect disabled when fields empty', () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    const btn = screen.getByTestId('wizard-detect') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it('Detect button enables when both fields populated', async () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    const btn = screen.getByTestId('wizard-detect') as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
  });

  it('Step 1 detect error stays on step and shows banner', async () => {
    vi.mocked(api.lark.detectSchema).mockRejectedValueOnce(new Error('91402 not found'));
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'x' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'y' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() =>
      expect(screen.getByTestId('wizard-detect-error').textContent).toContain('91402')
    );
    expect(screen.getByTestId('wizard-step-1')).toBeTruthy();
  });

  it('moves to Step 1.5 on successful detect, pre-fills suggestions', async () => {
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(proposalFixture);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeTruthy());
    // Advance through Step 1.5 to Step 2 and verify the title pre-fill is intact.
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    expect(screen.getByTestId('wizard-step-2')).toBeTruthy();
    const titleSel = screen.getByTestId('wizard-title-field') as HTMLSelectElement;
    expect(titleSel.value).toBe('fld_pri');
  });

  it('Step 2 → Step 3 when status field is single-select', async () => {
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(proposalFixture);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeTruthy());
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    expect(screen.getByTestId('wizard-step-3')).toBeTruthy();
  });

  it('Step 3 Save calls onSave with assembled binding', async () => {
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(proposalFixture);
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing: null, onSave, onCancel: vi.fn() },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeTruthy());
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await fireEvent.click(screen.getByTestId('wizard-save'));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const call = onSave.mock.calls[0][0];
    expect(call.app_token).toBe('bascn');
    expect(call.field_mapping.title.field_id).toBe('fld_pri');
    expect(call.status_value_mapping.entries['opt_a']).toBe('todo');
  });

  it('Cancel button fires onCancel', async () => {
    const onCancel = vi.fn();
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing: null, onSave: vi.fn(), onCancel },
    });
    await fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalled();
  });

  it('handleSave toasts on onSave error', async () => {
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(proposalFixture);
    const onSave = vi.fn().mockRejectedValue(new Error('save fail'));
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing: null, onSave, onCancel: vi.fn() },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeTruthy());
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    // At step 3 now — click Save
    await fireEvent.click(screen.getByTestId('wizard-save'));
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith(expect.stringContaining('save fail'), 'error')
    );
  });

  it('existing binding starts at Step 2 (pre-filled tokens, no proposal yet)', () => {
    const existing: BitableBinding = {
      app_token: 'bascn_existing',
      table_id: 'tbl_existing',
      view_id: null,
      field_mapping: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
      },
      status_value_mapping: { entries: {}, default_column: 'todo' },
      created_at: 0,
      updated_at: 0,
    };
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing, onSave: vi.fn(), onCancel: vi.fn() },
    });
    // existing present → starts at step 2
    expect(screen.getByTestId('wizard-step-2')).toBeTruthy();
  });

  it('handleSave early-returns when titleFieldId resolves to null (no proposal fields match)', async () => {
    // Set up a proposal with no fields matching the auto-selected titleFieldId
    const emptyFieldsProposal: ProposedMapping = {
      fields: [],
      suggested: {
        title: { field_id: 'fld_missing', field_name: 'Gone' },
        description: null,
        status: null,
        order: null,
      },
      status_options: [],
      suggested_status_values: { entries: {}, default_column: 'todo' },
    };
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(emptyFieldsProposal);
    const onSave = vi.fn();
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing: null, onSave, onCancel: vi.fn() },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), { target: { value: 'x' } });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), { target: { value: 'y' } });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeTruthy());
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    // Now at step 2, but no field options. titleFieldId='fld_missing' is set from suggested
    // but fields array is empty so fieldRefOf will return null → handleSave returns early
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await new Promise((r) => setTimeout(r, 0));
    // onSave should NOT have been called because of the early return
    expect(onSave).not.toHaveBeenCalled();
  });

  it('Step 2 continues directly to save when status type=3 but no options (property.options absent)', async () => {
    // handleContinueStep2 checks statusIsSingleSelect && statusField?.property?.options
    // If property.options is absent/falsy, it falls through to handleSave directly
    const noOptionsProposal: ProposedMapping = {
      fields: [
        { field_id: 'fld_pri', field_name: 'Task name', type: 1, is_primary: true },
        {
          field_id: 'fld_s',
          field_name: 'Status',
          type: 3,
          is_primary: false,
          property: {}, // no options
        },
      ],
      suggested: {
        title: { field_id: 'fld_pri', field_name: 'Task name' },
        description: null,
        status: { field_id: 'fld_s', field_name: 'Status' },
        order: null,
      },
      status_options: [],
      suggested_status_values: { entries: {}, default_column: 'todo' },
    };
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(noOptionsProposal);
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(LarkBindingWizard, {
      props: { repoId: 'repo_x', existing: null, onSave, onCancel: vi.fn() },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), { target: { value: 'x' } });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), { target: { value: 'y' } });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeTruthy());
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    // type=3 (single-select) but no options → falls through to handleSave directly
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
  });
});

describe('LarkBindingWizard step 1.5 (view picker)', () => {
  beforeEach(() => {
    const fixture: ProposedMapping = {
      fields: [
        {
          field_id: 'fld_t',
          field_name: 'Task name',
          type: 1,
          is_primary: true,
        },
      ],
      suggested: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
      },
      status_options: null,
      suggested_status_values: { entries: {}, default_column: 'todo' },
    };
    vi.mocked(api.lark.detectSchema).mockResolvedValue(fixture);
    vi.mocked(api.lark.listViews).mockResolvedValue([
      { view_id: 'vw_grid', view_name: 'Grid view', view_type: 'grid' },
      { view_id: 'vw_sprint', view_name: 'Current Sprint', view_type: 'grid' },
    ]);
  });

  it('renders dropdown with "All records (no view filter)" first', async () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeInTheDocument());
    const select = screen.getByTestId('wizard-view-select') as HTMLSelectElement;
    expect(select.options[0].value).toBe('');
    expect(select.options[0].textContent).toContain('All records');
    expect(select.options[1].value).toBe('vw_grid');
    expect(select.options[2].value).toBe('vw_sprint');
    expect(select.value).toBe('');
  });

  it('selecting a view stores its id and Continue advances to step 2', async () => {
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    const select = await screen.findByTestId('wizard-view-select');
    await fireEvent.change(select, { target: { value: 'vw_sprint' } });
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    expect(screen.getByTestId('wizard-step-2')).toBeInTheDocument();
  });

  it('proceeds even when view list is empty', async () => {
    vi.mocked(api.lark.listViews).mockResolvedValue([]);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    const select = await screen.findByTestId('wizard-view-select');
    expect((select as HTMLSelectElement).options.length).toBe(1);
    await fireEvent.click(screen.getByTestId('wizard-view-continue'));
    expect(screen.getByTestId('wizard-step-2')).toBeInTheDocument();
  });

  it('proceeds to step 1.5 even when listViews fails', async () => {
    vi.mocked(api.lark.listViews).mockRejectedValue(new Error('views API forbidden'));
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: null,
        onSave: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascntest' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbltest' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await waitFor(() => expect(screen.getByTestId('wizard-step-1-5')).toBeInTheDocument());
    const select = screen.getByTestId('wizard-view-select') as HTMLSelectElement;
    // Only the "All records" fallback option, no view rows.
    expect(select.options.length).toBe(1);
    expect(select.options[0].textContent).toContain('All records');
  });

  it('pre-selects view_id when editing existing binding and preserves it through Save', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(LarkBindingWizard, {
      props: {
        repoId: 'repo_x',
        existing: {
          app_token: 'bascntest',
          table_id: 'tbltest',
          view_id: 'vw_sprint',
          field_mapping: {
            title: { field_id: 'fld_t', field_name: 'Task name' },
            description: null,
            status: null,
            order: null,
          },
          status_value_mapping: { entries: {}, default_column: 'todo' },
          created_at: 1747200000,
          updated_at: 1747200000,
        },
        onSave,
        onCancel: vi.fn(),
      },
    });
    // Editing flow opens at step 2. Save immediately and verify view_id
    // round-trips through the saved binding.
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.view_id).toBe('vw_sprint');
  });
});
