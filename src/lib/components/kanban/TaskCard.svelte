<!-- src/lib/components/kanban/TaskCard.svelte -->
<script lang="ts">
  import type { Task } from '$lib/types';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { modeStore } from '$lib/stores/mode.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';
  import { tooltip } from '$lib/actions';
  import LinkWorkspacePicker from './LinkWorkspacePicker.svelte';
  import UnlinkConfirmModal from './UnlinkConfirmModal.svelte';

  const { task, onRemove }: { task: Task; onRemove: (id: string) => void } = $props();

  let menuOpen = $state(false);
  let linkPickerOpen = $state(false);
  let unlinkModalOpen = $state(false);
  let unlinkModalTitle = $state('');

  function toggleMenu(e: MouseEvent) {
    e.stopPropagation();
    menuOpen = !menuOpen;
  }

  function openLinkPicker(e: MouseEvent) {
    e.stopPropagation();
    menuOpen = false;
    linkPickerOpen = true;
  }

  async function startUnlink(e: MouseEvent) {
    e.stopPropagation();
    menuOpen = false;
    if (!task.workspace_id) return;
    const preview = await tasks.unlink(task.id, false, task.repo_id);
    if (preview.kind === 'would_remove') {
      unlinkModalTitle = preview.workspace_title;
      unlinkModalOpen = true;
      return;
    }
    // No cleanup would fire — execute immediately.
    await tasks.unlink(task.id, true, task.repo_id);
  }

  async function confirmUnlink() {
    unlinkModalOpen = false;
    await tasks.unlink(task.id, true, task.repo_id);
  }

  const truncatedDescription = $derived(
    task.description.length > 80 ? task.description.slice(0, 80) + '...' : task.description
  );

  // PIC display: first name + "+N" when multiple. Em-dash placeholder when
  // the binding has a PIC field mapped but this record has no assignees, or
  // when the binding has no PIC field at all. Full list goes in the title
  // attribute for hover discovery.
  const picList = $derived<string[]>(task.pic_names ?? []);
  const picLabel = $derived(
    picList.length === 0
      ? '—'
      : picList.length === 1
        ? picList[0]
        : `${picList[0]} +${picList.length - 1}`
  );
  const picTooltip = $derived(picList.length > 0 ? picList.join(', ') : '');

  const linkedWorkspace = $derived(
    task.workspace_id ? (workspaces.byId(task.workspace_id) ?? null) : null
  );

  function jumpToWorkspace(e: MouseEvent) {
    e.stopPropagation();
    if (!task.workspace_id) return;
    workspaces.select(task.workspace_id);
    modeStore.set('work');
  }
</script>

<div
  class="task-card group flex flex-col gap-1.5 p-2 rounded bg-[var(--bg-base)] border border-[var(--border-light)] hover:border-[var(--accent)] cursor-grab active:cursor-grabbing transition-colors"
  role="listitem"
  aria-label={task.title}
>
  <div class="flex items-start justify-between gap-2">
    <span class="text-xs font-semibold text-[var(--text-primary)] flex-1 leading-snug">
      {task.title}
    </span>
    <div class="relative flex items-center gap-0.5 flex-shrink-0">
      <button
        class="opacity-0 group-hover:opacity-100 flex items-center justify-center w-5 h-5 rounded text-sm leading-none text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-all cursor-pointer"
        aria-label="Task menu"
        data-testid="task-menu-trigger"
        onclick={toggleMenu}
      >
        ⋯
      </button>
      <button
        class="opacity-0 group-hover:opacity-100 flex items-center justify-center w-5 h-5 rounded text-sm leading-none text-[var(--text-muted)] hover:text-[var(--error)] hover:bg-[var(--error-bg)] transition-all cursor-pointer"
        aria-label="Remove task"
        onclick={() => onRemove(task.id)}
      >
        ×
      </button>
      {#if menuOpen}
        <div
          class="absolute right-0 top-6 z-20 min-w-[180px] py-1 rounded border border-[var(--border-light)] bg-[var(--bg-card)] shadow-lg text-xs"
          role="menu"
        >
          <button
            type="button"
            class="block w-full text-left px-2 py-1 text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
            data-testid="task-menu-link-workspace"
            onclick={openLinkPicker}
          >
            Link to workspace…
          </button>
          {#if task.workspace_id}
            <button
              type="button"
              class="block w-full text-left px-2 py-1 text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
              data-testid="task-menu-unlink"
              onclick={startUnlink}
            >
              Unlink from workspace
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </div>

  {#if task.description}
    <p class="text-[11px] text-[var(--text-secondary)] leading-snug" data-testid="task-description">
      {truncatedDescription}
    </p>
  {/if}

  <div class="flex items-center justify-between gap-2 mt-0.5">
    {#if task.workspace_id}
      <span
        class="inline-flex items-center px-1.5 py-0.5 text-[10px] font-mono rounded bg-[var(--bg-hover)] text-[var(--accent)]"
        data-testid="branch-badge"
      >
        branch
      </span>
    {:else}
      <span></span>
    {/if}
    <span
      class="text-[10px] text-[var(--text-muted)] truncate max-w-[50%]"
      data-testid="task-pic"
      title={picTooltip}
    >
      {picLabel}
    </span>
  </div>

  {#if linkedWorkspace}
    <button
      type="button"
      class="mt-2 inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] bg-[var(--bg-hover)] text-[var(--text-muted)] hover:text-[var(--text-base)]"
      data-testid="task-workspace-chip"
      onclick={jumpToWorkspace}
      use:tooltip={{ text: `Open workspace ${linkedWorkspace.title}` }}
    >
      <span aria-hidden="true">◆</span>
      <span class="truncate max-w-[140px]">{linkedWorkspace.title}</span>
    </button>
  {/if}
</div>

<LinkWorkspacePicker
  taskId={task.id}
  repoId={task.repo_id}
  open={linkPickerOpen}
  onClose={() => (linkPickerOpen = false)}
/>

<UnlinkConfirmModal
  open={unlinkModalOpen}
  workspaceTitle={unlinkModalTitle}
  onConfirm={confirmUnlink}
  onCancel={() => (unlinkModalOpen = false)}
/>
