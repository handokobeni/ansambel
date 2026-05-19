<script lang="ts">
  import { teamActivity } from '$lib/stores/team-activity.svelte';
  import { githubBranchUrl } from '$lib/github-url';

  const row = $derived.by(() => {
    const id = teamActivity.selectedWorkspaceId;
    if (!id) return null;
    return teamActivity.rows.get(id) ?? null;
  });

  const branchUrl = $derived(row ? githubBranchUrl(row.repo_remote_url, row.branch_name) : null);

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

{#if row}
  <article class="h-full overflow-auto p-6" data-testid="team-workspace-mirror">
    <header class="flex items-start justify-between gap-4 border-b border-[var(--border)] pb-4">
      <div class="min-w-0">
        <h1 class="text-xl font-semibold text-[var(--text-primary)] truncate">
          {row.task_title}
        </h1>
        <p class="mt-1 text-sm text-[var(--text-secondary)]">
          {row.assignee_machine}
          <span class="mx-2 text-[var(--text-dim)]">·</span>
          <span data-testid="team-mirror-status">{row.ansambel_status}</span>
          <span class="mx-2 text-[var(--text-dim)]">·</span>
          <span class="text-[var(--text-dim)]">{relativeTime(row.last_activity_at)}</span>
        </p>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        <button
          type="button"
          class="text-sm text-[var(--text-dim)] hover:text-[var(--text-primary)]"
          onclick={() => teamActivity.refresh()}
          aria-label="refresh team workspace"
        >
          ↻
        </button>
        <button
          type="button"
          class="text-sm rounded border border-[var(--border)] px-2 py-1 hover:bg-[var(--bg-hover)]"
          onclick={() => teamActivity.select(null)}
          aria-label="back to workspace"
        >
          ← Back
        </button>
      </div>
    </header>

    <section class="mt-6">
      <h2 class="text-sm uppercase tracking-wide text-[var(--text-dim)]">Code state</h2>
      <div class="mt-2 flex items-center gap-3 text-sm">
        {#if row.branch_name}
          <span
            class="rounded bg-[var(--bg-card)] px-2 py-0.5 font-mono text-[var(--text-secondary)]"
            >{row.branch_name}</span
          >
        {/if}
        {#if branchUrl}
          <a
            href={branchUrl}
            target="_blank"
            rel="noopener noreferrer"
            class="text-blue-400 hover:underline"
          >
            Open branch on GitHub
          </a>
        {/if}
      </div>
      <p class="mt-2 text-sm text-[var(--text-secondary)]">
        {#if row.diff_summary}
          {row.diff_summary}
        {:else}
          <span class="italic text-[var(--text-dim)]">
            Not yet published — Ansambel doesn't yet emit diff summaries; see Phase 3a-5/6 followups
            in the design spec.
          </span>
        {/if}
      </p>
    </section>

    <section class="mt-6">
      <h2 class="text-sm uppercase tracking-wide text-[var(--text-dim)]">Latest activity</h2>
      <pre
        class="mt-2 whitespace-pre-wrap rounded bg-[var(--bg-card)] p-3 text-sm text-[var(--text-primary)]">{row.last_message_preview ||
          '—'}</pre>
      {#if row.ansambel_status === 'pr_ready' && row.pr_url}
        <a
          href={row.pr_url}
          target="_blank"
          rel="noopener noreferrer"
          class="mt-3 inline-block rounded bg-purple-600 px-3 py-1.5 text-sm text-white hover:bg-purple-500"
        >
          Open PR
        </a>
      {/if}
    </section>
  </article>
{/if}
