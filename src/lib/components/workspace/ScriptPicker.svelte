<script lang="ts">
  import { untrack } from 'svelte';
  import { Channel } from '@tauri-apps/api/core';
  import { api } from '$lib/ipc';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type { RepoScript, TerminalChunk } from '$lib/types';

  interface Props {
    repoId: string;
    workspaceId: string;
    /** Optional: a TerminalChunk consumer (the Terminal component's
     *  channel handler). When provided, script_run uses a fresh Channel
     *  and forwards every chunk into this callback so the same xterm
     *  buffer renders both interactive shell and script output. The
     *  Terminal component owns the consumer; ScriptPicker is just a
     *  trigger surface. */
    onChunk?: (chunk: TerminalChunk) => void;
  }

  const { repoId, workspaceId, onChunk }: Props = $props();

  let scripts: RepoScript[] = $state([]);
  let loadError: string | null = $state(null);
  let loading = $state(false);

  $effect(() => {
    void repoId;
    untrack(() => {
      void load();
    });
  });

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const list = await api.script.list(repoId);
      scripts = Array.isArray(list) ? list : [];
    } catch (err) {
      loadError = String(err);
      scripts = [];
    } finally {
      loading = false;
    }
  }

  async function run(scriptId: string): Promise<void> {
    const channel = new Channel<TerminalChunk>();
    if (onChunk) channel.onmessage = onChunk;
    try {
      await api.script.run(workspaceId, scriptId, channel);
    } catch (err) {
      addToast(`Run failed: ${String(err)}`, 'error');
    }
  }
</script>

<div
  class="flex items-center gap-2 px-3 py-1.5 border-b border-[var(--border)] bg-[var(--bg-sidebar)] text-xs"
  data-testid="script-picker"
>
  <span class="text-[var(--text-muted)]">Scripts:</span>
  {#if loading}
    <span class="text-[var(--text-muted)]" data-testid="script-picker-loading">loading…</span>
  {:else if loadError}
    <span class="text-[var(--accent-error)]" data-testid="script-picker-error">{loadError}</span>
  {:else if scripts.length === 0}
    <span class="text-[var(--text-muted)]" data-testid="script-picker-empty">
      No scripts configured.
    </span>
  {:else}
    <div class="flex items-center gap-1">
      {#each scripts as s (s.id)}
        <button
          type="button"
          data-testid="script-picker-item"
          title={s.command}
          onclick={() => run(s.id)}
          class="px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--bg-card)] hover:text-[var(--text-primary)]"
        >
          {s.name}
        </button>
      {/each}
    </div>
  {/if}
</div>
