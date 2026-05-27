import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import TerminalTabBar from './TerminalTabBar.svelte';
import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';

describe('TerminalTabBar', () => {
  beforeEach(() => terminalTabs.reset());

  it('renders a tab per terminal and marks the active one', () => {
    terminalTabs.add('ws');
    const b = terminalTabs.add('ws');
    const { getAllByTestId, getByText } = render(TerminalTabBar, { props: { workspaceId: 'ws' } });
    expect(getAllByTestId('terminal-tab')).toHaveLength(2);
    expect(getByText('Terminal 1')).toBeTruthy();
    const tabs = getAllByTestId('terminal-tab');
    expect(tabs.find((t) => t.getAttribute('data-id') === b)?.getAttribute('aria-selected')).toBe(
      'true'
    );
  });

  it('clicking a tab activates it', async () => {
    const a = terminalTabs.add('ws');
    terminalTabs.add('ws');
    const { getAllByTestId } = render(TerminalTabBar, { props: { workspaceId: 'ws' } });
    const tabA = getAllByTestId('terminal-tab').find((t) => t.getAttribute('data-id') === a)!;
    await fireEvent.click(tabA);
    expect(terminalTabs.activeId('ws')).toBe(a);
  });

  it('the + button calls onAdd', async () => {
    terminalTabs.add('ws');
    const onAdd = vi.fn();
    const { getByLabelText } = render(TerminalTabBar, {
      props: { workspaceId: 'ws', onAdd, onClose: vi.fn() },
    });
    await fireEvent.click(getByLabelText(/new terminal/i));
    expect(onAdd).toHaveBeenCalled();
  });

  it('+ is disabled at the cap of 6', () => {
    for (let i = 0; i < 6; i++) terminalTabs.add('ws');
    const { getByLabelText } = render(TerminalTabBar, {
      props: { workspaceId: 'ws', onAdd: vi.fn(), onClose: vi.fn() },
    });
    expect((getByLabelText(/new terminal/i) as HTMLButtonElement).disabled).toBe(true);
  });

  it('the × button calls onClose with the tab id', async () => {
    const a = terminalTabs.add('ws');
    const onClose = vi.fn();
    const { getAllByTestId } = render(TerminalTabBar, {
      props: { workspaceId: 'ws', onAdd: vi.fn(), onClose },
    });
    const closeBtn = getAllByTestId('terminal-tab-close').find(
      (b) => b.getAttribute('data-id') === a
    )!;
    await fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalledWith(a);
  });
});
