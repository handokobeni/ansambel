import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import FilterBar from './FilterBar.svelte';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      listFields: vi.fn(async () => []),
      listPersonOptions: vi.fn(async () => []),
    },
  },
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
  vi.mocked(api.lark.listPersonOptions).mockClear();
  vi.mocked(api.lark.listPersonOptions).mockResolvedValue([]);
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

// ─── 8. Person (type 11) value picker ─────────────────────────────────────

describe('FilterBar Person field value picker', () => {
  it('renders <select> populated from listPersonOptions when options are available', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldPIC', field_name: 'PIC', type: 11, is_primary: false, property: null },
    ]);
    vi.mocked(api.lark.listPersonOptions).mockResolvedValue([
      { open_id: 'ou_alice', name: 'Alice' },
      { open_id: 'ou_bob', name: 'Bob' },
    ]);

    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldPIC',
              field_name: 'PIC',
              operator: 'is' as const,
              // Regression: value must be the person's display name (not open_id)
              // because Lark's Bitable filter API matches Person fields by name.
              value: ['Alice'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    // Wait for fields + person options to load
    await tick();
    await tick();
    await tick();

    // listPersonOptions should have been called with the field name
    expect(api.lark.listPersonOptions).toHaveBeenCalledWith('appA', 'tblA', 'PIC');
    // A <select> with person names as options should be rendered
    const personSelect = document.querySelector(
      '[data-testid="person-select"]'
    ) as HTMLSelectElement;
    expect(personSelect).toBeTruthy();
    expect(screen.queryByText('Alice')).toBeInTheDocument();
    expect(screen.queryByText('Bob')).toBeInTheDocument();

    // Regression: each <option> must carry the person's NAME as value — not open_id —
    // because Lark's Bitable records/search filter API matches Person fields by display name.
    const aliceOption = personSelect.querySelector('option[value="Alice"]') as HTMLOptionElement;
    expect(aliceOption).toBeTruthy();
    const bobOption = personSelect.querySelector('option[value="Bob"]') as HTMLOptionElement;
    expect(bobOption).toBeTruthy();
    // Ensure open_id is NOT used as the option value
    expect(personSelect.querySelector('option[value="ou_alice"]')).toBeNull();
    expect(personSelect.querySelector('option[value="ou_bob"]')).toBeNull();
  });

  it('sends person NAME (not open_id) to filterStore.update when selection changes', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldPIC', field_name: 'PIC', type: 11, is_primary: false, property: null },
    ]);
    vi.mocked(api.lark.listPersonOptions).mockResolvedValue([
      { open_id: 'ou_fikri', name: 'Fikri' },
      { open_id: 'ou_beni', name: 'Beni' },
    ]);

    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldPIC',
              field_name: 'PIC',
              operator: 'is' as const,
              value: [''],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    await tick();

    const personSelect = document.querySelector(
      '[data-testid="person-select"]'
    ) as HTMLSelectElement;
    expect(personSelect).toBeTruthy();

    // Simulate user picking 'Fikri' from the dropdown
    await fireEvent.change(personSelect, { target: { value: 'Fikri' } });

    // filterStore.update must be called with the person's name, never open_id
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [expect.objectContaining({ value: ['Fikri'] })],
      })
    );
    // Regression guard: open_id must NOT be sent
    const lastCall = vi.mocked(filterStore.update).mock.lastCall;
    const sentValue = lastCall?.[1]?.conditions?.[0]?.value?.[0];
    expect(sentValue).not.toMatch(/^ou_/);
  });

  it('falls back to text input when listPersonOptions errors', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldPIC', field_name: 'PIC', type: 11, is_primary: false, property: null },
    ]);
    vi.mocked(api.lark.listPersonOptions).mockRejectedValueOnce(new Error('network failure'));

    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldPIC',
              field_name: 'PIC',
              operator: 'is' as const,
              value: [''],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    await tick();

    // No person-select — should fall back to text input
    expect(document.querySelector('[data-testid="person-select"]')).not.toBeTruthy();
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });
});

// ─── 9. OPS_BY_TYPE operator ordering ─────────────────────────────────────

