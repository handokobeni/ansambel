import { describe, expect, it, vi, beforeEach } from 'vitest';
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

import { filterStore } from '$lib/stores/lark-binding-filters.svelte';
import { api } from '$lib/ipc';

const defaultProps = {
  repoId: 'repo-1',
  appToken: 'appA',
  tableId: 'tblA',
  filters: { conjunction: 'and' as const, conditions: [] },
};

const twoConditionProps = {
  ...defaultProps,
  filters: {
    conjunction: 'and' as const,
    conditions: [
      { field_id: 'fldA', field_name: 'Status', operator: 'is' as const, value: ['Todo'] },
      { field_id: 'fldB', field_name: 'Assignee', operator: 'is' as const, value: ['Beni'] },
    ],
  },
};

beforeEach(() => {
  vi.mocked(filterStore.update).mockClear();
  vi.mocked(api.lark.listFields).mockClear();
  vi.mocked(api.lark.listFields).mockResolvedValue([]);
});

// ─── 1. Trigger button label ───────────────────────────────────────────────

describe('FilterBar trigger button', () => {
  it('shows "Filter" label when no conditions are present', () => {
    render(FilterBar, { props: defaultProps });
    expect(screen.getByRole('button', { name: /^filter$/i })).toBeInTheDocument();
  });

  it('shows "Filter (N)" label when N conditions are present', () => {
    render(FilterBar, { props: twoConditionProps });
    expect(screen.getByRole('button', { name: /filter \(2\)/i })).toBeInTheDocument();
  });

  it('popover is not visible on initial render', () => {
    render(FilterBar, { props: defaultProps });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

// ─── 2. Popover open/close ─────────────────────────────────────────────────

describe('FilterBar popover toggle', () => {
  it('clicking trigger opens popover (role=dialog present)', async () => {
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('clicking backdrop closes popover', async () => {
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    const backdrop = document.querySelector('[data-testid="filter-backdrop"]') as HTMLElement;
    expect(backdrop).toBeTruthy();
    await fireEvent.click(backdrop);
    await tick();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('pressing Escape closes the popover', async () => {
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    await fireEvent.keyDown(document, { key: 'Escape' });
    await tick();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

// ─── 3. Conjunction header ─────────────────────────────────────────────────

describe('FilterBar conjunction header', () => {
  it('conjunction <select> is always present in the popover header (0 conditions)', async () => {
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    expect(screen.getByRole('combobox', { name: /conjunction/i })).toBeInTheDocument();
  });

  it('conjunction <select> is present with 1 condition', async () => {
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            { field_id: 'f1', field_name: 'Status', operator: 'is' as const, value: ['x'] },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    expect(screen.getByRole('combobox', { name: /conjunction/i })).toBeInTheDocument();
  });

  it('changing conjunction triggers filterStore.update with new conjunction', async () => {
    render(FilterBar, { props: twoConditionProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    const select = screen.getByRole('combobox', { name: /conjunction/i }) as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'or' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({ conjunction: 'or' })
    );
  });
});

// ─── 4. Add condition ──────────────────────────────────────────────────────

describe('FilterBar add condition', () => {
  it('clicking "+ Add Condition" appends a row and calls filterStore.update', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fldS',
        field_name: 'Status',
        type: 3,
        is_primary: false,
        property: { options: [{ id: 'o1', name: 'Todo' }] },
      },
    ]);
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await fireEvent.click(screen.getByRole('button', { name: /add condition/i }));
    await tick();
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: expect.arrayContaining([expect.objectContaining({ field_id: 'fldS' })]),
      })
    );
  });
});

// ─── 5. Change condition value ─────────────────────────────────────────────

describe('FilterBar condition value mutations', () => {
  it('changing a text value triggers filterStore.update with new value', async () => {
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            { field_id: 'fldT', field_name: 'Title', operator: 'is' as const, value: ['old'] },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    const input = screen.getByRole('textbox') as HTMLInputElement;
    await fireEvent.change(input, { target: { value: 'new value' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [expect.objectContaining({ value: ['new value'] })],
      })
    );
  });

  it('changing a condition field resets operator and value', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fldT',
        field_name: 'Title',
        type: 1,
        is_primary: true,
        property: null,
      },
      {
        field_id: 'fldS',
        field_name: 'Status',
        type: 3,
        is_primary: false,
        property: { options: [{ id: 'o1', name: 'Todo' }] },
      },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldT',
              field_name: 'Title',
              operator: 'contains' as const,
              value: ['hello'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    // Wait for fields to load
    await tick();
    // Change field from Title (type 1) to Status (type 3)
    const fieldSelect = screen
      .getAllByRole('combobox')
      .find((s) => (s as HTMLSelectElement).value === 'fldT') as HTMLSelectElement;
    expect(fieldSelect).toBeTruthy();
    await fireEvent.change(fieldSelect, { target: { value: 'fldS' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [expect.objectContaining({ field_id: 'fldS', operator: 'is', value: [''] })],
      })
    );
  });
});

// ─── 6. Remove condition ───────────────────────────────────────────────────

describe('FilterBar remove condition', () => {
  it('clicking × triggers filterStore.update without that condition', async () => {
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            { field_id: 'fldS', field_name: 'Status', operator: 'is' as const, value: ['Done'] },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await fireEvent.click(screen.getByRole('button', { name: /remove condition/i }));
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({ conditions: [] })
    );
  });
});

// ─── 7. Per-type value pickers ─────────────────────────────────────────────

describe('FilterBar per-type value pickers', () => {
  it('SingleSelect value picker renders <option> elements from property.options', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
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
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldStat',
              field_name: 'Status',
              operator: 'is' as const,
              value: ['Todo'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    // Wait for fields to load from mock
    await tick();
    await tick();
    // The value picker for SingleSelect is a <select>; options include the field's options
    // We look for the options by name
    expect(screen.queryByText('Todo')).toBeInTheDocument();
    expect(screen.queryByText('Done')).toBeInTheDocument();
  });

  it('DateTime value picker renders <input type="date"> when fields are loaded', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fldDate',
        field_name: 'Due Date',
        type: 5,
        is_primary: false,
        property: null,
      },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldDate',
              field_name: 'Due Date',
              operator: 'is' as const,
              value: ['2026-01-01'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const dateInput = document.querySelector('input[type="date"]') as HTMLInputElement;
    expect(dateInput).toBeTruthy();
  });

  it('isEmpty / isNotEmpty does NOT render a value picker', async () => {
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldT',
              field_name: 'Title',
              operator: 'isEmpty' as const,
              value: [],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    // No text input or number input should be present for unary operators
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(document.querySelector('input[type="number"]')).not.toBeTruthy();
  });
});

// ─── 8. Fields loaded lazily on popover open ───────────────────────────────

describe('FilterBar field loading', () => {
  it('fetches fields when popover is opened for the first time', async () => {
    const listFieldsMock = vi.mocked(api.lark.listFields);
    listFieldsMock.mockResolvedValue([
      {
        field_id: 'fldT',
        field_name: 'Title',
        type: 1,
        is_primary: true,
        property: null,
      },
    ]);
    render(FilterBar, { props: defaultProps });
    expect(listFieldsMock).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    expect(listFieldsMock).toHaveBeenCalledWith('appA', 'tblA');
  });

  it('does not re-fetch fields on subsequent popover opens', async () => {
    const listFieldsMock = vi.mocked(api.lark.listFields);
    listFieldsMock.mockResolvedValue([]);
    render(FilterBar, { props: defaultProps });
    // Open popover
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    // Close popover via backdrop
    const backdrop = document.querySelector('[data-testid="filter-backdrop"]') as HTMLElement;
    await fireEvent.click(backdrop);
    await tick();
    // Open again
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    // Should only have been called once
    expect(listFieldsMock).toHaveBeenCalledTimes(1);
  });

  it('falling listFields call falls back to empty list gracefully', async () => {
    const listFieldsMock = vi.mocked(api.lark.listFields);
    listFieldsMock.mockRejectedValueOnce(new Error('network error'));
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    // Popover still renders even though fields failed to load
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
});
