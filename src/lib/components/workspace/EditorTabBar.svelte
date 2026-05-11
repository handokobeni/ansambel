<script lang="ts">
  import { editorTabs } from '$lib/stores/editor-tabs.svelte';

  interface Props {
    workspaceId: string;
  }

  const { workspaceId }: Props = $props();

  const open = $derived(editorTabs.open(workspaceId));
  const active = $derived(editorTabs.active(workspaceId));

  function close(e: MouseEvent, path: string, dirty: boolean): void {
    // Stop propagation so the close ✕ doesn't also activate the tab
    // its host button is associated with.
    e.stopPropagation();
    if (dirty) {
      const ok = window.confirm(`${path} has unsaved changes — close anyway?`);
      if (!ok) return;
    }
    editorTabs.closeFile(workspaceId, path);
  }

  function basename(path: string): string {
    const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return idx >= 0 ? path.slice(idx + 1) : path;
  }
</script>

{#if open.length > 0}
  <div
    role="tablist"
    aria-label="Open files"
    class="flex items-stretch gap-px border-b border-[var(--border)] bg-[var(--bg-sidebar)] text-xs overflow-x-auto"
    data-testid="editor-tab-bar"
  >
    {#each open as file (file.path)}
      <button
        type="button"
        role="tab"
        aria-selected={active === file.path}
        data-testid="editor-tab"
        data-path={file.path}
        title={file.path}
        onclick={() => editorTabs.setActive(workspaceId, file.path)}
        class="px-3 py-1.5 flex items-center gap-2 border-b-2 max-w-[200px] {active === file.path
          ? 'border-b-[var(--accent)] text-[var(--text-primary)]'
          : 'border-transparent text-[var(--text-secondary)] hover:bg-[var(--bg-card)]'}"
      >
        <span class="truncate">{basename(file.path)}</span>
        {#if file.dirty}
          <span
            class="text-[var(--accent)]"
            aria-label="Unsaved changes"
            data-testid="editor-tab-dirty">•</span
          >
        {/if}
        <span
          role="button"
          tabindex="0"
          aria-label="Close {file.path}"
          data-testid="editor-tab-close"
          onclick={(e) => close(e, file.path, file.dirty)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              close(e as unknown as MouseEvent, file.path, file.dirty);
            }
          }}
          class="ml-1 px-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
        >
          ✕
        </span>
      </button>
    {/each}
  </div>
{/if}
