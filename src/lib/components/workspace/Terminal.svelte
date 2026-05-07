<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { Channel } from '@tauri-apps/api/core';
  import { api } from '$lib/ipc';
  import type { TerminalChunk } from '$lib/types';

  interface Props {
    workspaceId: string;
  }

  const { workspaceId }: Props = $props();

  let containerRef: HTMLDivElement | undefined = $state();
  // xterm types pulled at runtime so the hard ESM import isn't a
  // dependency wall for jsdom unit tests. The dynamic-import shape
  // also lets us swap the `Terminal` constructor in tests.
  type XTerm = import('@xterm/xterm').Terminal;
  let term: XTerm | undefined;
  let exited = $state(false);
  let attaching = $state(true);
  let unmounted = false;
  let observer: ResizeObserver | undefined;

  onMount(async () => {
    if (!containerRef) return;
    const { Terminal: XTermCtor } = await import('@xterm/xterm');
    const { FitAddon } = await import('@xterm/addon-fit');
    if (unmounted) return;

    const fit = new FitAddon();
    term = new XTermCtor({
      cursorBlink: true,
      fontFamily: 'ui-monospace, monospace',
      fontSize: 12,
      // xterm will request its CSS at construction time but our app
      // ships the @xterm/xterm/css file via the import below.
    });
    term.loadAddon(fit);
    term.open(containerRef);
    // Wait one tick for the container to settle before fitting —
    // otherwise xterm renders zero-row.
    await tick();
    // Component may have been destroyed while we awaited; bail before
    // touching term. onDestroy nulls term out, so without this guard
    // we'd crash on `undefined.onData(...)`.
    if (unmounted || !term) return;
    try {
      fit.fit();
    } catch {
      // jsdom-style runtimes without layout: skip the initial fit.
    }

    // Forward keystrokes (and pasted content) into the backend's PTY.
    term.onData((data: string) => {
      const bytes = Array.from(new TextEncoder().encode(data));
      void api.terminal.write(workspaceId, bytes);
    });

    // Keep the PTY's understanding of dimensions in sync with the
    // visible terminal. ResizeObserver fires when the surrounding tab
    // panel flips display from none → block. Stored at module scope
    // so onDestroy can disconnect it.
    observer = new ResizeObserver(() => {
      try {
        fit.fit();
        if (term) {
          void api.terminal.resize(workspaceId, term.cols, term.rows);
        }
      } catch {
        // No layout — skip.
      }
    });
    observer.observe(containerRef);

    const channel = new Channel<TerminalChunk>();
    channel.onmessage = (chunk: TerminalChunk) => {
      if (!term) return;
      if (chunk.kind === 'bytes') {
        // xterm.js's write API accepts both string and Uint8Array. We
        // get a JSON number array from the IPC layer; normalize.
        term.write(new Uint8Array(chunk.bytes));
      } else if (chunk.kind === 'exited') {
        exited = true;
        const codeStr = chunk.code === null ? 'unknown' : String(chunk.code);
        term.writeln(`\r\n[process exited with code ${codeStr}]`);
      }
    };

    // Try reattach first — the backend may already have a session for
    // this workspace from a prior mount. Spawn fresh on rejection.
    try {
      await api.terminal.reattach(workspaceId, channel);
    } catch {
      try {
        await api.terminal.spawn(workspaceId, channel, term.cols, term.rows);
      } catch (err) {
        if (term) {
          term.writeln(`\r\n[failed to start shell: ${String(err)}]`);
          exited = true;
        }
      }
    }
    attaching = false;
  });

  onDestroy(() => {
    unmounted = true;
    observer?.disconnect();
    term?.dispose();
    term = undefined;
  });
</script>

<div class="flex flex-col h-full bg-[var(--bg-base)]" data-testid="terminal-view">
  <div
    class="flex items-center justify-between px-3 py-1.5 border-b border-[var(--border)] text-xs text-[var(--text-secondary)]"
  >
    <span>Terminal</span>
    <span class="text-[var(--text-muted)]" data-testid="terminal-status">
      {#if attaching}attaching…{:else if exited}exited{:else}live{/if}
    </span>
  </div>
  <div
    bind:this={containerRef}
    data-testid="terminal-container"
    class="flex-1 overflow-hidden"
  ></div>
</div>
