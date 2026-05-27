import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, fireEvent } from '@testing-library/svelte';
import type { TerminalChunk } from '$lib/types';

// Capture the Channel handed to terminal_spawn / terminal_reattach so
// tests can simulate streaming chunks. xterm.js itself is mocked
// because it needs a real DOM with measurement APIs that jsdom lacks.
let lastChannel: { onmessage?: (chunk: TerminalChunk) => void } | null = null;
let reattachBehavior: 'success' | 'reject' = 'reject';
let spawnBehavior: 'success' | 'reject' = 'success';
const writeCalls: { terminalId: string; bytes: number[] }[] = [];
const resizeCalls: { terminalId: string; cols: number; rows: number }[] = [];
const killCalls: string[] = [];

vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    onmessage?: (chunk: TerminalChunk) => void;
  }
  return {
    Channel: MockChannel,
    invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'terminal_reattach' && args && 'channel' in args) {
        lastChannel = args.channel as { onmessage?: (chunk: TerminalChunk) => void };
        if (reattachBehavior === 'reject') throw 'no active terminal';
        return undefined;
      }
      if (cmd === 'terminal_spawn' && args && 'channel' in args) {
        lastChannel = args.channel as { onmessage?: (chunk: TerminalChunk) => void };
        if (spawnBehavior === 'reject') throw 'pty failed';
        return undefined;
      }
      if (cmd === 'terminal_write' && args) {
        writeCalls.push({
          terminalId: args.terminalId as string,
          bytes: args.bytes as number[],
        });
        return undefined;
      }
      if (cmd === 'terminal_resize' && args) {
        resizeCalls.push({
          terminalId: args.terminalId as string,
          cols: args.cols as number,
          rows: args.rows as number,
        });
        return undefined;
      }
      if (cmd === 'terminal_kill' && args) {
        killCalls.push(args.terminalId as string);
        return undefined;
      }
      return undefined;
    }),
  };
});

// xterm.js is heavy and needs canvas/measurement APIs. Stub the parts
// the component actually calls — open(), write(), writeln(), onData(),
// dispose(), loadAddon, plus rows/cols accessors.
const writes: Uint8Array[] = [];
const stringWrites: string[] = [];
const writelns: string[] = [];
let dataHandler: ((data: string) => void) | null = null;

vi.mock('@xterm/xterm', () => {
  class MockTerminal {
    cols = 80;
    rows = 24;
    open = vi.fn();
    write = (data: string | Uint8Array) => {
      if (data instanceof Uint8Array) writes.push(data);
      else if (typeof data === 'string') stringWrites.push(data);
    };
    writeln = (line: string) => {
      writelns.push(line);
    };
    loadAddon = vi.fn();
    onData = (h: (data: string) => void) => {
      dataHandler = h;
    };
    dispose = vi.fn();
  }
  return { Terminal: MockTerminal };
});

// SerializeAddon stub: serialize() returns a per-test settable string so we
// can assert the destroy path stashes the right snapshot.
let serializeReturn = '';

vi.mock('@xterm/addon-serialize', () => {
  class SerializeAddon {
    activate = vi.fn();
    dispose = vi.fn();
    serialize = vi.fn(() => serializeReturn);
  }
  return { SerializeAddon };
});

// `fitThrows` lets a single test simulate a no-layout runtime where
// the FitAddon's `.fit()` raises — the component must swallow that.
let fitThrows = false;

vi.mock('@xterm/addon-fit', () => {
  class MockFitAddon {
    fit = vi.fn(() => {
      if (fitThrows) throw new Error('no layout');
    });
    activate = vi.fn();
    dispose = vi.fn();
  }
  return { FitAddon: MockFitAddon };
});

