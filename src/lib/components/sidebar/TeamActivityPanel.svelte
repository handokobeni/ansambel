<script module lang="ts">
  /** Status colour palette mirrors the WorkspaceView dot for visual
   *  continuity. Module-scoped so the function is hoist-stable and
   *  testable in isolation. */
  export function statusColor(status: string): string {
    switch (status) {
      case 'running':
        return '#22c55e'; // green
      case 'waiting':
        return '#eab308'; // yellow
      case 'blocked':
        return '#ef4444'; // red
      case 'pr_ready':
        return '#a855f7'; // purple
      case 'done':
        return '#6b7280'; // grey
      default:
        return '#9ca3af'; // grey-light
    }
  }
</script>

<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity';
  import { teamActivity } from '$lib/stores/team-activity.svelte';
  import type { TeamActivityRow } from '$lib/types';

  const COLLAPSE_STORAGE_KEY = 'ansambel-team-activity-collapsed';

  function readCollapsed(): boolean {
    /* v8 ignore next 1 */
    if (typeof localStorage === 'undefined') return false;
    return localStorage.getItem(COLLAPSE_STORAGE_KEY) === 'true';
  }

  let collapsed = $state(readCollapsed());

  function toggleCollapsed(): void {
    collapsed = !collapsed;
    /* v8 ignore next 1 */
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(COLLAPSE_STORAGE_KEY, String(collapsed));
    }
  }

  function groupByRepo(rows: Iterable<TeamActivityRow>): Array<[string, TeamActivityRow[]]> {
    const groups = new SvelteMap<string, TeamActivityRow[]>();
    for (const r of rows) {
      const key = r.repo_display_name || '—';
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(r);
    }
    return Array.from(groups.entries()).sort(([a], [b]) => a.localeCompare(b));
  }

  const groups = $derived(groupByRepo(teamActivity.rows.values()));
  const isEmpty = $derived(teamActivity.rows.size === 0);

  function relativeTime(epochMs: number): string {
    if (!epochMs) return '';
    const diffMs = Date.now() - epochMs;
    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 60) return 'just now';
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }
</script>

<section
  class="border-t border-[var(--border)] mt-2 pt-2 text-xs"
  aria-label="Team Activity"
  data-testid="team-activity-panel"
>
  <header class="flex items-center justify-between px-3 py-1">
    <button
      type="button"
      class="flex items-center gap-1 text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
      onclick={toggleCollapsed}
      aria-label={collapsed ? 'expand team activity' : 'collapse team activity'}
    >
      <span aria-hidden="true">{collapsed ? '▶' : '▼'}</span>
      <span class="uppercase tracking-wide font-semibold">Team Activity</span>
    </button>
    <button
      type="button"
      class="text-[var(--text-dim)] hover:text-[var(--text-primary)]"
      onclick={() => teamActivity.refresh()}
      aria-label="refresh team activity"
    >
      ↻
    </button>
  </header>

  {#if !collapsed}
    {#if teamActivity.status === 'disabled'}
      <p class="px-3 py-2 text-[var(--text-muted)]">Configure Team Activity in Settings →</p>
    {:else if teamActivity.status === 'machine_label_empty'}
      <p class="px-3 py-2 text-[var(--text-muted)]">Set your machine label in Settings →</p>
    {:else if teamActivity.status === 'no_overlap_repos'}
      <p class="px-3 py-2 text-[var(--text-muted)]">Add a repo to see team activity.</p>
    {:else if teamActivity.status === 'error'}
      <div class="mx-3 my-2 p-2 rounded border border-red-500/40 bg-red-500/10">
        <p class="text-red-300">{teamActivity.error}</p>
        <button
          type="button"
          class="mt-1 text-xs underline text-red-300 hover:text-red-200"
          onclick={() => teamActivity.refresh()}
        >
          Retry
        </button>
      </div>
    {:else if isEmpty}
      <p class="px-3 py-2 text-[var(--text-muted)]">No team activity in your repos right now.</p>
    {:else}
      <ul class="space-y-2">
        {#each groups as [repo, rows] (repo)}
          <li data-testid="team-activity-repo-group" data-repo={repo}>
            <p class="px-3 text-[var(--text-dim)] uppercase tracking-wide">
              {repo}
            </p>
            <ul>
              {#each rows as row (row.workspace_id)}
                <li>
                  <button
                    type="button"
                    class="flex items-center gap-2 w-full px-3 py-1 hover:bg-[var(--bg-hover)] text-left"
                    onclick={() => teamActivity.select(row.workspace_id)}
                    data-testid="team-activity-row"
                    data-workspace-id={row.workspace_id}
                  >
                    <span
                      class="inline-block w-2 h-2 rounded-full"
                      data-testid="team-activity-status-dot"
                      data-status={row.ansambel_status || 'idle'}
                      style:background-color={statusColor(row.ansambel_status)}
                    ></span>
                    <span class="flex-1 truncate">
                      <span class="text-[var(--text-primary)]">{row.assignee_machine}</span>
                      <span class="text-[var(--text-muted)]"> · {row.task_title}</span>
                    </span>
                    <span class="text-[var(--text-dim)] shrink-0">
                      {relativeTime(row.last_activity_at)}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>
