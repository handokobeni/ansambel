import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

import ToolGroup from './ToolGroup.svelte';
import type { Message } from '$lib/types';

function tool(id: string): Message {
  return {
    id,
    workspace_id: 'ws',
    role: 'tool',
    text: '',
    is_partial: false,
    tool_use: { id: `toolu_${id}`, name: 'Read', input: { file_path: `/x/${id}` } },
    tool_result: null,
    created_at: 0,
  };
}

describe('ToolGroup', () => {
  it('renders a header showing the call count', () => {
    const { container } = render(ToolGroup, {
      props: { messages: [tool('a'), tool('b'), tool('c')] },
    });
    const header = container.querySelector('[data-tool-group-header]');
    expect(header?.textContent).toMatch(/3/);
    expect(header?.textContent?.toLowerCase()).toContain('tool');
  });

  it('starts expanded so no calls are hidden by default', () => {
    const { container } = render(ToolGroup, {
      props: { messages: [tool('a'), tool('b'), tool('c')] },
    });
    const header = container.querySelector<HTMLButtonElement>('[data-tool-group-toggle]')!;
    expect(header.getAttribute('aria-expanded')).toBe('true');
    // All three child bubbles should be in the DOM.
    expect(container.querySelectorAll('[data-tool-use]').length).toBe(3);
  });

  it('clicking the toggle collapses the group', async () => {
    const { container } = render(ToolGroup, {
      props: { messages: [tool('a'), tool('b'), tool('c')] },
    });
    const toggle = container.querySelector<HTMLButtonElement>('[data-tool-group-toggle]')!;
    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(container.querySelectorAll('[data-tool-use]').length).toBe(0);
  });

  it('clicking again re-expands the group', async () => {
    const { container } = render(ToolGroup, {
      props: { messages: [tool('a'), tool('b'), tool('c')] },
    });
    const toggle = container.querySelector<HTMLButtonElement>('[data-tool-group-toggle]')!;
    await fireEvent.click(toggle);
    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(container.querySelectorAll('[data-tool-use]').length).toBe(3);
  });

  it('preserves the data-tool-group container attribute for layout hooks', () => {
    const { container } = render(ToolGroup, {
      props: { messages: [tool('a'), tool('b'), tool('c'), tool('d')] },
    });
    const wrap = container.querySelector('[data-tool-group]');
    expect(wrap).not.toBeNull();
    expect(wrap?.getAttribute('data-tool-group-count')).toBe('4');
  });

  it('honors initialExpanded=false and renders the collapsed-chevron variant', () => {
    const { container } = render(ToolGroup, {
      props: { messages: [tool('a'), tool('b'), tool('c')], initialExpanded: false },
    });
    const toggle = container.querySelector<HTMLButtonElement>('[data-tool-group-toggle]')!;
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(container.querySelectorAll('[data-tool-use]').length).toBe(0);
    // Collapse state surfaces in the toggle's trailing label as "expand".
    expect(toggle.textContent?.toLowerCase()).toContain('expand');
  });
});
