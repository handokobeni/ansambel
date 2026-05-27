<!-- src/lib/components/Sidebar.svelte -->
<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';
  import { repos } from '$lib/stores/repos.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { messages } from '$lib/stores/messages.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import { tooltip } from '$lib/actions';
  import ResizeHandle from './ResizeHandle.svelte';
  import type { AgentStatus, KanbanColumn, Workspace, WorkspaceStatus } from '$lib/types';
  import { onDestroy, onMount } from 'svelte';
  import TeamActivityPanel from './sidebar/TeamActivityPanel.svelte';
  import { teamActivity } from '$lib/stores/team-activity.svelte';

  onMount(() => {
    teamActivity.start();
  });

  onDestroy(() => {
    teamActivity.stop();
  });

  const selectedRepo = $derived(repos.getSelected());
  const workspaceList = $derived(selectedRepo ? workspaces.listForRepo(selectedRepo.id) : []);

  let showForm = $state(false);
  let formTitle = $state('');
  let formDescription = $state('');
  let formBranch = $state('');
  let formSubmitting = $state(false);

  // ── Width / resize ─────────────────────────────────────────────────
  const SIDEBAR_MIN = 180;
  const SIDEBAR_MAX = 420;
  const SIDEBAR_DEFAULT = 280;
  const WIDTH_STORAGE_KEY = 'ansambel-sidebar-width';

  function readStoredWidth(): number {
    /* v8 ignore next 1 — SSR/Node guard: not reachable in jsdom. */
    if (typeof localStorage === 'undefined') return SIDEBAR_DEFAULT;
    const raw = localStorage.getItem(WIDTH_STORAGE_KEY);
    const parsed = raw === null ? NaN : Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) return SIDEBAR_DEFAULT;
    return Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, parsed));
  }

  let width = $state(readStoredWidth());

  function handleResize(delta: number): void {
    width = Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, width + delta));
  }

  function handleResizeEnd(): void {
    /* v8 ignore next 1 — SSR/Node guard: not reachable in jsdom. */
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(WIDTH_STORAGE_KEY, String(width));
  }

  // ── Grouping ───────────────────────────────────────────────────────
  // Active = todo + in_progress (an agent is either staged or actively working).
  // Review = review (waiting on human sign-off).
  // Done   = done (kept collapsed by default so the active list isn't drowned).
  type GroupKey = 'active' | 'review' | 'done';

  const GROUP_LABELS: Record<GroupKey, string> = {
    active: 'Active',
    review: 'Review',
    done: 'Done',
  };

  function bucketFor(column: KanbanColumn): GroupKey {
    if (column === 'review') return 'review';
    if (column === 'done') return 'done';
    return 'active';
  }

  const groups = $derived.by(() => {
    const buckets: Record<GroupKey, Workspace[]> = { active: [], review: [], done: [] };
    for (const ws of workspaceList) buckets[bucketFor(ws.column)].push(ws);
    // Stable order: oldest workspaces stay anchored at the top of each
    // group, freshly-created ones land at the bottom.
    for (const key of Object.keys(buckets) as GroupKey[]) {
      buckets[key].sort((a, b) => a.created_at - b.created_at);
    }
    return (Object.keys(GROUP_LABELS) as GroupKey[]).map((key) => ({
      key,
      label: GROUP_LABELS[key],
      items: buckets[key],
    }));
  });

  // Done starts collapsed so freshly-shipped workspaces don't dominate
  // the visible list. Other groups stay expanded.
  const collapsed = new SvelteSet<GroupKey>(['done']);

  function toggleGroup(key: GroupKey): void {
    if (collapsed.has(key)) collapsed.delete(key);
    else collapsed.add(key);
  }

  // ── Status dots ────────────────────────────────────────────────────
  // The persisted `Workspace.status` trails the live agent run state (it's
  // only refreshed when the backend repersists the workspace), so a running
  // agent would otherwise show a stale, non-green dot here while the title
  // bar correctly reads "Running". Overlay the live messages-store status
  // exactly like WorkspaceView does so both surfaces agree.
  function effectiveStatus(ws: Workspace): WorkspaceStatus | AgentStatus {
    return messages.statusFor(ws.id) ?? ws.status;
  }

  function statusDotClass(status: WorkspaceStatus | AgentStatus): string {
    // Green = running (agent is actively processing) matches the convention
    // users expect ("green = active/on"). Waiting is alive-but-idle, shown
    // amber as a softer ping.
    if (status === 'running') return 'bg-[var(--status-ok)]';
    if (status === 'waiting') return 'bg-amber-400';
    return 'bg-[var(--text-muted)]';
  }

  function handleSelectWorkspace(id: string): void {
    workspaces.select(id);
  }

  async function handleRemoveWorkspace(e: MouseEvent, wsId: string, repoId: string): Promise<void> {
    e.stopPropagation();
    if (!window.confirm('Remove this workspace? The git worktree will be deleted.')) return;
    try {
      await workspaces.remove(wsId, repoId);
    } catch (err) {
      addToast(
        `Failed to remove workspace: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      );
    }
  }

  async function handleCreateSubmit(e: Event): Promise<void> {
    e.preventDefault();
    if (!selectedRepo || !formTitle.trim()) return;
    formSubmitting = true;
    try {
      await workspaces.create({
        repoId: selectedRepo.id,
        title: formTitle.trim(),
        description: formDescription.trim(),
        branchName: formBranch.trim() || undefined,
      });
      formTitle = '';
      formDescription = '';
      formBranch = '';
      showForm = false;
    } catch (err) {
      addToast(
        `Failed to create workspace: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      );
    } finally {
      formSubmitting = false;
    }
  }

  function handleCancelForm(): void {
    showForm = false;
    formTitle = '';
    formDescription = '';
    formBranch = '';
  }