describe('FilterBar operator list by field type', () => {
  it('Text (type 1) shows contains/doesNotContain/is/isNot/isEmpty/isNotEmpty', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldT', field_name: 'Title', type: 1, is_primary: true, property: null },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            { field_id: 'fldT', field_name: 'Title', operator: 'contains' as const, value: ['x'] },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    // Find the operator select by aria-label
    const operatorSelects = screen.getAllByRole('combobox', { name: /operator/i });
    expect(operatorSelects.length).toBeGreaterThan(0);
    const opSel = operatorSelects[0] as HTMLSelectElement;
    const opValues = Array.from(opSel.options).map((o) => o.value);
    expect(opValues).toEqual([
      'contains',
      'doesNotContain',
      'is',
      'isNot',
      'isEmpty',
      'isNotEmpty',
    ]);
  });

  it('Person (type 11) shows only contains/doesNotContain/isEmpty/isNotEmpty (no is/isNot)', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldPIC', field_name: 'PIC', type: 11, is_primary: false, property: null },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldPIC',
              field_name: 'PIC',
              operator: 'contains' as const,
              value: ['Alice'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const operatorSelects = screen.getAllByRole('combobox', { name: /operator/i });
    expect(operatorSelects.length).toBeGreaterThan(0);
    const opSel = operatorSelects[0] as HTMLSelectElement;
    const opValues = Array.from(opSel.options).map((o) => o.value);
    expect(opValues).toEqual(['contains', 'doesNotContain', 'isEmpty', 'isNotEmpty']);
    // Ensure is/isNot are NOT present
    expect(opValues).not.toContain('is');
    expect(opValues).not.toContain('isNot');
  });

  it('Person field defaults to contains operator when added', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldPIC', field_name: 'PIC', type: 11, is_primary: false, property: null },
    ]);
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    await fireEvent.click(screen.getByRole('button', { name: /add condition/i }));
    await tick();
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: expect.arrayContaining([
          expect.objectContaining({ field_id: 'fldPIC', operator: 'contains' }),
        ]),
      })
    );
  });

  it('Text field defaults to contains operator when added', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldT', field_name: 'Title', type: 1, is_primary: true, property: null },
    ]);
    render(FilterBar, { props: defaultProps });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    await fireEvent.click(screen.getByRole('button', { name: /add condition/i }));
    await tick();
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: expect.arrayContaining([
          expect.objectContaining({ field_id: 'fldT', operator: 'contains' }),
        ]),
      })
    );
  });

  it('Changing field from Text to Person resets operator to contains', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldT', field_name: 'Title', type: 1, is_primary: true, property: null },
      { field_id: 'fldPIC', field_name: 'PIC', type: 11, is_primary: false, property: null },
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
              operator: 'is' as const,
              value: ['exact match'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    // Change field from Title (Text, type 1) to PIC (Person, type 11)
    const fieldSelect = screen.getAllByRole('combobox', { name: /field/i })[0] as HTMLSelectElement;
    expect(fieldSelect).toBeTruthy();
    await fireEvent.change(fieldSelect, { target: { value: 'fldPIC' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [expect.objectContaining({ field_id: 'fldPIC', operator: 'contains' })],
      })
    );
  });
});

// ─── 10. Fields loaded lazily on popover open ─────────────────────────────

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

// ─── 11. Value reset on field / operator change (Bug 1 regression) ──────────

