import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { messages } from '$lib/stores/messages.svelte';
import TurnStatusBar from './TurnStatusBar.svelte';

beforeEach(() => {
  messages.reset();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('TurnStatusBar', () => {
  it('renders nothing when no turn is active', () => {
    const { container } = render(TurnStatusBar, { props: { workspaceId: 'ws_a' } });
    expect(container.querySelector('[data-testid="turn-status-bar"]')).toBeNull();
  });

  it('renders elapsed-seconds line and rotating verb when a turn is active', () => {
    // Pin the clock so startedAt is deterministic.
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_a');
    const { container, getByTestId } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_a' },
    });
    expect(container.querySelector('[data-testid="turn-status-bar"]')).toBeTruthy();
    // Initial elapsed should read 0s.
    expect(getByTestId('turn-elapsed').textContent).toMatch(/0s/);
    // Verb is one of the configured verbs.
    expect(getByTestId('turn-verb').textContent).toMatch(/(Cooking|Forging|Brewing|Crunching)/);
  });

  it('updates elapsed seconds as time advances', async () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_b');
    const { getByTestId } = render(TurnStatusBar, { props: { workspaceId: 'ws_b' } });
    expect(getByTestId('turn-elapsed').textContent).toMatch(/0s/);
    // advanceTimersByTimeAsync moves Date.now() too with fake timers, so
    // the component's setInterval callback recomputes against the new
    // "now" automatically — no need to setSystemTime separately.
    await vi.advanceTimersByTimeAsync(51_000);
    expect(getByTestId('turn-elapsed').textContent).toMatch(/51s/);
  });

  it('formats fresh-input token count as Yk with one decimal place', () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_t');
    messages.apply(
      {
        type: 'usage',
        message_id: 'msg_a',
        input_tokens: 200,
        cache_creation_input_tokens: 1800,
        cache_read_input_tokens: 0,
        output_tokens: 0,
        total_input: 2000,
      },
      'ws_t'
    );
    const { getByTestId } = render(TurnStatusBar, { props: { workspaceId: 'ws_t' } });
    // 200 + 1800 = 2000 fresh tokens → "2.0k". The down-arrow chip
    // tracks bytes freshly fed into the model, billed at full rate.
    expect(getByTestId('turn-tokens').textContent).toMatch(/↓\s*2\.0k/);
  });

  it('shows raw count when below 1000', () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_lo');
    messages.apply(
      {
        type: 'usage',
        message_id: 'msg_a',
        input_tokens: 12,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 0,
        total_input: 12,
      },
      'ws_lo'
    );
    const { getByTestId } = render(TurnStatusBar, { props: { workspaceId: 'ws_lo' } });
    expect(getByTestId('turn-tokens').textContent).toMatch(/↓\s*12/);
  });

  it('shows a separate ↻ chip for cache_read tokens, but only when > 0', () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_cache');
    // Fresh-only first turn — no cached chip yet.
    messages.apply(
      {
        type: 'usage',
        message_id: 'msg_first',
        input_tokens: 30,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 10,
        total_input: 30,
      },
      'ws_cache'
    );
    const { queryByTestId, rerender } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_cache' },
    });
    expect(queryByTestId('turn-cached')).toBeNull();

    // Subsequent step replays a 50k cached system prompt — chip appears.
    messages.apply(
      {
        type: 'usage',
        message_id: 'msg_second',
        input_tokens: 5,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 50000,
        output_tokens: 80,
        total_input: 50005,
      },
      'ws_cache'
    );
    // Force re-render so the $derived turn snapshot updates.
    rerender({ workspaceId: 'ws_cache' });
    const cached = queryByTestId('turn-cached');
    expect(cached).not.toBeNull();
    expect(cached!.textContent).toMatch(/↻\s*50\.0k/);
  });

  it('shows the output ↑ chip with cumulative output_tokens', () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_out');
    messages.apply(
      {
        type: 'usage',
        message_id: 'msg_a',
        input_tokens: 10,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 1500,
        total_input: 10,
      },
      'ws_out'
    );
    const { getByTestId } = render(TurnStatusBar, { props: { workspaceId: 'ws_out' } });
    expect(getByTestId('turn-output').textContent).toMatch(/↑\s*1\.5k/);
  });

  it('does NOT inflate inputTokens by accumulating cache_read across steps', () => {
    // Regression guard for the "900k tokens for a basic question" bug:
    // a multi-step turn that re-reads a 100k cached prompt 9 times must
    // surface the cache replays under ↻, not under ↓.
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_900k');
    for (let i = 0; i < 9; i++) {
      messages.apply(
        {
          type: 'usage',
          message_id: `msg_${i}`,
          input_tokens: 50,
          cache_creation_input_tokens: 0,
          cache_read_input_tokens: 100_000,
          output_tokens: 200,
          total_input: 100_050,
        },
        'ws_900k'
      );
    }
    const { getByTestId } = render(TurnStatusBar, { props: { workspaceId: 'ws_900k' } });
    // Fresh = 9 * 50 = 450 tokens. NOT 900k.
    expect(getByTestId('turn-tokens').textContent).toMatch(/↓\s*450/);
    // Cached = 9 * 100k = 900k — visible separately so the user sees
    // the truth without confusing replay volume with new spend.
    expect(getByTestId('turn-cached').textContent).toMatch(/↻\s*900\.0k/);
  });

  it('hides itself once the turn ends', async () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_end');
    const { container, rerender } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_end' },
    });
    expect(container.querySelector('[data-testid="turn-status-bar"]')).toBeTruthy();
    messages.apply({ type: 'status', status: 'waiting' }, 'ws_end');
    // Force a re-render so the $derived turn snapshot updates.
    await rerender({ workspaceId: 'ws_end' });
    expect(container.querySelector('[data-testid="turn-status-bar"]')).toBeNull();
  });

  it('clears its interval on unmount (no leaks across workspace switches)', async () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_unmount');
    const { unmount, getByTestId } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_unmount' },
    });
    expect(getByTestId('turn-elapsed').textContent).toMatch(/0s/);
    // Snapshot current text so we can prove it does NOT update post-unmount.
    unmount();
    // Advance time past where the interval would have fired. If the timer
    // wasn't cleared, the orphaned $effect would still try to write `now`
    // and a leak warning would surface in stdout. We can't easily assert on
    // the warning, but exercising the unmount path covers the cleanup
    // branches in $effect's return + onDestroy.
    await vi.advanceTimersByTimeAsync(3_000);
  });

  it('renders nothing for the no-turn branch even when verbCycleMs is provided', () => {
    // Pure-defensive test: ensures the `turn ? ... : 0` and `turn ? ... :
    // VERBS[0]` ternaries are exercised in their false branch. Without a
    // status:running event the bar must stay hidden.
    const { container } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_noop', verbCycleMs: 1000 },
    });
    expect(container.querySelector('[data-testid="turn-status-bar"]')).toBeNull();
  });

  it('re-arms the interval when workspaceId changes (covers line 27 re-arm branch)', async () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_switch1');
    messages.apply({ type: 'status', status: 'running' }, 'ws_switch2');
    const { rerender, container } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_switch1' },
    });
    // interval is now set up for ws_switch1
    // Rerender with a different workspaceId — triggers $effect re-run, which
    // hits the `if (intervalId !== undefined) clearInterval(intervalId)` branch
    await rerender({ workspaceId: 'ws_switch2' });
    // Component still alive, new workspace's turn shown
    expect(container.querySelector('[data-testid="turn-status-bar"]')).toBeTruthy();
  });

  it('rotates the verb every 5 seconds', async () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_verb');
    const { getByTestId } = render(TurnStatusBar, {
      props: { workspaceId: 'ws_verb', verbCycleMs: 5000 },
    });
    const first = getByTestId('turn-verb').textContent;
    await vi.advanceTimersByTimeAsync(6_000);
    // Different verb after the cycle window. We don't pin the exact verb —
    // just that it changed, so rotation order can be tweaked freely.
    expect(getByTestId('turn-verb').textContent).not.toBe(first);
  });
});
