<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity';
  import { untrack, tick } from 'svelte';
  import { api } from '$lib/ipc';
  import { workspaceTabs } from '$lib/stores/workspace-tabs.svelte';
  import type { FileEntry } from '$lib/types';

  interface Props {
    workspaceId: string;
    /** Called with the relative path when the user clicks a file. The
     *  editor isn't here in 2a — for now this is wired to the Search
     *  modal "click hit → highlight in tree" flow. */
    onOpen?: (path: string) => void;
    /** Currently-highlighted file (e.g. from a search hit). */
    selectedPath?: string | null;
  }

  const { workspaceId, onOpen, selectedPath = null }: Props = $props();

  /** Children indexed by parent path. '' = worktree root. */

  const children: Map<string, FileEntry[]> = new SvelteMap();
  /** Per-path load error message. */

  const errors: Map<string, string> = new SvelteMap();
  /** Paths currently being loaded — used to render a per-row spinner. */

  const loading: Map<string, boolean> = new SvelteMap();

  const expanded = $derived(workspaceTabs.expanded(workspaceId));

  async function loadChildren(path: string): Promise<void> {
    if (loading.get(path)) return;
    loading.set(path, true);
    errors.delete(path);
    try {
      const list = await api.workspace.files(workspaceId, path || undefined);
      children.set(path, list);
    } catch (err) {
      errors.set(path, String(err));
    } finally {
      loading.set(path, false);
    }
  }

  $effect(() => {
    // Track only the workspace id — clearing + reloading on workspace
    // switch must not depend on the maps we're writing to, or each write
    // triggers a re-run loop (Svelte 5: read+write of the same reactive
    // store inside an effect = effect_update_depth_exceeded).
    void workspaceId;
    untrack(() => {
      children.clear();
      errors.clear();
      loading.clear();
      void loadChildren('');
    });
  });

  // Reveal-on-select: when `selectedPath` flips to a non-null value
  // (search hit, future jump-from-anywhere flows), expand every ancestor
  // directory top-down, lazy-load any children that aren't cached yet,
  // and scroll the matching row into view. Without this the FileBrowser
  // would silently hide rows for any deeply-nested file because the
  // ancestor `<details>` containers were never opened.
  $effect(() => {
    const target = selectedPath;
    if (!target) return;
    untrack(() => {
      void revealPath(target);
    });
  });

  async function revealPath(target: string): Promise<void> {
    // Compute every ancestor directory path top-down. For
    // `app/Http/Controllers/web.php` → ['app', 'app/Http',
    // 'app/Http/Controllers']. The file itself isn't in the list — only
    // the dirs that need to be expanded.
    const parts = target.split('/').filter(Boolean);
    const ancestors: string[] = [];
    for (let i = 1; i < parts.length; i++) {
      ancestors.push(parts.slice(0, i).join('/'));
    }
    const expandedSet = workspaceTabs.expanded(workspaceId);
    for (const ancestor of ancestors) {
      expandedSet.add(ancestor);
      if (!children.has(ancestor)) {
        await loadChildren(ancestor);
      }
    }
    // After all loads, give Svelte a tick to render the now-deeper tree
    // and then scroll the file row into view. `block: 'nearest'` keeps
    // surrounding context visible if the row is already on-screen.
    await tick();
    const row = document.querySelector<HTMLElement>(
      `[data-testid="file-row"][data-path="${cssEscape(target)}"]`
    );
    // jsdom and older browsers don't ship scrollIntoView — fall back
    // gracefully so reveal still expands the tree even if the auto-scroll
    // half no-ops.
    if (row && typeof row.scrollIntoView === 'function') {
      row.scrollIntoView({ block: 'nearest' });
    }
  }

  function cssEscape(s: string): string {
    // CSS.escape isn't on every test runtime; do the bare minimum here so
    // attribute selectors stay valid for the path strings we generate
    // (alphanumerics + `/` + `.` + `-` + `_`).
    return s.replace(/["\\]/g, '\\$&');
  }

  async function handleDirClick(path: string): Promise<void> {
    const state = workspaceTabs.toggleExpanded(workspaceId, path);
    if (state === 'expanded' && !children.has(path)) {
      await loadChildren(path);
    }
  }

  function handleFileClick(path: string): void {
    onOpen?.(path);
  }
</script>

<div class="h-full overflow-auto text-sm bg-[var(--bg-base)]" data-testid="file-browser">
  {#if errors.has('')}
    <div class="px-3 py-2 text-red-300" data-testid="file-browser-root-error">
      {errors.get('')}
    </div>
  {:else if loading.get('') && !children.has('')}
    <div class="px-3 py-2 text-[var(--text-muted)]" data-testid="file-browser-loading">
      Loading…
    </div>
  {:else if children.get('')?.length === 0}
    <div class="px-3 py-2 text-[var(--text-muted)]" data-testid="file-browser-empty">
      Worktree is empty.
    </div>
  {:else}
    <ul role="tree" class="py-1">
      {#each children.get('') ?? [] as entry (entry.path)}
        {@render row(entry, 0)}
      {/each}
    </ul>
  {/if}
</div>

{#snippet row(entry: FileEntry, depth: number)}
  <li
    role="treeitem"
    aria-expanded={entry.kind === 'dir' ? expanded.has(entry.path) : undefined}
    aria-selected={entry.path === selectedPath}
    data-testid="file-row"
    data-path={entry.path}
    data-kind={entry.kind}
    class="select-none"
  >
    <button
      type="button"
      class="flex w-full items-center gap-1 px-2 py-0.5 text-left hover:bg-[var(--bg-card)] {entry.path ===
      selectedPath
        ? 'bg-[var(--accent)]/15 text-[var(--text-primary)]'
        : 'text-[var(--text-secondary)]'}"
      style:padding-left="{8 + depth * 16}px"
      onclick={() =>
        entry.kind === 'dir' ? handleDirClick(entry.path) : handleFileClick(entry.path)}
    >
      <span class="w-3 text-center text-[var(--text-muted)]" aria-hidden="true">
        {#if entry.kind === 'dir'}
          {expanded.has(entry.path) ? '▾' : '▸'}
        {:else}
          ·
        {/if}
      </span>
      <span class="truncate">{entry.name}</span>
    </button>
    {#if entry.kind === 'dir' && expanded.has(entry.path)}
      {#if errors.has(entry.path)}
        <div
          class="text-red-300 text-xs"
          style:padding-left="{8 + (depth + 1) * 16}px"
          data-testid="file-row-error"
          data-path={entry.path}
        >
          {errors.get(entry.path)}
        </div>
      {:else if loading.get(entry.path)}
        <div class="text-[var(--text-muted)] text-xs" style:padding-left="{8 + (depth + 1) * 16}px">
          Loading…
        </div>
      {:else}
        <ul role="group">
          {#each children.get(entry.path) ?? [] as child (child.path)}
            {@render row(child, depth + 1)}
          {/each}
        </ul>
      {/if}
    {/if}
  </li>
{/snippet}