describe('FilterBar value reset on field/operator change', () => {
  it('Changing field from Text to Person resets value to empty string placeholder', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fld_title', field_name: 'Task name', type: 1, is_primary: true, property: null },
      {
        field_id: 'fld_pic',
        field_name: 'Assignee (PIC)',
        type: 11,
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
              field_id: 'fld_title',
              field_name: 'Task name',
              operator: 'contains' as const,
              value: ['user'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const fieldSelect = screen.getAllByRole('combobox', { name: /field/i })[0] as HTMLSelectElement;
    expect(fieldSelect).toBeTruthy();
    await fireEvent.change(fieldSelect, { target: { value: 'fld_pic' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [
          expect.objectContaining({
            field_id: 'fld_pic',
            // value must be reset — stale 'user' string must NOT carry over
            value: [''],
          }),
        ],
      })
    );
  });

  it('Changing operator to isEmpty resets value to empty array', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fldS',
        field_name: 'Status',
        type: 3,
        is_primary: false,
        property: { options: [{ id: 'o1', name: 'Done' }] },
      },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fldS',
              field_name: 'Status',
              operator: 'is' as const,
              value: ['Done'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const operatorSelect = screen.getAllByRole('combobox', {
      name: /operator/i,
    })[0] as HTMLSelectElement;
    expect(operatorSelect).toBeTruthy();
    await fireEvent.change(operatorSelect, { target: { value: 'isEmpty' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [
          expect.objectContaining({
            operator: 'isEmpty',
            value: [],
          }),
        ],
      })
    );
  });

  it('Changing operator from isEmpty to contains seeds empty string value', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      { field_id: 'fldT', field_name: 'Title', type: 1, is_primary: true, property: null },
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
              operator: 'isEmpty' as const,
              value: [],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const operatorSelect = screen.getAllByRole('combobox', {
      name: /operator/i,
    })[0] as HTMLSelectElement;
    expect(operatorSelect).toBeTruthy();
    await fireEvent.change(operatorSelect, { target: { value: 'contains' } });
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [
          expect.objectContaining({
            operator: 'contains',
            value: [''],
          }),
        ],
      })
    );
  });
});

// ─── 12. mappedFieldIds prop hides bound fields (Bug 2 regression) ──────────

describe('FilterBar mappedFieldIds prop', () => {
  it('Field picker hides fields whose IDs are in mappedFieldIds', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_status',
        field_name: 'Task Status',
        type: 3,
        is_primary: false,
        property: { options: [] },
      },
      { field_id: 'fld_title', field_name: 'Task name', type: 1, is_primary: true, property: null },
      {
        field_id: 'fld_assignee',
        field_name: 'Assignee',
        type: 11,
        is_primary: false,
        property: null,
      },
      {
        field_id: 'fld_sprint',
        field_name: 'Sprint Status',
        type: 3,
        is_primary: false,
        property: { options: [] },
      },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        mappedFieldIds: new Set(['fld_status', 'fld_title']),
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fld_assignee',
              field_name: 'Assignee',
              operator: 'contains' as const,
              value: [''],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    // Find the field picker <select>
    const fieldSelects = screen.getAllByRole('combobox', { name: /field/i });
    expect(fieldSelects.length).toBeGreaterThan(0);
    const fieldSelect = fieldSelects[0] as HTMLSelectElement;
    const optionValues = Array.from(fieldSelect.options).map((o) => o.value);
    // Mapped fields must NOT appear
    expect(optionValues).not.toContain('fld_status');
    expect(optionValues).not.toContain('fld_title');
    // Non-mapped supported fields must appear
    expect(optionValues).toContain('fld_assignee');
    expect(optionValues).toContain('fld_sprint');
  });

  it('Field picker shows all supported fields when mappedFieldIds is empty', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_status',
        field_name: 'Task Status',
        type: 3,
        is_primary: false,
        property: { options: [] },
      },
      { field_id: 'fld_title', field_name: 'Task name', type: 1, is_primary: true, property: null },
      {
        field_id: 'fld_assignee',
        field_name: 'Assignee',
        type: 11,
        is_primary: false,
        property: null,
      },
      {
        field_id: 'fld_sprint',
        field_name: 'Sprint Status',
        type: 3,
        is_primary: false,
        property: { options: [] },
      },
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        // mappedFieldIds omitted — defaults to empty Set
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fld_title',
              field_name: 'Task name',
              operator: 'contains' as const,
              value: [''],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const fieldSelects = screen.getAllByRole('combobox', { name: /field/i });
    expect(fieldSelects.length).toBeGreaterThan(0);
    const fieldSelect = fieldSelects[0] as HTMLSelectElement;
    const optionValues = Array.from(fieldSelect.options).map((o) => o.value);
    // All 4 supported fields must appear when no IDs are excluded
    expect(optionValues).toContain('fld_status');
    expect(optionValues).toContain('fld_title');
    expect(optionValues).toContain('fld_assignee');
    expect(optionValues).toContain('fld_sprint');
  });
});
