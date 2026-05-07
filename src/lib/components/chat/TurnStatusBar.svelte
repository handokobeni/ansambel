<script lang="ts">
  import { onDestroy } from 'svelte';
  import { messages } from '$lib/stores/messages.svelte';
  import { tooltip } from '$lib/actions';
  import type { TurnState } from '$lib/types';

  interface Props {
    workspaceId: string;
    /** How often (ms) the active verb rotates through the playful list.
     *  Exposed for tests; production uses the default. */
    verbCycleMs?: number;
  }

  const { workspaceId, verbCycleMs = 5_000 }: Props = $props();

  // Live wall-clock — bumped every second by setInterval. Drives the
  // elapsed-seconds derivation off Date.now() against the turn's startedAt
  // so resuming after a workspace switch keeps the count accurate.
  let now = $state(Date.now());
  let intervalId: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    // Re-arm the timer whenever the workspace changes; the component is
    // typically mounted once per ChatPanel so the dependency on
    // workspaceId is mostly a safety net.
    void workspaceId;
    if (intervalId !== undefined) clearInterval(intervalId);
    intervalId = setInterval(() => {
      now = Date.now();
    }, 1_000);
    return () => {
      if (intervalId !== undefined) clearInterval(intervalId);
      intervalId = undefined;
    };
  });

  onDestroy(() => {
    if (intervalId !== undefined) clearInterval(intervalId);
  });

  const turn = $derived(messages.turnFor(workspaceId));

  // Rotating playful verbs — purely cosmetic, matches the Claude CLI vibe.
  // Cycle index is computed off the elapsed seconds so the rotation is
  // deterministic and doesn't need its own timer.
  const VERBS = ['Cooking', 'Forging', 'Brewing', 'Crunching'];

  function formatTokens(n: number): string {
    // < 1000 stays as a raw count to avoid the misleading "0.0k" early in
    // the turn. ≥ 1000 collapses to one decimal so the line stays compact
    // even when context is massive.
    if (n < 1_000) return `${n}`;
    return `${(n / 1_000).toFixed(1)}k`;
  }

  function tokensTooltip(t: TurnState): string {
    return `Fresh input: ${t.inputTokens.toLocaleString()} tokens (input + cache writes, billed at full rate)`;
  }
</script>

{#if turn}
  {@const elapsedSec = Math.max(0, Math.floor((now - turn.startedAt) / 1000))}
  {@const verbStep = Math.max(1, Math.floor(verbCycleMs / 1000))}
  {@const verb = VERBS[Math.floor(elapsedSec / verbStep) % VERBS.length]}
  <div
    data-testid="turn-status-bar"
    class="flex items-center gap-2 px-3 py-1.5 text-xs text-[var(--text-muted)] border-t border-[var(--border)] bg-[var(--bg-base)]"
    aria-live="polite"
  >
    <span class="text-[var(--accent)]">·</span>
    <span data-testid="turn-verb" class="text-[var(--text-secondary)] font-medium">{verb}…</span>
    <span data-testid="turn-elapsed">({elapsedSec}s</span>
    <span aria-hidden="true">·</span>
    <!-- ↓ = fresh input + cache_creation; ↻ = cached re-reads (billed
         at 10% of input). Splitting these keeps multi-step turns from
         flashing alarming numbers like "900k tokens" when most of that
         is the same cached system prompt being re-read each step. -->
    <span data-testid="turn-tokens" use:tooltip={{ text: tokensTooltip(turn) }}>
      ↓ {formatTokens(turn.inputTokens)}
    </span>
    {#if turn.cachedTokens > 0}
      <span aria-hidden="true">·</span>
      <span
        data-testid="turn-cached"
        class="text-[var(--text-dim)]"
        use:tooltip={{ text: 'Cached context re-read (billed at 10% of input)' }}
      >
        ↻ {formatTokens(turn.cachedTokens)}
      </span>
    {/if}
    {#if turn.outputTokens > 0}
      <span aria-hidden="true">·</span>
      <span data-testid="turn-output">↑ {formatTokens(turn.outputTokens)}</span>
    {/if}
    <span aria-hidden="true">)</span>
  </div>
{/if}
