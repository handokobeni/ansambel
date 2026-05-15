import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import LarkBindingWizard from './LarkBindingWizard.svelte';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

import { invoke } from '@tauri-apps/api/core';

const proposalFixture = {
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
    vi.mocked(invoke).mockRejectedValueOnce(new Error('91402 not found'));
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
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('wizard-detect-error').textContent).toContain('91402');
    expect(screen.getByTestId('wizard-step-1')).toBeTruthy();
  });

  it('moves to Step 2 on successful detect, pre-fills suggestions', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(proposalFixture);
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
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('wizard-step-2')).toBeTruthy();
    const titleSel = screen.getByTestId('wizard-title-field') as HTMLSelectElement;
    expect(titleSel.value).toBe('fld_pri');
  });

  it('Step 2 → Step 3 when status field is single-select', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(proposalFixture);
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
    await new Promise((r) => setTimeout(r, 0));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    expect(screen.getByTestId('wizard-step-3')).toBeTruthy();
  });

  it('Step 3 Save calls onSave with assembled binding', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(proposalFixture);
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
    await new Promise((r) => setTimeout(r, 0));
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await fireEvent.click(screen.getByTestId('wizard-save'));
    await new Promise((r) => setTimeout(r, 0));
    expect(onSave).toHaveBeenCalled();
    const call = onSave.mock.calls[0][0];
    expect(call.app_token).toBe('bascn');
    expect(call.field_mapping.title.field_id).toBe('fld_pri');
    expect(call.status_value_mapping.entries['opt_a']).toBe('todo');
  });
});
