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
