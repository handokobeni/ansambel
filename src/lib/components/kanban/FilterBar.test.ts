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
      listLookupOptions: vi.fn(async () => []),
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
  vi.mocked(api.lark.listLookupOptions).mockClear();
  vi.mocked(api.lark.listLookupOptions).mockResolvedValue([]);
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
              // Value is open_id because Lark's records/search with user_id_type=open_id
              // and is/isNot operators matches Person fields by open_id.
              value: ['ou_alice'],
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

    // Each <option> must carry the person's open_id as value because Lark's
    // records/search filter API with user_id_type=open_id matches Person fields by open_id.
    const aliceOption = personSelect.querySelector('option[value="ou_alice"]') as HTMLOptionElement;
    expect(aliceOption).toBeTruthy();
    const bobOption = personSelect.querySelector('option[value="ou_bob"]') as HTMLOptionElement;
    expect(bobOption).toBeTruthy();
    // Ensure display name is NOT used as the option value
    expect(personSelect.querySelector('option[value="Alice"]')).toBeNull();
    expect(personSelect.querySelector('option[value="Bob"]')).toBeNull();
  });

  it('Person field with "is" operator sends open_id as condition value', async () => {
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

    // Simulate user picking 'Fikri' from the dropdown (value is open_id)
    await fireEvent.change(personSelect, { target: { value: 'ou_fikri' } });

    // filterStore.update must be called with the person's open_id, not the display name
    expect(filterStore.update).toHaveBeenCalledWith(
      'repo-1',
      expect.objectContaining({
        conditions: [expect.objectContaining({ value: ['ou_fikri'] })],
      })
    );
    // Regression guard: open_id MUST be sent (not the display name)
    const lastCall = vi.mocked(filterStore.update).mock.lastCall;
    const sentValue = lastCall?.[1]?.conditions?.[0]?.value?.[0];
    expect(sentValue).toMatch(/^ou_/);
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

  it('Person (type 11) shows is/isNot/contains/doesNotContain/isEmpty/isNotEmpty', async () => {
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
              operator: 'is' as const,
              value: ['ou_alice'],
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
    expect(opValues).toEqual([
      'is',
      'isNot',
      'contains',
      'doesNotContain',
      'isEmpty',
      'isNotEmpty',
    ]);
    // Ensure is/isNot ARE present (required by Lark Person filter with open_id)
    expect(opValues).toContain('is');
    expect(opValues).toContain('isNot');
  });

  it('Person field defaults to is operator when added', async () => {
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
          expect.objectContaining({ field_id: 'fldPIC', operator: 'is' }),
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

  it('Changing field from Text to Person resets operator to is', async () => {
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
        conditions: [expect.objectContaining({ field_id: 'fldPIC', operator: 'is' })],
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

// ─── 12. mappedFieldIds / mappedFieldNames prop hides bound fields ───────────

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

  it('Field picker hides fields whose names match mappedFieldNames (case-insensitive)', async () => {
    // The field_id for Task Status here (fld_status_different) is NOT in
    // mappedFieldIds, but the name 'Task Status' IS in mappedFieldNames.
    // This proves the name-based fallback works when field_ids don't line up.
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_status_different',
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
    ]);
    render(FilterBar, {
      props: {
        ...defaultProps,
        // fld_status_stale is the STALE id (not in the schema above)
        mappedFieldIds: new Set(['fld_status_stale']),
        // But the name 'task status' IS the name of the field above
        mappedFieldNames: new Set(['task status']),
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
    const fieldSelects = screen.getAllByRole('combobox', { name: /field/i });
    expect(fieldSelects.length).toBeGreaterThan(0);
    const fieldSelect = fieldSelects[0] as HTMLSelectElement;
    const optionValues = Array.from(fieldSelect.options).map((o) => o.value);
    // Task Status must be hidden by name even though its ID doesn't match mappedFieldIds
    expect(optionValues).not.toContain('fld_status_different');
    // Non-mapped fields must still appear
    expect(optionValues).toContain('fld_assignee');
    // fld_title is hidden by mappedFieldNames matching 'task name' — not set here so it shows
    expect(optionValues).toContain('fld_title');
  });
});

// ─── 13. OPS_BY_TYPE new field types ─────────────────────────────────────────

describe('FilterBar new field types (Lookup, Formula, Created/Modified Time/By)', () => {
  it('Lookup (type 19) shows is/isNot/contains/doesNotContain/isEmpty/isNotEmpty', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_lookup',
        field_name: 'Sprint Status',
        type: 19,
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
              field_id: 'fld_lookup',
              field_name: 'Sprint Status',
              operator: 'is' as const,
              value: ['optDone'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const operatorSelects = screen.getAllByRole('combobox', { name: /operator/i });
    const opSel = operatorSelects[0] as HTMLSelectElement;
    const opValues = Array.from(opSel.options).map((o) => o.value);
    expect(opValues).toEqual([
      'is',
      'isNot',
      'contains',
      'doesNotContain',
      'isEmpty',
      'isNotEmpty',
    ]);
  });

  it('Created Time (type 1001) shows comparison operators including isGreater', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_created',
        field_name: 'Created Time',
        type: 1001,
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
              field_id: 'fld_created',
              field_name: 'Created Time',
              operator: 'isGreater' as const,
              value: ['2026-01-01'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    const operatorSelects = screen.getAllByRole('combobox', { name: /operator/i });
    const opSel = operatorSelects[0] as HTMLSelectElement;
    const opValues = Array.from(opSel.options).map((o) => o.value);
    expect(opValues).toContain('isGreater');
    expect(opValues).toContain('isGreaterEqual');
    expect(opValues).toContain('isLess');
    expect(opValues).toContain('isLessEqual');
  });

  it('Lookup field (type 19) renders text input as value picker', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_lookup',
        field_name: 'Sprint Status',
        type: 19,
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
              field_id: 'fld_lookup',
              field_name: 'Sprint Status',
              operator: 'contains' as const,
              value: ['Active'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    // Should render a text input, not a <select>
    const textInput = screen.getByRole('textbox') as HTMLInputElement;
    expect(textInput).toBeTruthy();
    expect(textInput.type).toBe('text');
    // Should NOT render a date input or number input
    expect(document.querySelector('input[type="date"]')).toBeNull();
    expect(document.querySelector('input[type="number"]')).toBeNull();
  });

  it('Created Time (type 1001) renders date input as value picker', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_created',
        field_name: 'Created Time',
        type: 1001,
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
              field_id: 'fld_created',
              field_name: 'Created Time',
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

  it('Lookup field defaults to "is" operator when added', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_lookup',
        field_name: 'Sprint Status',
        type: 19,
        is_primary: false,
        property: null,
      },
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
          expect.objectContaining({ field_id: 'fld_lookup', operator: 'is' }),
        ]),
      })
    );
  });

  it('Lookup field renders <select> with options from listLookupOptions', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_lookup',
        field_name: 'Sprint Status',
        type: 19,
        is_primary: false,
        property: null,
      },
    ]);
    vi.mocked(api.lark.listLookupOptions).mockResolvedValue([
      { option_id: 'optDone', name: 'Done' },
      { option_id: 'optInProg', name: 'In Progress' },
      { option_id: 'optUpcoming', name: 'Upcoming +1' },
    ]);

    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fld_lookup',
              field_name: 'Sprint Status',
              operator: 'is' as const,
              value: ['optDone'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    await tick();

    // listLookupOptions should be called with the field_id
    expect(api.lark.listLookupOptions).toHaveBeenCalledWith('appA', 'tblA', 'fld_lookup');

    // A <select> with option names should be rendered
    const lookupSelect = document.querySelector(
      '[data-testid="lookup-select"]'
    ) as HTMLSelectElement;
    expect(lookupSelect).toBeTruthy();
    expect(screen.queryByText('Done')).toBeInTheDocument();
    expect(screen.queryByText('In Progress')).toBeInTheDocument();
    expect(screen.queryByText('Upcoming +1')).toBeInTheDocument();

    // Each <option> must use option_id as value (not name)
    const doneOption = lookupSelect.querySelector('option[value="optDone"]') as HTMLOptionElement;
    expect(doneOption).toBeTruthy();
    const inProgOption = lookupSelect.querySelector(
      'option[value="optInProg"]'
    ) as HTMLOptionElement;
    expect(inProgOption).toBeTruthy();
    // Name must NOT be used as value
    expect(lookupSelect.querySelector('option[value="Done"]')).toBeNull();
  });

  it('Lookup field falls back to text input when listLookupOptions returns empty', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_lookup',
        field_name: 'Sprint Status',
        type: 19,
        is_primary: false,
        property: null,
      },
    ]);
    // Default mock already returns [] for listLookupOptions
    render(FilterBar, {
      props: {
        ...defaultProps,
        filters: {
          conjunction: 'and' as const,
          conditions: [
            {
              field_id: 'fld_lookup',
              field_name: 'Sprint Status',
              operator: 'contains' as const,
              value: ['Active'],
            },
          ],
        },
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: /filter/i }));
    await tick();
    await tick();
    await tick();

    // No lookup-select — should fall back to text input
    expect(document.querySelector('[data-testid="lookup-select"]')).not.toBeTruthy();
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });

  it('Lookup (type 19) appears in field picker (not excluded by unsupported type)', async () => {
    vi.mocked(api.lark.listFields).mockResolvedValue([
      {
        field_id: 'fld_lookup',
        field_name: 'Sprint Status',
        type: 19,
        is_primary: false,
        property: null,
      },
      { field_id: 'fld_title', field_name: 'Task name', type: 1, is_primary: true, property: null },
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
    const fieldSelect = fieldSelects[0] as HTMLSelectElement;
    const optionValues = Array.from(fieldSelect.options).map((o) => o.value);
    // Lookup field must now appear in the picker
    expect(optionValues).toContain('fld_lookup');
  });
});
