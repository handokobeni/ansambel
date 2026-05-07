import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import EditorTabBar from './EditorTabBar.svelte';
import { editorTabs } from '$lib/stores/editor-tabs.svelte';

beforeEach(() => {
  editorTabs.reset();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('EditorTabBar', () => {
  it('renders nothing when no files are open', () => {
    const { container } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    expect(container.querySelector('[data-testid="editor-tab-bar"]')).toBeNull();
  });

  it('renders one button per open file with the basename label', async () => {
    editorTabs.openFile('ws_a', 'src/main.ts', 'A', 'sha', false);
    editorTabs.openFile('ws_a', 'docs/readme.md', 'B', 'sha', false);
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const tabs = await findAllByTestId('editor-tab');
    expect(tabs).toHaveLength(2);
    expect(tabs[0].textContent).toMatch(/main\.ts/);
    expect(tabs[1].textContent).toMatch(/readme\.md/);
  });

  it('marks dirty files with a • indicator', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'sha', false);
    editorTabs.openFile('ws_a', 'b.ts', 'B', 'sha', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'A modified');
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const dirty = await findAllByTestId('editor-tab-dirty');
    expect(dirty).toHaveLength(1);
    expect(dirty[0].closest('[data-testid="editor-tab"]')?.getAttribute('data-path')).toBe('a.ts');
  });

  it('clicking a tab activates it', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'sha', false);
    editorTabs.openFile('ws_a', 'b.ts', 'B', 'sha', false);
    expect(editorTabs.active('ws_a')).toBe('b.ts');
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const tabs = await findAllByTestId('editor-tab');
    await fireEvent.click(tabs[0]);
    expect(editorTabs.active('ws_a')).toBe('a.ts');
  });

  it('clicking close on a clean file removes it without confirmation', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'sha', false);
    const confirmSpy = vi.spyOn(window, 'confirm');
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const close = (await findAllByTestId('editor-tab-close'))[0];
    await fireEvent.click(close);
    expect(editorTabs.open('ws_a')).toEqual([]);
    expect(confirmSpy).not.toHaveBeenCalled();
  });

  it('clicking close on a dirty file confirms before removing', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'sha', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'A modified');
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const close = (await findAllByTestId('editor-tab-close'))[0];
    await fireEvent.click(close);
    expect(editorTabs.open('ws_a')).toEqual([]);
  });

  it('cancelling the close confirmation keeps the file open', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'sha', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'A modified');
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const close = (await findAllByTestId('editor-tab-close'))[0];
    await fireEvent.click(close);
    expect(editorTabs.open('ws_a')).toHaveLength(1);
  });

  it('Enter key on the close button triggers close', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'sha', false);
    const { findAllByTestId } = render(EditorTabBar, { props: { workspaceId: 'ws_a' } });
    const close = (await findAllByTestId('editor-tab-close'))[0];
    await fireEvent.keyDown(close, { key: 'Enter' });
    expect(editorTabs.open('ws_a')).toEqual([]);
  });
});
