import { describe, it, expect, beforeEach } from 'vitest';
import { editorTabs } from './editor-tabs.svelte';

beforeEach(() => {
  editorTabs.reset();
});

describe('editorTabs store', () => {
  it('defaults to no open files and null active path', () => {
    expect(editorTabs.open('ws_a')).toEqual([]);
    expect(editorTabs.active('ws_a')).toBeNull();
    expect(editorTabs.activeFile('ws_a')).toBeNull();
  });

  it('openFile appends unique paths and activates them', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_a', 'src/b.ts', 'B', 'shaB', false);
    expect(editorTabs.open('ws_a').map((f) => f.path)).toEqual(['src/a.ts', 'src/b.ts']);
    // The most-recently-opened file becomes active.
    expect(editorTabs.active('ws_a')).toBe('src/b.ts');
  });

  it('openFile re-activates an already-open file without duplicating or overwriting content', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'first content', 'shaA', false);
    editorTabs.updateContent('ws_a', 'src/a.ts', 'edited locally');
    // Open again with different content — the existing buffer must NOT be replaced.
    editorTabs.openFile('ws_a', 'src/a.ts', 'fresh from disk', 'shaA', false);
    expect(editorTabs.open('ws_a').length).toBe(1);
    expect(editorTabs.activeFile('ws_a')?.content).toBe('edited locally');
    expect(editorTabs.active('ws_a')).toBe('src/a.ts');
  });

  it('setActive switches active path when the file is open', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_a', 'src/b.ts', 'B', 'shaB', false);
    editorTabs.setActive('ws_a', 'src/a.ts');
    expect(editorTabs.active('ws_a')).toBe('src/a.ts');
  });

  it('setActive is a no-op when path is not open', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'A', 'shaA', false);
    editorTabs.setActive('ws_a', 'src/ghost.ts');
    expect(editorTabs.active('ws_a')).toBe('src/a.ts');
  });

  it('updateContent flips dirty when content diverges from saved', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'hello', 'shaA', false);
    expect(editorTabs.activeFile('ws_a')?.dirty).toBe(false);
    editorTabs.updateContent('ws_a', 'src/a.ts', 'hello there');
    expect(editorTabs.activeFile('ws_a')?.dirty).toBe(true);
  });

  it('updateContent leaves dirty alone when content equals current buffer', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'hello', 'shaA', false);
    editorTabs.updateContent('ws_a', 'src/a.ts', 'hello');
    expect(editorTabs.activeFile('ws_a')?.dirty).toBe(false);
  });

  it('markSaved clears dirty and updates diskSha1', () => {
    editorTabs.openFile('ws_a', 'src/a.ts', 'hello', 'shaA', false);
    editorTabs.updateContent('ws_a', 'src/a.ts', 'hello there');
    editorTabs.markSaved('ws_a', 'src/a.ts', 'shaB');
    const f = editorTabs.activeFile('ws_a');
    expect(f?.dirty).toBe(false);
    expect(f?.diskSha1).toBe('shaB');
  });

  it('closeFile removes the entry and picks the next neighbour as active', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_a', 'b.ts', 'B', 'shaB', false);
    editorTabs.openFile('ws_a', 'c.ts', 'C', 'shaC', false);
    editorTabs.setActive('ws_a', 'b.ts');
    editorTabs.closeFile('ws_a', 'b.ts');
    expect(editorTabs.open('ws_a').map((f) => f.path)).toEqual(['a.ts', 'c.ts']);
    // Index 1 of the original list (b.ts) → after removal, index 1 is c.ts.
    expect(editorTabs.active('ws_a')).toBe('c.ts');
  });

  it('closeFile falls back to the previous neighbour when last in list is removed', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_a', 'b.ts', 'B', 'shaB', false);
    editorTabs.setActive('ws_a', 'b.ts');
    editorTabs.closeFile('ws_a', 'b.ts');
    expect(editorTabs.active('ws_a')).toBe('a.ts');
  });

  it('closeFile resets active to null when the last open file closes', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.closeFile('ws_a', 'a.ts');
    expect(editorTabs.open('ws_a')).toEqual([]);
    expect(editorTabs.active('ws_a')).toBeNull();
  });

  it('closeFile preserves active when a non-active file is closed', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_a', 'b.ts', 'B', 'shaB', false);
    editorTabs.setActive('ws_a', 'a.ts');
    editorTabs.closeFile('ws_a', 'b.ts');
    expect(editorTabs.active('ws_a')).toBe('a.ts');
  });

  it('isolates state per workspace', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_b', 'b.ts', 'B', 'shaB', false);
    expect(editorTabs.active('ws_a')).toBe('a.ts');
    expect(editorTabs.active('ws_b')).toBe('b.ts');
    expect(editorTabs.open('ws_a').length).toBe(1);
    expect(editorTabs.open('ws_b').length).toBe(1);
  });

  it('forget drops all editor state for a workspace', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.forget('ws_a');
    expect(editorTabs.open('ws_a')).toEqual([]);
    expect(editorTabs.active('ws_a')).toBeNull();
  });

  it('reset clears every workspace', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.openFile('ws_b', 'b.ts', 'B', 'shaB', false);
    editorTabs.reset();
    expect(editorTabs.open('ws_a')).toEqual([]);
    expect(editorTabs.open('ws_b')).toEqual([]);
  });

  it('binary-flagged files preserve isBinary across re-activation', () => {
    editorTabs.openFile('ws_a', 'img.png', '', 'shaA', true);
    expect(editorTabs.activeFile('ws_a')?.isBinary).toBe(true);
    editorTabs.openFile('ws_a', 'img.png', '', 'shaA', true);
    expect(editorTabs.activeFile('ws_a')?.isBinary).toBe(true);
  });

  it('updateContent is a no-op when the path is not open', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.updateContent('ws_a', 'never-opened.ts', 'noop');
    // The opened file is untouched — no phantom entry was inserted.
    expect(editorTabs.open('ws_a').map((f) => f.path)).toEqual(['a.ts']);
    expect(editorTabs.activeFile('ws_a')?.dirty).toBe(false);
  });

  it('markSaved is a no-op when the path is not open', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.updateContent('ws_a', 'a.ts', 'A edited');
    editorTabs.markSaved('ws_a', 'never-opened.ts', 'irrelevant');
    // a.ts is still dirty — the phantom mark didn't accidentally clear it.
    expect(editorTabs.activeFile('ws_a')?.dirty).toBe(true);
  });

  it('closeFile is a no-op when the path is not open', () => {
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.closeFile('ws_a', 'never-opened.ts');
    expect(editorTabs.open('ws_a').map((f) => f.path)).toEqual(['a.ts']);
    expect(editorTabs.active('ws_a')).toBe('a.ts');
  });

  it('activeFile returns null when state exists but active is null (last file closed)', () => {
    // Covers the `!state.active` branch in activeFile.
    editorTabs.openFile('ws_a', 'a.ts', 'A', 'shaA', false);
    editorTabs.closeFile('ws_a', 'a.ts'); // resets active to null
    // state exists for 'ws_a' but state.active === null
    expect(editorTabs.activeFile('ws_a')).toBeNull();
  });
});
