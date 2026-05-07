import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import type { FileWriteResponse } from '$lib/types';

type WriteCall = {
  workspaceId: string;
  path: string;
  content: string;
  expectedSha1: string;
};

let writeCalls: WriteCall[] = [];
let writeBehavior: 'success' | 'sha-mismatch' | 'other-error' = 'success';

vi.mock('@tauri-apps/api/core', () => {
  return {
    Channel: class {},
    invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'file_write' && args) {
        writeCalls.push({
          workspaceId: args.workspaceId as string,
          path: args.path as string,
          content: args.content as string,
          expectedSha1: args.expectedSha1 as string,
        });
        if (writeBehavior === 'sha-mismatch') {
          throw 'FileChangedOnDisk: expected sha1 abc, got def';
        }
        if (writeBehavior === 'other-error') {
          throw 'permission denied';
        }
        const resp: FileWriteResponse = {
          sha1: 'sha-after-write',
          size: (args.content as string).length,
        };
        return resp;
      }
      return undefined;
    }),
  };
});

import Editor from './Editor.svelte';
import { editorTabs } from '$lib/stores/editor-tabs.svelte';
import { getToasts, removeToast } from '$lib/stores/toasts.svelte';

function clearToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

beforeEach(() => {
  writeCalls = [];
  writeBehavior = 'success';
  editorTabs.reset();
  clearToasts();
});

afterEach(() => {
  vi.clearAllMocks();
  clearToasts();
});

describe('Editor', () => {
  it('renders the empty placeholder when no file is open', async () => {
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    expect(await findByTestId('editor-empty')).toBeTruthy();
  });

  it('shows the active file path in the header label after opening a file', async () => {
    editorTabs.openFile('ws_a', 'src/main.ts', 'console.log(1);', 'sha-orig', false);
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const label = await findByTestId('editor-active-path');
    expect(label.textContent).toMatch(/src\/main\.ts/);
  });

  it('shows the dirty indicator (•) when the buffer has unsaved changes', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'A modified');
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const label = await findByTestId('editor-active-path');
    expect(label.textContent).toMatch(/•/);
  });

  it('renders the binary placeholder when the active file is binary', async () => {
    editorTabs.openFile('ws_a', 'img.png', '', 'shaImg', true);
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    expect(await findByTestId('editor-binary')).toBeTruthy();
  });

  it('disables the Save button for binary files', async () => {
    editorTabs.openFile('ws_a', 'img.png', '', 'shaImg', true);
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const btn = (await findByTestId('editor-save')) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it('clicking Save invokes file_write with the current diskSha1', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'hello', 'sha-orig', false);
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const btn = await findByTestId('editor-save');
    await fireEvent.click(btn);
    await waitFor(() => expect(writeCalls.length).toBe(1));
    expect(writeCalls[0]).toMatchObject({
      workspaceId: 'ws_a',
      path: 'a.ts',
      expectedSha1: 'sha-orig',
    });
  });

  it('successful save marks the file clean and updates diskSha1 in the store', async () => {
    editorTabs.openFile('ws_a', 'a.ts', 'hello', 'sha-orig', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'hello world');
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    await fireEvent.click(await findByTestId('editor-save'));
    await waitFor(() => {
      const f = editorTabs.activeFile('ws_a');
      expect(f?.dirty).toBe(false);
      expect(f?.diskSha1).toBe('sha-after-write');
    });
  });

  it('FileChangedOnDisk surfaces an explicit "reload before saving" toast', async () => {
    writeBehavior = 'sha-mismatch';
    editorTabs.openFile('ws_a', 'a.ts', 'hello', 'sha-orig', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'hello edited');
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    await fireEvent.click(await findByTestId('editor-save'));
    await waitFor(() => {
      const toasts = Array.from(getToasts().values());
      expect(toasts.some((t) => t.message.includes('changed on disk'))).toBe(true);
    });
    // The buffer is still dirty so the user can decide what to do.
    expect(editorTabs.activeFile('ws_a')?.dirty).toBe(true);
  });

  it('non-race errors surface as a generic save-failed toast', async () => {
    writeBehavior = 'other-error';
    editorTabs.openFile('ws_a', 'a.ts', 'hello', 'sha-orig', false);
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    await fireEvent.click(await findByTestId('editor-save'));
    await waitFor(() => {
      const toasts = Array.from(getToasts().values());
      expect(toasts.some((t) => t.message.startsWith('Save failed'))).toBe(true);
    });
  });

  it('does not invoke save when there is no active file', async () => {
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    // No file open → save button isn't rendered (header shows the
    // "No file open" label instead). Verify by querying for the empty
    // label rather than the save button.
    expect(await findByTestId('editor-empty-label')).toBeTruthy();
    expect(writeCalls.length).toBe(0);
  });

  it('a programmatic doc-change dispatch updates the buffer in the store (updateListener path)', async () => {
    // Mount with no file open first so the view is fully wired before
    // the activeFile changes; opening the file after mount drives the
    // $effect's swap-doc branch with `view` already defined.
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const cm = await findByTestId('editor-codemirror');
    const { EditorView } = await import('@codemirror/view');
    const view = await waitFor(() => {
      const v = EditorView.findFromDOM(cm as HTMLElement);
      if (!v) throw new Error('view not yet mounted');
      return v;
    });
    editorTabs.openFile('ws_a', 'a.ts', 'hello', 'sha-orig', false);
    await waitFor(() => expect(view.state.doc.toString()).toBe('hello'));
    view.dispatch({
      changes: { from: 5, insert: ' world' },
    });
    await waitFor(() => {
      expect(editorTabs.activeFile('ws_a')?.content).toBe('hello world');
      expect(editorTabs.activeFile('ws_a')?.dirty).toBe(true);
    });
  });

  it('closing the last open file reverts the editor doc back to empty', async () => {
    // Mount with no file → wait for view → open a file → close it. The
    // close drives the $effect's "active became null" branch which dispatches
    // a swap back to ''.
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const cm = await findByTestId('editor-codemirror');
    const { EditorView } = await import('@codemirror/view');
    const view = await waitFor(() => {
      const v = EditorView.findFromDOM(cm as HTMLElement);
      if (!v) throw new Error('view not yet mounted');
      return v;
    });
    editorTabs.openFile('ws_a', 'a.ts', 'hello', 'sha-orig', false);
    await waitFor(() => expect(view.state.doc.toString()).toBe('hello'));
    editorTabs.closeFile('ws_a', 'a.ts');
    await waitFor(() => expect(view.state.doc.toString()).toBe(''));
    expect(editorTabs.activeFile('ws_a')).toBeNull();
  });

  it('opening a known-extension file kicks off the lazy lang import + dispatches the resolved extension', async () => {
    // The Compartment's reconfigure path runs through langExtensionsFor,
    // which fires loadLang(...).then(...) and dispatches the resolved
    // language extension once the dynamic import settles.
    const { findByTestId } = render(Editor, { props: { workspaceId: 'ws_a' } });
    const cm = await findByTestId('editor-codemirror');
    const { EditorView } = await import('@codemirror/view');
    const view = await waitFor(() => {
      const v = EditorView.findFromDOM(cm as HTMLElement);
      if (!v) throw new Error('view not yet mounted');
      return v;
    });
    editorTabs.openFile('ws_a', 'main.ts', 'const x = 1;', 'sha-ts', false);
    await waitFor(() => expect(view.state.doc.toString()).toBe('const x = 1;'));
    // Give the dynamic import a microtask window to land + redispatch.
    await new Promise((r) => setTimeout(r, 100));
    expect(view.state.doc.toString()).toBe('const x = 1;');
  });
});
