import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import TabStrip from './TabStrip.svelte';

describe('TabStrip', () => {
  it('renders all five tab buttons with the right labels', () => {
    const { getByTestId } = render(TabStrip, {
      props: { active: 'chat', onSelect: () => {} },
    });
    expect(getByTestId('tab-chat').textContent).toMatch(/Chat/);
    expect(getByTestId('tab-diff').textContent).toMatch(/Diff/);
    expect(getByTestId('tab-files').textContent).toMatch(/Files/);
    expect(getByTestId('tab-editor').textContent).toMatch(/Editor/);
    expect(getByTestId('tab-terminal').textContent).toMatch(/Terminal/);
  });

  it('marks the active tab via aria-selected', () => {
    const { getByTestId } = render(TabStrip, {
      props: { active: 'editor', onSelect: () => {} },
    });
    expect(getByTestId('tab-chat').getAttribute('aria-selected')).toBe('false');
    expect(getByTestId('tab-diff').getAttribute('aria-selected')).toBe('false');
    expect(getByTestId('tab-files').getAttribute('aria-selected')).toBe('false');
    expect(getByTestId('tab-editor').getAttribute('aria-selected')).toBe('true');
    expect(getByTestId('tab-terminal').getAttribute('aria-selected')).toBe('false');
  });

  it('only the active tab is in the focus order', () => {
    const { getByTestId } = render(TabStrip, {
      props: { active: 'terminal', onSelect: () => {} },
    });
    expect(getByTestId('tab-terminal').getAttribute('tabindex')).toBe('0');
    expect(getByTestId('tab-chat').getAttribute('tabindex')).toBe('-1');
    expect(getByTestId('tab-diff').getAttribute('tabindex')).toBe('-1');
    expect(getByTestId('tab-files').getAttribute('tabindex')).toBe('-1');
    expect(getByTestId('tab-editor').getAttribute('tabindex')).toBe('-1');
  });

  it('clicking a non-active tab fires onSelect with the tab id', async () => {
    const onSelect = vi.fn();
    const { getByTestId } = render(TabStrip, {
      props: { active: 'chat', onSelect },
    });
    await fireEvent.click(getByTestId('tab-diff'));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith('diff');
  });

  it('clicking the already-active tab still fires onSelect (caller decides what to do)', async () => {
    const onSelect = vi.fn();
    const { getByTestId } = render(TabStrip, {
      props: { active: 'files', onSelect },
    });
    await fireEvent.click(getByTestId('tab-files'));
    expect(onSelect).toHaveBeenCalledWith('files');
  });

  it('renders the keyboard hint chip per tab', () => {
    const { getByTestId } = render(TabStrip, {
      props: { active: 'chat', onSelect: () => {} },
    });
    expect(getByTestId('tab-chat').textContent).toMatch(/⌃1/);
    expect(getByTestId('tab-diff').textContent).toMatch(/⌃2/);
    expect(getByTestId('tab-files').textContent).toMatch(/⌃3/);
    expect(getByTestId('tab-editor').textContent).toMatch(/⌃4/);
    expect(getByTestId('tab-terminal').textContent).toMatch(/⌃5/);
  });

  it('clicking the editor tab fires onSelect("editor")', async () => {
    const onSelect = vi.fn();
    const { getByTestId } = render(TabStrip, {
      props: { active: 'chat', onSelect },
    });
    await fireEvent.click(getByTestId('tab-editor'));
    expect(onSelect).toHaveBeenCalledWith('editor');
  });

  it('clicking the terminal tab fires onSelect("terminal")', async () => {
    const onSelect = vi.fn();
    const { getByTestId } = render(TabStrip, {
      props: { active: 'chat', onSelect },
    });
    await fireEvent.click(getByTestId('tab-terminal'));
    expect(onSelect).toHaveBeenCalledWith('terminal');
  });
});