// jsdom doesn't ship ResizeObserver — provide a stub that captures the
// LATEST callback so tests can fire a fake resize and assert the
// component reflows + calls api.terminal.resize. observe() also fires
// once with a synthetic non-zero contentRect so the production
// `waitForLayout` helper unblocks immediately in tests.
type RoCallback = (entries: ResizeObserverEntry[], observer: ResizeObserver) => void;
let resizeCb: (() => void) | null = null;
class CapturingResizeObserver {
  cb: RoCallback;
  constructor(cb: RoCallback) {
    resizeCb = cb as unknown as () => void;
    this.cb = cb;
  }
  observe(target: Element): void {
    queueMicrotask(() => {
      const entry = {
        target,
        contentRect: { width: 800, height: 600 } as DOMRectReadOnly,
      } as ResizeObserverEntry;
      this.cb([entry], this as unknown as ResizeObserver);
    });
  }
  unobserve(): void {}
  disconnect(): void {}
}
vi.stubGlobal('ResizeObserver', CapturingResizeObserver);

// Mock the terminal-tabs store so Terminal container tests are deterministic.
// The factory runs lazily after hoisting, so module-level vars are in scope.
vi.mock('$lib/stores/terminal-tabs.svelte', () => {
  // Use a plain Map (not SvelteMap) — Svelte reactivity is not needed in jsdom.
  type TabState = { tabs: { id: string; label: string }[]; active: string | null; counter: number };
  const states = new Map<string, TabState>();
  function ensure(wsId: string): TabState {
    let s = states.get(wsId);
    if (!s) {
      s = { tabs: [], active: null, counter: 0 };
      states.set(wsId, s);
    }
    return s;
  }
  return {
    terminalTabs: {
      list: (wsId: string) => states.get(wsId)?.tabs ?? [],
      activeId: (wsId: string) => states.get(wsId)?.active ?? null,
      add: (wsId: string) => {
        const s = ensure(wsId);
        const tab = { id: `term_test_${s.counter + 1}`, label: `Terminal ${s.counter + 1}` };
        states.set(wsId, { tabs: [...s.tabs, tab], active: tab.id, counter: s.counter + 1 });
        return tab.id;
      },
      setActive: (wsId: string, id: string) => {
        const s = ensure(wsId);
        states.set(wsId, { ...s, active: id });
      },
      close: (wsId: string, id: string) => {
        const s = ensure(wsId);
        const tabs = s.tabs.filter((t) => t.id !== id);
        const active = s.active === id ? (tabs[0]?.id ?? null) : s.active;
        states.set(wsId, { ...s, tabs, active });
      },
      forget: (wsId: string) => states.delete(wsId),
      reset: () => states.clear(),
    },
  };
});

import TerminalPane from './TerminalPane.svelte';
import Terminal from './Terminal.svelte';
import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';
import { terminalSnapshots } from '$lib/stores/terminal-snapshots';

