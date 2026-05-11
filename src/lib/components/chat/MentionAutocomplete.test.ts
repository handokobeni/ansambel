import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import MentionAutocomplete from './MentionAutocomplete.svelte';

describe('MentionAutocomplete', () => {
  const noop = () => {};

  it('renders each file as a row', () => {
    const { getAllByTestId } = render(MentionAutocomplete, {
      props: {
        files: ['src/main.ts', 'src/lib.ts', 'README.md'],
        query: '',
        highlighted: 0,
        onSelect: noop,
        onHighlight: noop,
        onDismiss: noop,
      },
    });
    const rows = getAllByTestId('mention-row');
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain('src/main.ts');
  });

  it('marks the row at `highlighted` index via aria-selected', () => {
    const { getAllByTestId } = render(MentionAutocomplete, {
      props: {
        files: ['a.ts', 'b.ts', 'c.ts'],
        query: '',
        highlighted: 1,
        onSelect: noop,
        onHighlight: noop,
        onDismiss: noop,
      },
    });
    const rows = getAllByTestId('mention-row');
    expect(rows[0].getAttribute('aria-selected')).toBe('false');
    expect(rows[1].getAttribute('aria-selected')).toBe('true');
    expect(rows[2].getAttribute('aria-selected')).toBe('false');
  });

  it('renders an empty-state when files is empty', () => {
    const { getByTestId } = render(MentionAutocomplete, {
      props: {
        files: [],
        query: 'nomatch',
        highlighted: 0,
        onSelect: noop,
        onHighlight: noop,
        onDismiss: noop,
      },
    });
    expect(getByTestId('mention-empty')).toBeTruthy();
  });

  it('shows the typed query inside the empty-state', () => {
    const { getByTestId } = render(MentionAutocomplete, {
      props: {
        files: [],
        query: 'zzz',
        highlighted: 0,
        onSelect: noop,
        onHighlight: noop,
        onDismiss: noop,
      },
    });
    expect(getByTestId('mention-empty').textContent).toContain('zzz');
  });

  it('clicking a row calls onSelect with that path', async () => {
    const onSelect = vi.fn();
    const { getAllByTestId } = render(MentionAutocomplete, {
      props: {
        files: ['a.ts', 'b.ts', 'c.ts'],
        query: '',
        highlighted: 0,
        onSelect,
        onHighlight: noop,
        onDismiss: noop,
      },
    });
    const rows = getAllByTestId('mention-row');
    await fireEvent.click(rows[1]);
    expect(onSelect).toHaveBeenCalledWith('b.ts');
  });

  it('mouseenter on a row calls onHighlight with its index', async () => {
    const onHighlight = vi.fn();
    const { getAllByTestId } = render(MentionAutocomplete, {
      props: {
        files: ['a.ts', 'b.ts'],
        query: '',
        highlighted: 0,
        onSelect: noop,
        onHighlight,
        onDismiss: noop,
      },
    });
    const rows = getAllByTestId('mention-row');
    await fireEvent.mouseEnter(rows[1]);
    expect(onHighlight).toHaveBeenCalledWith(1);
  });

  it('shows a loading indicator when loading prop is true', () => {
    const { getByTestId } = render(MentionAutocomplete, {
      props: {
        files: [],
        query: '',
        highlighted: 0,
        loading: true,
        onSelect: noop,
        onHighlight: noop,
        onDismiss: noop,
      },
    });
    expect(getByTestId('mention-loading')).toBeTruthy();
  });

  it('calls onDismiss when the user clicks outside the dropdown', async () => {
    const onDismiss = vi.fn();
    render(MentionAutocomplete, {
      props: {
        files: ['a.ts'],
        query: '',
        highlighted: 0,
        onSelect: noop,
        onHighlight: noop,
        onDismiss,
      },
    });
    // Click on document body, which is outside the listbox.
    await fireEvent.click(document.body);
    expect(onDismiss).toHaveBeenCalled();
  });

  it('does NOT call onDismiss when clicking inside the dropdown', async () => {
    const onDismiss = vi.fn();
    const { getByTestId } = render(MentionAutocomplete, {
      props: {
        files: ['a.ts'],
        query: '',
        highlighted: 0,
        onSelect: noop,
        onHighlight: noop,
        onDismiss,
      },
    });
    await fireEvent.click(getByTestId('mention-autocomplete'));
    expect(onDismiss).not.toHaveBeenCalled();
  });
});
