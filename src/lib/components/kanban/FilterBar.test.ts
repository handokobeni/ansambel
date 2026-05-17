import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import FilterBar from './FilterBar.svelte';

vi.mock('$lib/ipc', () => ({
  api: { lark: { listFields: vi.fn(async () => []) } },
}));

vi.mock('$lib/stores/lark-binding-filters.svelte', () => ({
  filterStore: { update: vi.fn(async () => {}) },
}));

describe('FilterBar — empty state', () => {
  it('renders + Add filter button when no conditions present', () => {
    render(FilterBar, {
      props: {
        repoId: 'repo-1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    expect(screen.getByRole('button', { name: /add filter/i })).toBeInTheDocument();
  });

  it('does not show conjunction toggle when 0 or 1 conditions', () => {
    const { unmount } = render(FilterBar, {
      props: {
        repoId: 'repo-1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    expect(screen.queryByRole('combobox', { name: /conjunction/i })).not.toBeInTheDocument();
    unmount();

    render(FilterBar, {
      props: {
        repoId: 'repo-1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [{ field_id: 'f1', field_name: 'Status', operator: 'is', value: [] }],
        },
      },
    });
    expect(screen.queryByRole('combobox', { name: /conjunction/i })).not.toBeInTheDocument();
  });
});

describe('FilterBar — add condition flow', () => {
  it('clicking column populates operator list for SingleSelect (type 3)', async () => {
    const { api } = await import('$lib/ipc');
    const listFieldsMock = vi.mocked(api.lark.listFields);
    listFieldsMock.mockResolvedValue([
      {
        field_id: 'fldStat',
        field_name: 'Status',
        type: 3,
        is_primary: false,
        property: {
          options: [
            { id: 'o1', name: 'Todo' },
            { id: 'o2', name: 'Done' },
          ],
        },
      },
    ]);

    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await tick();
    await fireEvent.click(await screen.findByText('Status'));
    await tick();
    const ops = await screen.findAllByRole('option');
    const labels = ops.map((o) => o.textContent?.trim()).filter(Boolean);
    expect(labels).toEqual(expect.arrayContaining(['is', 'isNot', 'isEmpty', 'isNotEmpty']));
    expect(labels).not.toEqual(expect.arrayContaining(['contains'])); // not a single-select op
  });

  it('Text (type 1) shows is/isNot/contains/doesNotContain/isEmpty/isNotEmpty', async () => {
    const { api } = await import('$lib/ipc');
    const listFieldsMock = vi.mocked(api.lark.listFields);
    listFieldsMock.mockResolvedValue([
      { field_id: 'fldT', field_name: 'Title', type: 1, is_primary: true, property: null },
    ]);
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await fireEvent.click(await screen.findByText('Title'));
    const ops = await screen.findAllByRole('option');
    const labels = ops.map((o) => o.textContent?.trim());
    expect(labels).toEqual(['is', 'isNot', 'contains', 'doesNotContain', 'isEmpty', 'isNotEmpty']);
  });
});

import { filterStore } from '$lib/stores/lark-binding-filters.svelte';

describe('FilterBar — addCondition and setValue', () => {
  it('selecting an operator calls filterStore.update with new condition', async () => {
    const { api } = await import('$lib/ipc');
    const listFieldsMock = vi.mocked(api.lark.listFields);
    const updateMock = vi.mocked(filterStore.update);
    updateMock.mockClear();
    listFieldsMock.mockResolvedValue([
      { field_id: 'fldS', field_name: 'Status', type: 3, is_primary: false, property: null },
    ]);
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await tick();
    await fireEvent.click(await screen.findByText('Status'));
    await tick();
    const ops = await screen.findAllByRole('option');
    await fireEvent.click(ops[0]); // picks first op ('is')
    await tick();
    expect(updateMock).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conditions: expect.arrayContaining([
          expect.objectContaining({ field_id: 'fldS', operator: 'is' }),
        ]),
      })
    );
  });

  it('selecting isEmpty (unary) adds condition with value []', async () => {
    const { api } = await import('$lib/ipc');
    const listFieldsMock = vi.mocked(api.lark.listFields);
    const updateMock = vi.mocked(filterStore.update);
    updateMock.mockClear();
    listFieldsMock.mockResolvedValue([
      { field_id: 'fldT', field_name: 'Title', type: 1, is_primary: true, property: null },
    ]);
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await tick();
    await fireEvent.click(await screen.findByText('Title'));
    await tick();
    const ops = await screen.findAllByRole('option');
    const emptyOp = ops.find((o) => o.textContent?.trim() === 'isEmpty')!;
    await fireEvent.click(emptyOp);
    await tick();
    expect(updateMock).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conditions: expect.arrayContaining([
          expect.objectContaining({ operator: 'isEmpty', value: [] }),
        ]),
      })
    );
  });

  it('editing a condition value input calls filterStore.update with new value', async () => {
    const updateMock = vi.mocked(filterStore.update);
    updateMock.mockClear();
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [{ field_id: 'fldT', field_name: 'Title', operator: 'is', value: ['old'] }],
        },
      },
    });
    const input = screen.getByRole('textbox') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'new value' } });
    expect(updateMock).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conditions: [expect.objectContaining({ value: ['new value'] })],
      })
    );
  });

  it('openPicker catches listFields errors and falls back to empty fields', async () => {
    const { api } = await import('$lib/ipc');
    const listFieldsMock = vi.mocked(api.lark.listFields);
    listFieldsMock.mockRejectedValueOnce(new Error('network error'));
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await tick();
    // picker is open but fields are empty — dialog has no field buttons
    const dialog = screen.getByRole('dialog', { name: /pick column/i });
    expect(dialog).toBeInTheDocument();
    expect(dialog.querySelectorAll('button')).toHaveLength(0);
  });

  it('clicking an unsupported field type does not pick the field', async () => {
    const { api } = await import('$lib/ipc');
    const listFieldsMock = vi.mocked(api.lark.listFields);
    // type 99 is not in OPS_BY_TYPE
    listFieldsMock.mockResolvedValue([
      { field_id: 'fldU', field_name: 'Unknown', type: 99, is_primary: false, property: null },
    ]);
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add filter/i }));
    await tick();
    const fieldBtn = await screen.findByText(/Unknown/);
    await fireEvent.click(fieldBtn);
    await tick();
    // operator list should NOT appear because field is unsupported
    expect(screen.queryByRole('listbox', { name: /pick operator/i })).not.toBeInTheDocument();
  });
});

