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

  it('formats token count as Yk with one decimal place', () => {
    vi.setSystemTime(new Date('2026-04-30T12:00:00Z').getTime());
    messages.apply({ type: 'status', status: 'running' }, 'ws_t');
    messages.apply(
      {
        type: 'usage',
        message_id: 'msg_a',
        input_tokens: 0,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 2000,
        output_tokens: 0,
        total_input: 2000,
      },
      'ws_t'
    );
    const { getByTestId } = render(TurnStatusBar, { props: { workspaceId: 'ws_t' } });
    // 2000 → "2.0k". The down-arrow points at the input direction (context
    // sent to the model), matching the Claude CLI convention.
    expect(getByTestId('turn-tokens').textContent).toMatch(/↓\s*2\.0k tokens/);
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
    expect(getByTestId('turn-tokens').textContent).toMatch(/↓\s*12 tokens/);
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
