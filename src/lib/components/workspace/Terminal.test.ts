import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import type { TerminalChunk } from '$lib/types';

// Capture the Channel handed to terminal_spawn / terminal_reattach so
// tests can simulate streaming chunks. xterm.js itself is mocked
// because it needs a real DOM with measurement APIs that jsdom lacks.
let lastChannel: { onmessage?: (chunk: TerminalChunk) => void } | null = null;
let reattachBehavior: 'success' | 'reject' = 'reject';
let spawnBehavior: 'success' | 'reject' = 'success';
const writeCalls: { workspaceId: string; bytes: number[] }[] = [];
const resizeCalls: { workspaceId: string; cols: number; rows: number }[] = [];

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
          workspaceId: args.workspaceId as string,
          bytes: args.bytes as number[],
        });
        return undefined;
      }
      if (cmd === 'terminal_resize' && args) {
        resizeCalls.push({
          workspaceId: args.workspaceId as string,
          cols: args.cols as number,
          rows: args.rows as number,
        });
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
const writelns: string[] = [];
let dataHandler: ((data: string) => void) | null = null;

vi.mock('@xterm/xterm', () => {
  class MockTerminal {
    cols = 80;
    rows = 24;
    open = vi.fn();
    write = (data: string | Uint8Array) => {
      if (data instanceof Uint8Array) writes.push(data);
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
// callback so tests can fire a fake resize and assert the component
// reflows + calls api.terminal.resize.
let resizeCb: (() => void) | null = null;
class CapturingResizeObserver {
  constructor(cb: () => void) {
    resizeCb = cb;
  }
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
vi.stubGlobal('ResizeObserver', CapturingResizeObserver);

import Terminal from './Terminal.svelte';

beforeEach(() => {
  lastChannel = null;
  writeCalls.length = 0;
  resizeCalls.length = 0;
  writes.length = 0;
  writelns.length = 0;
  dataHandler = null;
  resizeCb = null;
  reattachBehavior = 'reject';
  spawnBehavior = 'success';
  fitThrows = false;
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('Terminal', () => {
  it('mounts and falls back to spawn when reattach rejects', async () => {
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_a' } });
    expect(await findByTestId('terminal-view')).toBeTruthy();
    await waitFor(() => expect(lastChannel).not.toBeNull());
    // Status flips from "attaching…" to "live" once spawn resolves.
    await waitFor(async () => {
      const status = await findByTestId('terminal-status');
      expect(status.textContent).toMatch(/live/);
    });
  });

  it('uses reattach when an existing session is available', async () => {
    reattachBehavior = 'success';
    const { invoke } = await import('@tauri-apps/api/core');
    render(Terminal, { props: { workspaceId: 'ws_b' } });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    const calls = vi.mocked(invoke).mock.calls.map((c) => c[0]);
    expect(calls).toContain('terminal_reattach');
    expect(calls).not.toContain('terminal_spawn');
  });

  it('streams TerminalChunk::Bytes into the xterm buffer', async () => {
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_c' } });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    lastChannel!.onmessage!({ kind: 'bytes', bytes: [104, 105] });
    expect(writes.length).toBe(1);
    expect(Array.from(writes[0])).toEqual([104, 105]);
    void findByTestId; // narrow lint: keep within the test scope
  });

  it('renders an exited marker and flips status to "exited"', async () => {
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_d' } });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    lastChannel!.onmessage!({ kind: 'exited', code: 0 });
    await waitFor(async () => {
      const status = await findByTestId('terminal-status');
      expect(status.textContent).toMatch(/exited/);
    });
    expect(writelns.some((l) => l.includes('exited with code 0'))).toBe(true);
  });

  it('forwards onData keystrokes through terminal_write', async () => {
    render(Terminal, { props: { workspaceId: 'ws_e' } });
    await waitFor(() => expect(dataHandler).not.toBeNull());
    dataHandler!('hi');
    expect(writeCalls.length).toBe(1);
    expect(writeCalls[0].workspaceId).toBe('ws_e');
    // 'hi' encoded as UTF-8 bytes.
    expect(writeCalls[0].bytes).toEqual([0x68, 0x69]);
  });

  it('renders the exited marker for a null exit code as "unknown"', async () => {
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_f' } });
    await waitFor(() => expect(lastChannel).not.toBeNull());
    lastChannel!.onmessage!({ kind: 'exited', code: null });
    await waitFor(() => {
      expect(writelns.some((l) => l.includes('exited with code unknown'))).toBe(true);
    });
    expect((await findByTestId('terminal-status')).textContent).toMatch(/exited/);
  });

  it('writes a "[failed to start shell]" marker when both reattach and spawn reject', async () => {
    reattachBehavior = 'reject';
    spawnBehavior = 'reject';
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_g' } });
    await waitFor(() => {
      expect(writelns.some((l) => l.includes('failed to start shell'))).toBe(true);
    });
    // Status flips from "attaching…" to "exited" once the spawn-failure
    // branch sets `exited = true`.
    await waitFor(async () => {
      const status = await findByTestId('terminal-status');
      expect(status.textContent).toMatch(/exited/);
    });
  });

  it('ResizeObserver callback fires fit + api.terminal.resize with current cols/rows', async () => {
    render(Terminal, { props: { workspaceId: 'ws_h' } });
    await waitFor(() => expect(resizeCb).not.toBeNull());
    resizeCb!();
    await waitFor(() => {
      expect(resizeCalls.length).toBeGreaterThan(0);
    });
    expect(resizeCalls[0]).toMatchObject({ workspaceId: 'ws_h', cols: 80, rows: 24 });
  });

  it('initial fit() throwing in a no-layout runtime is swallowed (component still mounts)', async () => {
    fitThrows = true;
    const { findByTestId } = render(Terminal, { props: { workspaceId: 'ws_i' } });
    // Even though fit.fit() throws on every call, the component still
    // wires up the channel + reaches the spawn path.
    expect(await findByTestId('terminal-view')).toBeTruthy();
    await waitFor(() => expect(lastChannel).not.toBeNull());
  });

  it('ResizeObserver callback swallows fit() throws without crashing', async () => {
    render(Terminal, { props: { workspaceId: 'ws_j' } });
    await waitFor(() => expect(resizeCb).not.toBeNull());
    fitThrows = true;
    // Should not throw — component must catch and skip the resize call.
    expect(() => resizeCb!()).not.toThrow();
  });
});