describe('FilterBar — chip mutations', () => {
  it('clicking × removes that condition and calls filterStore.update', async () => {
    const updateMock = vi.mocked(filterStore.update);
    updateMock.mockClear();
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [{ field_id: 'fldS', field_name: 'Status', operator: 'is', value: ['Done'] }],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /remove condition/i }));
    expect(updateMock).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conditions: [],
      })
    );
  });

  it('changing AND/OR triggers update with new conjunction', async () => {
    const updateMock = vi.mocked(filterStore.update);
    updateMock.mockClear();
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [
            { field_id: 'a', field_name: 'A', operator: 'is', value: ['x'] },
            { field_id: 'b', field_name: 'B', operator: 'is', value: ['y'] },
          ],
        },
      },
    });
    const select = screen.getByRole('combobox', { name: /conjunction/i }) as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'or' } });
    expect(updateMock).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conjunction: 'or',
      })
    );
  });

  it('chip with unary operator (isEmpty) does not render a text input', () => {
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [{ field_id: 'fldT', field_name: 'Title', operator: 'isEmpty', value: [] }],
        },
      },
    });
    // unary ops show no value input
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    // chip is still rendered with field name and operator label
    expect(screen.getByText('Title')).toBeInTheDocument();
    expect(screen.getByText('isEmpty')).toBeInTheDocument();
  });

  it('setValue only updates the matching condition index, leaving others unchanged', async () => {
    const updateMock = vi.mocked(filterStore.update);
    updateMock.mockClear();
    render(FilterBar, {
      props: {
        repoId: 'r1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: {
          conjunction: 'and',
          conditions: [
            { field_id: 'fldA', field_name: 'A', operator: 'is', value: ['x'] },
            { field_id: 'fldB', field_name: 'B', operator: 'is', value: ['y'] },
          ],
        },
      },
    });
    const inputs = screen.getAllByRole('textbox') as HTMLInputElement[];
    // edit the second input (idx=1)
    await fireEvent.input(inputs[1], { target: { value: 'z' } });
    expect(updateMock).toHaveBeenCalledWith(
      'r1',
      expect.objectContaining({
        conditions: [
          expect.objectContaining({ field_id: 'fldA', value: ['x'] }),
          expect.objectContaining({ field_id: 'fldB', value: ['z'] }),
        ],
      })
    );
  });
});
