import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('$lib/stores/slash-commands.svelte', () => {
  const sample = [
    { name: 'help', description: 'Show help', source: { kind: 'builtin' } },
    {
      name: 'agents',
      description: 'Manage agents',
      source: { kind: 'builtin' },
    },
    {
      name: 'writing-plans',
      description: 'Spec → plan',
      source: { kind: 'plugin', plugin: 'superpowers' },
    },
  ];
  return {
    slashCommands: {
      filtered: vi.fn((prefix: string) =>
        prefix === ''
          ? sample
          : sample.filter((c) => c.name.toLowerCase().startsWith(prefix.toLowerCase()))
      ),
    },
  };
});

import SlashCommandPicker from './SlashCommandPicker.svelte';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('SlashCommandPicker', () => {
  it('renders all filtered items when open with empty filterText', () => {
    const { getAllByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        onSelect: vi.fn(),
        onClose: vi.fn(),
      },
    });
    expect(getAllByTestId('slash-picker-row').length).toBe(3);
  });

  it('renders only prefix-matched items when filterText is set', () => {
    const { getAllByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: 'wri',
        onSelect: vi.fn(),
        onClose: vi.fn(),
      },
    });
    const rows = getAllByTestId('slash-picker-row');
    expect(rows.length).toBe(1);
    expect(rows[0].textContent).toMatch(/writing-plans/);
  });

  it('Enter on the highlighted item fires onSelect with the name', async () => {
    const onSelect = vi.fn();
    render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        onSelect,
        onClose: vi.fn(),
      },
    });
    await fireEvent.keyDown(document, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('help');
  });

  it('ArrowDown moves the highlight; Enter selects the new highlight', async () => {
    const onSelect = vi.fn();
    render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        onSelect,
        onClose: vi.fn(),
      },
    });
    await fireEvent.keyDown(document, { key: 'ArrowDown' });
    await fireEvent.keyDown(document, { key: 'Enter' });
    // Items render in their natural order; index 1 is 'agents' (the mock returns help, agents, writing-plans in that order).
    expect(onSelect).toHaveBeenCalledWith('agents');
  });

  it('Esc fires onClose without calling onSelect', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(SlashCommandPicker, {
      props: { open: true, filterText: '', onSelect, onClose },
    });
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('clicking an item fires onSelect with that name', async () => {
    const onSelect = vi.fn();
    const { getAllByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        onSelect,
        onClose: vi.fn(),
      },
    });
    await fireEvent.click(getAllByTestId('slash-picker-row')[2]);
    expect(onSelect).toHaveBeenCalledWith('writing-plans');
  });

  it('shows empty-state hint when filtered list is empty', () => {
    const { getByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: 'zzz-no-match',
        onSelect: vi.fn(),
        onClose: vi.fn(),
      },
    });
    expect(getByTestId('slash-picker-empty').textContent).toMatch(/no slash commands/i);
  });
});