beforeEach(() => {
  lastChannel = null;
  writeCalls.length = 0;
  resizeCalls.length = 0;
  killCalls.length = 0;
  writes.length = 0;
  stringWrites.length = 0;
  writelns.length = 0;
  dataHandler = null;
  resizeCb = null;
  reattachBehavior = 'reject';
  spawnBehavior = 'success';
  fitThrows = false;
  serializeReturn = '';
  terminalTabs.reset();
  terminalSnapshots.reset();
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('TerminalPane', () => {
  it('mounts and falls back to spawn when reattach rejects', async () => {
    const { findByTestId } = render(TerminalPane, {
      props: { workspaceId: 'ws_a', terminalId: 'term_a' },
    });
    expect(await findByTestId('terminal-container')).toBeTruthy();
    await waitFor(() => expect(lastChannel).not.toBeNull());
  });

  it('uses reattach when an existing session is available', async () => {
    reattachBehavior = 'success';
    const { invoke } = await import('@tauri-apps/api/core');
    render(TerminalPane, { props: { workspaceId: 'ws_b', terminalId: 'term_b' } });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    const calls = vi.mocked(invoke).mock.calls.map((c) => c[0]);
    expect(calls).toContain('terminal_reattach');
    expect(calls).not.toContain('terminal_spawn');
  });

  it('streams TerminalChunk::Bytes into the xterm buffer', async () => {
    const { findByTestId } = render(TerminalPane, {
      props: { workspaceId: 'ws_c', terminalId: 'term_c' },
    });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    lastChannel!.onmessage!({ kind: 'bytes', bytes: [104, 105] });
    expect(writes.length).toBe(1);
    expect(Array.from(writes[0])).toEqual([104, 105]);
    void findByTestId; // narrow lint: keep within the test scope
  });

  it('writes an exited marker to the xterm buffer on exit chunk', async () => {
    const { findByTestId } = render(TerminalPane, {
      props: { workspaceId: 'ws_d', terminalId: 'term_d' },
    });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    lastChannel!.onmessage!({ kind: 'exited', code: 0 });
    await waitFor(() => {
      expect(writelns.some((l) => l.includes('exited with code 0'))).toBe(true);
    });
    void findByTestId; // narrow lint: keep within the test scope
  });

  it('forwards onData keystrokes through terminal_write using terminalId', async () => {
    render(TerminalPane, { props: { workspaceId: 'ws_e', terminalId: 'term_e' } });
    await waitFor(() => expect(dataHandler).not.toBeNull());
    dataHandler!('hi');
    expect(writeCalls.length).toBe(1);
    expect(writeCalls[0].terminalId).toBe('term_e');
    // 'hi' encoded as UTF-8 bytes.
    expect(writeCalls[0].bytes).toEqual([0x68, 0x69]);
  });

  it('renders the exited marker for a null exit code as "unknown"', async () => {
    render(TerminalPane, { props: { workspaceId: 'ws_f', terminalId: 'term_f' } });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    lastChannel!.onmessage!({ kind: 'exited', code: null });
    await waitFor(() => {
      expect(writelns.some((l) => l.includes('exited with code unknown'))).toBe(true);
    });
  });

  it('writes a "[failed to start shell]" marker when both reattach and spawn reject', async () => {
    reattachBehavior = 'reject';
    spawnBehavior = 'reject';
    render(TerminalPane, { props: { workspaceId: 'ws_g', terminalId: 'term_g' } });
    await waitFor(() => {
      expect(writelns.some((l) => l.includes('failed to start shell'))).toBe(true);
    });
  });

  it('ResizeObserver callback fires fit + api.terminal.resize with terminalId', async () => {
    render(TerminalPane, { props: { workspaceId: 'ws_h', terminalId: 'term_h' } });
    await waitFor(() => expect(resizeCb).not.toBeNull());
    resizeCb!();
    await waitFor(() => {
      expect(resizeCalls.length).toBeGreaterThan(0);
    });
    expect(resizeCalls[0]).toMatchObject({ terminalId: 'term_h', cols: 80, rows: 24 });
  });

  it('initial fit() throwing in a no-layout runtime is swallowed (component still mounts)', async () => {
    fitThrows = true;
    const { findByTestId } = render(TerminalPane, {
      props: { workspaceId: 'ws_i', terminalId: 'term_i' },
    });
    // Even though fit.fit() throws on every call, the component still
    // wires up the channel + reaches the spawn path.
    expect(await findByTestId('terminal-container')).toBeTruthy();
    await waitFor(() => expect(lastChannel).not.toBeNull());
  });

  it('ResizeObserver callback swallows fit() throws without crashing', async () => {
    render(TerminalPane, { props: { workspaceId: 'ws_j', terminalId: 'term_j' } });
    await waitFor(() => expect(resizeCb).not.toBeNull());
    fitThrows = true;
    // Should not throw — component must catch and skip the resize call.
    expect(() => resizeCb!()).not.toThrow();
  });

  it('falls back through the 500ms safety timeout when the runtime never fires ResizeObserver', async () => {
    // Override the global with a no-op RO so waitForLayout's safety
    // timeout is the only path that resolves the promise. Then advance
    // fake timers past 500ms and assert the spawn flow still kicks in.
    class SilentResizeObserver {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    }
    vi.stubGlobal('ResizeObserver', SilentResizeObserver);
    vi.useFakeTimers();
    try {
      render(TerminalPane, { props: { workspaceId: 'ws_safety', terminalId: 'term_safety' } });
      // Advance through the dynamic-import microtasks first…
      await vi.advanceTimersByTimeAsync(0);
      // …then fire the safety timeout.
      await vi.advanceTimersByTimeAsync(600);
      // After the timeout resolves, the rest of onMount runs and
      // reattach (rejects) → spawn captures the channel.
      await vi.runAllTimersAsync();
      expect(lastChannel).not.toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('restores a stored snapshot to the xterm on mount instead of the ready marker', async () => {
    terminalSnapshots.set('term_restore', 'RESTORED-GRID');
    render(TerminalPane, { props: { workspaceId: 'ws_r', terminalId: 'term_restore' } });
    await waitFor(() => expect(stringWrites).toContain('RESTORED-GRID'));
    // one-shot: consumed from the stash
    expect(terminalSnapshots.take('term_restore')).toBeUndefined();
    // fresh-spawn marker is NOT shown when restoring
    expect(writelns.some((l) => l.includes('xterm ready'))).toBe(false);
  });

  it('writes the ready marker (not a restore) when no snapshot is stored', async () => {
    render(TerminalPane, { props: { workspaceId: 'ws_fresh', terminalId: 'term_fresh' } });
    await waitFor(() => expect(writelns.some((l) => l.includes('xterm ready'))).toBe(true));
  });

  it('serializes the screen into the stash on destroy', async () => {
    serializeReturn = 'SERIALIZED-SCREEN';
    const { unmount } = render(TerminalPane, {
      props: { workspaceId: 'ws_s', terminalId: 'term_save' },
    });
    await waitFor(() => expect(lastChannel).not.toBeNull()); // ensure onMount finished
    unmount(); // triggers onDestroy
    expect(terminalSnapshots.take('term_save')).toBe('SERIALIZED-SCREEN');
  });
});

describe('Terminal container', () => {
  it('renders terminal-view with tab bar and one pane when a tab exists', async () => {
    // Pre-populate a tab so the container renders the tab bar + pane immediately.
    // (The real $effect auto-create works at runtime; in jsdom the plain-Map mock
    // is not reactive enough to trigger a re-render, so we seed it here.)
    terminalTabs.add('ws_container_a');
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_container_a' } });
    expect(await findByTestId('terminal-view')).toBeTruthy();
    expect(await findByTestId('terminal-tab-bar')).toBeTruthy();
    expect(await findByTestId('terminal-pane-host')).toBeTruthy();
  });

  it('renders an empty-state button when no tabs exist', async () => {
    // Do NOT pre-add tabs — the empty branch should render "+ New terminal".
    const { findByTestId, getByRole } = render(Terminal, {
      props: { workspaceId: 'ws_container_empty' },
    });
    expect(await findByTestId('terminal-view')).toBeTruthy();
    expect(getByRole('button', { name: /New terminal/i })).toBeTruthy();
  });

  it('renders all pane hosts without {#if}-gating (display-toggled only)', async () => {
    // Pre-populate two tabs so the container renders both pane-host divs.
    terminalTabs.add('ws_container_b');
    terminalTabs.add('ws_container_b');
    const { getAllByTestId } = render(Terminal, { props: { workspaceId: 'ws_container_b' } });
    await waitFor(() => {
      const hosts = getAllByTestId('terminal-pane-host');
      // Both panes exist in DOM; only one is visible.
      expect(hosts.length).toBe(2);
    });
  });

  it('drops the stored snapshot when a terminal tab is closed', async () => {
    // Seed a tab, then stash a snapshot for it. Closing the tab via the UI
    // must drop the stash so a re-created terminal can't restore a stale grid.
    terminalTabs.add('ws_container_close');
    const tabId = terminalTabs.list('ws_container_close')[0].id;
    terminalSnapshots.set(tabId, 'STALE-GRID');
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_container_close' } });
    const closeBtn = await findByTestId('terminal-tab-close');
    await fireEvent.click(closeBtn);
    expect(terminalSnapshots.take(tabId)).toBeUndefined();
  });
});
