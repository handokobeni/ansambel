import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import FilterBar from './FilterBar.svelte';

vi.mock('$lib/ipc', () => ({
  api: { lark: { listFields: vi.fn(async () => []) } },
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
    render(FilterBar, {
      props: {
        repoId: 'repo-1',
        appToken: 'appA',
        tableId: 'tblA',
        filters: { conjunction: 'and', conditions: [] },
      },
    });
    expect(screen.queryByRole('combobox', { name: /conjunction/i })).not.toBeInTheDocument();
  });
});
