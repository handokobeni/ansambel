<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { Channel } from '@tauri-apps/api/core';
  import { api } from '$lib/ipc';
  import type { SearchHit, SearchMode } from '$lib/types';

  interface Props {
    open: boolean;
    workspaceId: string | null;
    /** Initial mode the modal opens in. Caller flips this to 'content' for
     *  Ctrl+Shift+F. */
    initialMode?: SearchMode;
    onClose: () => void;
    /** Called with the relative path (and optional 1-based line number for
     *  content hits) when the user clicks a result. */
    onJump: (path: string, line?: number) => void;
  }

  const { open, workspaceId, initialMode = 'filename', onClose, onJump }: Props = $props();

  // Initialized to a literal default; the $effect below syncs it to the
  // latest `initialMode` whenever the modal flips from closed → open, so
  // the parent can change Ctrl+P/Ctrl+Shift+F intent between opens.
  let mode = $state<SearchMode>('filename');
  let query = $state('');
  let hits = $state<SearchHit[]>([]);
  let loading = $state(false);
  let unavailable = $state<string | null>(null);
  let inputEl: HTMLInputElement | undefined = $state();
  let generation = 0;

  // When the modal flips from closed → open, reset state and focus the
  // input. Caller decides initialMode each time so we mirror it.
  $effect(() => {
    if (open) {
      mode = initialMode;
      hits = [];
      unavailable = null;
      // Wait for the input to render before focusing — without `tick` the
      // ref is still undefined.
      tick().then(() => inputEl?.focus());
    }
  });

  function close() {
    generation += 1; // drop in-flight chunks
    onClose();
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  }

  async function runSearch() {
    if (!workspaceId || !query.trim()) return;
    generation += 1;
    const myGen = generation;
    hits = [];
    unavailable = null;
    loading = true;
    const channel = new Channel<SearchHit>();
    channel.onmessage = (hit: SearchHit) => {
      if (myGen !== generation) return;
      switch (hit.kind) {
        case 'eof':
          loading = false;
          return;
        case 'ripgrep_unavailable':
          unavailable = hit.reason;
          return;
        default:
          hits = [...hits, hit];
      }
    };
    try {
      await api.workspace.search(workspaceId, query, mode, channel);
    } catch (err) {
      if (myGen === generation) {
        unavailable = String(err);
        loading = false;
      }
    }
  }

  function handleHitClick(hit: SearchHit) {
    if (hit.kind === 'filename') onJump(hit.path);
    else if (hit.kind === 'content') onJump(hit.path, hit.line_number);
    close();
  }

  onDestroy(() => {
    generation += 1;
  });
</script>

{#if open}
  <div
    role="dialog"
    aria-modal="true"
    aria-label="Search worktree"
    data-testid="search-modal"
    class="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-[var(--overlay-bg)] backdrop-blur-sm"
    onkeydown={handleKey}
    tabindex="-1"
  >
    <button
      type="button"
      class="fixed inset-0 -z-10 cursor-default"
      aria-label="Close search"
      onclick={close}
    ></button>
    <div
      class="w-[640px] max-w-[90vw] rounded-lg border border-[var(--border)] bg-[var(--bg-base)] shadow-2xl flex flex-col overflow-hidden"
    >
      <div
        role="tablist"
        class="flex border-b border-[var(--border)] text-xs text-[var(--text-secondary)]"
      >
        <button
          type="button"
          role="tab"
          data-testid="search-mode-filename"
          aria-selected={mode === 'filename'}
          class="px-3 py-1.5 border-b-2 {mode === 'filename'
            ? 'border-[var(--accent)] text-[var(--text-primary)]'
            : 'border-transparent'}"
          onclick={() => (mode = 'filename')}
        >
          Files <span class="opacity-60 ml-1">⌃P</span>
        </button>
        <button
          type="button"
          role="tab"
          data-testid="search-mode-content"
          aria-selected={mode === 'content'}
          class="px-3 py-1.5 border-b-2 {mode === 'content'
            ? 'border-[var(--accent)] text-[var(--text-primary)]'
            : 'border-transparent'}"
          onclick={() => (mode = 'content')}
        >
          Content <span class="opacity-60 ml-1">⌃⇧F</span>
        </button>
        <button
          type="button"
          aria-label="Close"
          data-testid="search-close"
          class="ml-auto px-3 py-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
          onclick={close}
        >
          ✕
        </button>
      </div>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void runSearch();
        }}
      >
        <input
          bind:this={inputEl}
          bind:value={query}
          data-testid="search-input"
          type="text"
          placeholder={mode === 'filename' ? 'Search files by name…' : 'Search file contents…'}
          class="w-full px-3 py-2 bg-transparent text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none border-b border-[var(--border)]"
          autocomplete="off"
          spellcheck="false"
        />
      </form>
      {#if unavailable}
        <div
          class="px-3 py-2 text-xs text-amber-300 bg-amber-500/10 border-b border-amber-500/30"
          data-testid="search-unavailable"
        >
          {unavailable}
        </div>
      {/if}
      <div class="max-h-[60vh] overflow-auto" data-testid="search-results">
        {#if loading && hits.length === 0}
          <div class="px-3 py-2 text-[var(--text-muted)] text-xs" data-testid="search-loading">
            Searching…
          </div>
        {:else if !loading && hits.length === 0 && query}
          <div class="px-3 py-2 text-[var(--text-muted)] text-xs" data-testid="search-empty">
            No results.
          </div>
        {:else}
          <ul role="listbox">
            {#each hits as hit, i (i)}
              <li role="option" aria-selected="false" data-testid="search-hit">
                <button
                  type="button"
                  class="w-full text-left px-3 py-1.5 hover:bg-[var(--bg-card)] flex items-baseline gap-3"
                  onclick={() => handleHitClick(hit)}
                >
                  {#if hit.kind === 'filename'}
                    <span class="text-[var(--text-primary)] text-sm truncate">{hit.path}</span>
                  {:else if hit.kind === 'content'}
                    <span class="text-[var(--text-primary)] text-sm truncate">{hit.path}</span>
                    <span class="text-[var(--text-muted)] text-xs">:{hit.line_number}</span>
                    <span class="text-[var(--text-secondary)] text-xs truncate font-mono">
                      {hit.line_text}
                    </span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  </div>
{/if}
