// In-memory, session-only stash of serialized xterm screen state, keyed by
// terminal id. Not reactive UI state — a TerminalPane writes its serialized
// grid here on destroy and the next mount of the same terminal id reads it
// back (one-shot) to repaint before resubscribing to live PTY output. Plain
// Map (not SvelteMap): nothing renders from it.

const snapshots = new Map<string, string>();

export const terminalSnapshots = {
  set(terminalId: string, data: string): void {
    snapshots.set(terminalId, data);
  },
  /** Read and remove — restore is one-shot so a stale grid is never reused. */
  take(terminalId: string): string | undefined {
    const data = snapshots.get(terminalId);
    snapshots.delete(terminalId);
    return data;
  },
  /** Forget a terminal's snapshot without reading it (e.g. on explicit close). */
  drop(terminalId: string): void {
    snapshots.delete(terminalId);
  },
  reset(): void {
    snapshots.clear();
  },
};
