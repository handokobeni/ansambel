import { describe, it, expect, beforeEach } from 'vitest';
import { terminalTabs } from './terminal-tabs.svelte';

describe('terminalTabs', () => {
  beforeEach(() => terminalTabs.reset());

  it('add returns a unique id, appends, and activates it', () => {
    const a = terminalTabs.add('ws');
    const b = terminalTabs.add('ws');
    expect(a).not.toBe(b);
    expect(terminalTabs.list('ws').map((t) => t.id)).toEqual([a, b]);
    expect(terminalTabs.activeId('ws')).toBe(b);
  });

  it('labels are monotonic "Terminal N" and not reused after close', () => {
    terminalTabs.add('ws');
    const t2 = terminalTabs.add('ws');
    expect(terminalTabs.list('ws').map((t) => t.label)).toEqual(['Terminal 1', 'Terminal 2']);
    terminalTabs.close('ws', t2!);
    const t3 = terminalTabs.add('ws');
    expect(terminalTabs.list('ws').find((t) => t.id === t3)?.label).toBe('Terminal 3');
  });

  it('caps at 6 terminals per workspace', () => {
    const ids = Array.from({ length: 6 }, () => terminalTabs.add('ws'));
    expect(ids.every((id) => id !== null)).toBe(true);
    expect(terminalTabs.add('ws')).toBeNull();
    expect(terminalTabs.list('ws')).toHaveLength(6);
  });

  it('close activates a neighbour; empties to null when last closed', () => {
    const a = terminalTabs.add('ws');
    const b = terminalTabs.add('ws');
    terminalTabs.setActive('ws', a!);
    terminalTabs.close('ws', a!);
    expect(terminalTabs.activeId('ws')).toBe(b);
    terminalTabs.close('ws', b!);
    expect(terminalTabs.activeId('ws')).toBeNull();
    expect(terminalTabs.list('ws')).toHaveLength(0);
  });

  it('isolates workspaces', () => {
    const a = terminalTabs.add('ws1');
    terminalTabs.add('ws2');
    expect(terminalTabs.list('ws1').map((t) => t.id)).toEqual([a]);
    expect(terminalTabs.list('ws2')).toHaveLength(1);
  });

  it('forget drops a workspace', () => {
    terminalTabs.add('ws');
    terminalTabs.forget('ws');
    expect(terminalTabs.list('ws')).toHaveLength(0);
    expect(terminalTabs.activeId('ws')).toBeNull();
  });
});