</script>

<div class="flex h-full" data-sidebar-shell>
  <aside
    class="flex flex-col h-full bg-[var(--bg-sidebar)] border-r border-[var(--border)] overflow-hidden"
    style="width: {width}px"
    data-sidebar
    data-sidebar-width={width}
  >
    <div class="flex items-center justify-between px-3 py-2 border-b border-[var(--border)]">
      <span class="text-xs font-semibold uppercase tracking-wider text-[var(--text-muted)]">
        Workspaces
      </span>
      <button
        class="flex items-center justify-center w-6 h-6 rounded text-base leading-none text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer disabled:opacity-50"
        onclick={() => {
          showForm = !showForm;
        }}
        disabled={!selectedRepo}
        aria-label="New Workspace"
        use:tooltip={{ text: selectedRepo ? 'New workspace' : 'Select a repo first' }}
      >
        +
      </button>
    </div>

    {#if showForm}
      <!-- Inline new workspace form -->
      <form
        class="flex flex-col gap-2 px-3 py-2 border-b border-[var(--border)] bg-[var(--bg-card)]"
        onsubmit={handleCreateSubmit}
      >
        <input
          class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
          type="text"
          placeholder="Workspace title"
          bind:value={formTitle}
          required
        />
        <textarea
          class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none"
          placeholder="Description (optional)"
          rows={2}
          bind:value={formDescription}
        ></textarea>
        <input
          class="w-full px-2 py-1 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
          type="text"
          placeholder="Branch name (optional)"
          bind:value={formBranch}
        />
        <div class="flex gap-2">
          <button
            type="submit"
            class="flex-1 py-1 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 cursor-pointer"
            disabled={formSubmitting || !formTitle.trim()}
            aria-label="Create"
          >
            {formSubmitting ? 'Creating…' : 'Create'}
          </button>
          <button
            type="button"
            class="py-1 px-2 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
            onclick={handleCancelForm}
          >
            Cancel
          </button>
        </div>
      </form>
    {/if}

    <div class="flex-1 overflow-y-auto py-1">
      {#if workspaceList.length === 0}
        <div class="px-3 py-3 text-xs text-[var(--text-muted)] text-center">
          {#if selectedRepo}
            No workspaces yet
          {:else}
            Select a repo to see workspaces
          {/if}
        </div>
      {:else}
        {#each groups as group (group.key)}
          {#if group.items.length > 0}
            <section data-sidebar-group={group.key}>
              <button
                type="button"
                class="w-full flex items-center justify-between px-3 py-1 text-[0.65rem] font-semibold uppercase tracking-wider text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors cursor-pointer"
                aria-expanded={!collapsed.has(group.key)}
                aria-controls="sidebar-group-{group.key}"
                onclick={() => toggleGroup(group.key)}
              >
                <span class="flex items-center gap-1.5">
                  <svg
                    viewBox="0 0 24 24"
                    width="9"
                    height="9"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="3"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                    class="transition-transform {collapsed.has(group.key) ? '-rotate-90' : ''}"
                  >
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                  {group.label}
                </span>
                <span class="text-[var(--text-dim)]">{group.items.length}</span>
              </button>
              {#if !collapsed.has(group.key)}
                <ul id="sidebar-group-{group.key}" data-sidebar-group-list={group.key}>
                  {#each group.items as ws (ws.id)}
                    {@const dotStatus = effectiveStatus(ws)}
                    <!-- svelte-ignore a11y_click_events_have_key_events -->
                    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                    <li
                      class="group flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors border-l-2"
                      class:bg-[var(--bg-active)]={workspaces.selectedWorkspaceId === ws.id}
                      class:border-[var(--accent)]={workspaces.selectedWorkspaceId === ws.id}
                      class:border-transparent={workspaces.selectedWorkspaceId !== ws.id}
                      data-workspace-id={ws.id}
                      data-selected={workspaces.selectedWorkspaceId === ws.id}
                      onclick={() => handleSelectWorkspace(ws.id)}
                    >
                      <!-- Status dot -->
                      <span
                        class="w-2 h-2 rounded-full flex-shrink-0 {statusDotClass(dotStatus)}"
                        data-status-dot
                        data-status={dotStatus}
                        aria-label="Status: {dotStatus}"
                      ></span>

                      <!-- Title -->
                      <span
                        class="flex-1 text-xs text-[var(--text-secondary)] overflow-hidden text-ellipsis whitespace-nowrap group-hover:text-[var(--text-primary)] transition-colors"
                      >
                        {ws.title}
                      </span>

                      <!-- Remove button -->
                      <button
                        class="opacity-0 group-hover:opacity-100 flex items-center justify-center w-4 h-4 rounded text-[var(--text-muted)] hover:text-[var(--error)] hover:bg-[var(--error-bg)] transition-all cursor-pointer"
                        onclick={(e) => handleRemoveWorkspace(e, ws.id, ws.repo_id)}
                        aria-label="Remove workspace"
                        use:tooltip={{ text: 'Remove workspace' }}
                      >
                        ×
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </section>
          {/if}
        {/each}
      {/if}
    </div>
    <TeamActivityPanel />
  </aside>
  <ResizeHandle direction="horizontal" onResize={handleResize} onResizeEnd={handleResizeEnd} />
</div>
