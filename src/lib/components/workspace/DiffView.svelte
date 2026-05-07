<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Channel } from '@tauri-apps/api/core';
  import { api } from '$lib/ipc';
  import { parseUnifiedDiff } from '$lib/diff';
  import type { ParsedDiff as Parsed } from '$lib/diff';
  import type { DiffChunk } from '$lib/types';

  interface Props {
    workspaceId: string;
  }

  const { workspaceId }: Props = $props();

  type LoadState =
    | { kind: 'loading' }
    | { kind: 'loaded'; diff: Parsed }
    | { kind: 'error'; message: string };

  let state = $state<LoadState>({ kind: 'loading' });
  let buffer = '';
  /** Generation id — bumped on every load(). Stale-channel chunks are
   *  rejected so a Refresh-mid-stream click can't have its in-flight chunks
   *  blend into the new run. */
  let generation = 0;

  function load() {
    generation += 1;
    const myGen = generation;
    buffer = '';
    state = { kind: 'loading' };
    const channel = new Channel<DiffChunk>();
    channel.onmessage = (chunk: DiffChunk) => {
      if (myGen !== generation) return;
      switch (chunk.kind) {
        case 'text':
          buffer += chunk.text;
          break;
        case 'error':
          state = { kind: 'error', message: chunk.message };
          break;
        case 'eof':
          state = { kind: 'loaded', diff: parseUnifiedDiff(buffer) };
          break;
      }
    };
    api.workspace.diff(workspaceId, channel).catch((err) => {
      if (myGen !== generation) return;
      state = { kind: 'error', message: String(err) };
    });
  }

  $effect(() => {
    void workspaceId;
    load();
  });

  onDestroy(() => {
    // Bump the generation so any in-flight chunk is rejected after unmount.
    generation += 1;
  });

  function lineBg(kind: 'add' | 'del' | 'ctx' | 'meta'): string {
    if (kind === 'add') return 'bg-green-500/10 text-green-200';
    if (kind === 'del') return 'bg-red-500/10 text-red-200';
    if (kind === 'meta') return 'text-[var(--text-muted)] italic';
    return 'text-[var(--text-secondary)]';
  }

  function lineSign(kind: 'add' | 'del' | 'ctx' | 'meta'): string {
    if (kind === 'add') return '+';
    if (kind === 'del') return '-';
    if (kind === 'meta') return ' ';
    return ' ';
  }
</script>

<div class="flex flex-col h-full bg-[var(--bg-base)]" data-testid="diff-view">
  <div
    class="flex items-center justify-between px-3 py-1.5 border-b border-[var(--border)] text-xs text-[var(--text-secondary)]"
  >
    <span>Diff vs HEAD</span>
    <button
      type="button"
      onclick={load}
      data-testid="diff-refresh"
      class="px-2 py-0.5 rounded border border-[var(--border)] hover:bg-[var(--bg-card)] text-[var(--text-primary)]"
    >
      Refresh
    </button>
  </div>

  <div class="flex-1 overflow-auto font-mono text-xs">
    {#if state.kind === 'loading'}
      <div class="p-4 text-[var(--text-muted)]" data-testid="diff-loading">Loading diff…</div>
    {:else if state.kind === 'error'}
      <div
        class="p-4 text-red-300 bg-red-500/10 border-b border-red-500/30"
        data-testid="diff-error"
      >
        {state.message}
      </div>
    {:else if state.diff.files.length === 0}
      <div class="p-4 text-[var(--text-muted)]" data-testid="diff-empty">
        No uncommitted changes.
      </div>
    {:else}
      {#each state.diff.files as file (file.path)}
        <div data-testid="diff-file" data-path={file.path} class="border-b border-[var(--border)]">
          <div
            class="px-3 py-1 sticky top-0 bg-[var(--bg-sidebar)] border-b border-[var(--border)] text-[var(--text-primary)] font-semibold"
          >
            {file.path}
          </div>
          {#if file.isBinary}
            <div class="px-3 py-2 text-[var(--text-muted)]">Binary file — diff not shown.</div>
          {:else}
            {#each file.hunks as hunk (hunk.header)}
              <div class="px-3 py-1 text-[var(--accent)] bg-[var(--bg-card)]">
                {hunk.header}
              </div>
              {#each hunk.lines as line, i (i)}
                <div
                  class="flex {lineBg(line.kind)}"
                  data-line-kind={line.kind}
                  data-testid="diff-line"
                >
                  <span class="w-4 text-center select-none opacity-60" aria-hidden="true">
                    {lineSign(line.kind)}
                  </span>
                  <span class="flex-1 whitespace-pre">{line.text}</span>
                </div>
              {/each}
            {/each}
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>
