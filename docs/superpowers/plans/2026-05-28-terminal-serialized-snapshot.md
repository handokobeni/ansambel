# Terminal Serialized-Snapshot Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Fix B's raw-byte replay (which garbles full-screen programs
like vite on pane remount) with a serialized-screen snapshot taken on the
frontend, so a remounted terminal repaints cleanly and then resumes live output.

**Architecture:** Each `TerminalPane` owns a `SerializeAddon`. On destroy it
serializes the live xterm grid+scrollback into a module-level stash keyed by
`terminalId`. On the next mount it writes that serialized snapshot (a
normalized, self-consistent escape stream) before re-subscribing to the live
PTY. The backend reverts to a plain `reattach` that only returns the live
receiver — the `output_buffer` ring buffer added in Fix B is removed.

**Tech Stack:** Svelte 5 runes, `@xterm/xterm` + `@xterm/addon-serialize`, Tauri
Channel API, Rust (`commands/terminal.rs`, `state.rs`), vitest, cargo test.

**Why this is correct:** Within a workspace, panes are display-toggled and never
unmounted, so the live xterm has seen every byte right up to dispose —
serializing at dispose captures the true screen. Raw-byte replay re-executes
cursor/clear sequences against a terminal whose size/state differs from capture
time, which is what produced the vite gap. A serialized grid has no such
dependency.

**Trade-off (accepted):** Output emitted in the brief dispose→reattach window
(tens of ms on a workspace switch) is not restored. The live stream resumes
immediately after. For long-lived dev servers this is invisible.

---

## Task 1: Frontend snapshot stash + dependency

**Files:**

- Modify: `package.json` (add `@xterm/addon-serialize`)
- Create: `src/lib/stores/terminal-snapshots.ts`
- Test: `src/lib/stores/terminal-snapshots.test.ts`

- [ ] **Step 1: Add the dependency**

Run: `bun add @xterm/addon-serialize@^0.13.0` Expected: resolves a version
compatible with `@xterm/xterm@^6` and updates `package.json` + lockfile. If
`^0.13.0` does not satisfy xterm 6, take whatever version
`bun add @xterm/addon-serialize` resolves and record it.

- [ ] **Step 2: Write the failing test**

```ts
// src/lib/stores/terminal-snapshots.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { terminalSnapshots } from './terminal-snapshots';

describe('terminalSnapshots', () => {
  beforeEach(() => terminalSnapshots.reset());

  it('take returns the stored snapshot then clears it (one-shot)', () => {
    terminalSnapshots.set('term_a', 'SCREEN');
    expect(terminalSnapshots.take('term_a')).toBe('SCREEN');
    expect(terminalSnapshots.take('term_a')).toBeUndefined();
  });

  it('take returns undefined when nothing is stored', () => {
    expect(terminalSnapshots.take('missing')).toBeUndefined();
  });

  it('keeps snapshots isolated per terminal id', () => {
    terminalSnapshots.set('term_a', 'A');
    terminalSnapshots.set('term_b', 'B');
    expect(terminalSnapshots.take('term_b')).toBe('B');
    expect(terminalSnapshots.take('term_a')).toBe('A');
  });

  it('drop removes a snapshot without reading it', () => {
    terminalSnapshots.set('term_a', 'A');
    terminalSnapshots.drop('term_a');
    expect(terminalSnapshots.take('term_a')).toBeUndefined();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `bun run vitest run src/lib/stores/terminal-snapshots.test.ts` Expected:
FAIL — module `./terminal-snapshots` not found.

- [ ] **Step 4: Write minimal implementation**

```ts
// src/lib/stores/terminal-snapshots.ts
//
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
```

- [ ] **Step 5: Run test to verify it passes**

Run: `bun run vitest run src/lib/stores/terminal-snapshots.test.ts` Expected:
PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add package.json bun.lock src/lib/stores/terminal-snapshots.ts src/lib/stores/terminal-snapshots.test.ts
git commit -m "feat(terminal): session snapshot stash for serialized screen restore"
```

---

## Task 2: Serialize on destroy, restore on mount in TerminalPane

**Files:**

- Modify: `src/lib/components/workspace/TerminalPane.svelte`
- Test: `src/lib/components/workspace/TerminalPane.test.ts` (extend existing)

Behaviour to add to `TerminalPane.svelte`:

- Import `terminalSnapshots` from `$lib/stores/terminal-snapshots` and load the
  `SerializeAddon` alongside `FitAddon` in `onMount`.
- In `onMount`, AFTER `fit.fit()` and the `[xterm ready]` marker but BEFORE the
  reattach/spawn block:
  `const restored = terminalSnapshots.take(terminalId); if (restored) term.write(restored);`
  Writing the marker first then the restored grid is fine — the serialized grid
  is a full screen repaint; if a snapshot exists it visually supersedes the
  marker. (Do not skip the marker — it still helps the no-snapshot fresh-spawn
  case.)
- Keep a reference to the `SerializeAddon` instance (e.g. `let serializer`).
- In `onDestroy`, BEFORE `term.dispose()`: if `term` and `serializer` exist,
  wrap `terminalSnapshots.set(terminalId, serializer.serialize())` in try/catch
  (serialize can throw on a never-opened term) — on throw, do nothing.

- [ ] **Step 1: Write the failing tests**

Add to `src/lib/components/workspace/TerminalPane.test.ts`. These rely on the
existing xterm mock in that file (a fake `Terminal` with `write`, `onData`,
`open`, `loadAddon`, `dispose`, `cols`, `rows`). Extend the mock so `loadAddon`
records addon instances and the fake `SerializeAddon.serialize()` returns a
sentinel. Match the existing mocking style in the file.

```ts
import { terminalSnapshots } from '$lib/stores/terminal-snapshots';

it('restores a stored snapshot to the terminal on mount before reattach', async () => {
  terminalSnapshots.set('term_restore', 'RESTORED-GRID');
  const { unmount } = render(TerminalPane, {
    props: { workspaceId: 'ws1', terminalId: 'term_restore' },
  });
  await waitForOnMount(); // however the existing tests flush onMount (tick/await)
  expect(lastTerm.write).toHaveBeenCalledWith('RESTORED-GRID');
  // one-shot: snapshot consumed
  expect(terminalSnapshots.take('term_restore')).toBeUndefined();
  unmount();
});

it('serializes the screen into the stash on destroy', async () => {
  fakeSerialize.mockReturnValue('SERIALIZED-SCREEN');
  const { unmount } = render(TerminalPane, {
    props: { workspaceId: 'ws1', terminalId: 'term_save' },
  });
  await waitForOnMount();
  unmount(); // triggers onDestroy
  expect(terminalSnapshots.take('term_save')).toBe('SERIALIZED-SCREEN');
});
```

Adapt helper names (`waitForOnMount`, `lastTerm`, `fakeSerialize`) to whatever
the existing test file already uses; reuse its mock harness rather than adding a
parallel one.

- [ ] **Step 2: Run tests to verify they fail**

Run: `bun run vitest run src/lib/components/workspace/TerminalPane.test.ts`
Expected: FAIL — no `write('RESTORED-GRID')`, stash empty after destroy.

- [ ] **Step 3: Implement in TerminalPane.svelte**

Wire the imports, addon load, restore-before-reattach, and serialize-on-destroy
exactly as described in the task header. Keep `await import(...)` dynamic-import
style consistent with the existing FitAddon load.

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun run vitest run src/lib/components/workspace/TerminalPane.test.ts`
Expected: PASS (existing + 2 new).

- [ ] **Step 5: Wire close → drop the stash**

In `src/lib/components/workspace/Terminal.svelte`, `closeTerminal(id)` should
also call `terminalSnapshots.drop(id)` so a closed terminal's id can't restore a
stale grid if the id is somehow reused. Add an assertion to the Terminal
container test if it already drives close; otherwise a brief unit test that
`closeTerminal` calls `drop`.

- [ ] **Step 6: Run the terminal test suites**

Run:
`bun run vitest run src/lib/components/workspace src/lib/stores/terminal-snapshots.test.ts`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/lib/components/workspace/TerminalPane.svelte src/lib/components/workspace/TerminalPane.test.ts src/lib/components/workspace/Terminal.svelte src/lib/components/workspace/Terminal.test.ts
git commit -m "feat(terminal): serialize screen on destroy, restore on mount"
```

---

## Task 3: Revert Fix B backend raw-byte replay

**Files:**

- Modify: `src-tauri/src/state.rs` (remove `output_buffer` from
  `TerminalHandle`)
- Modify: `src-tauri/src/commands/terminal.rs` (reader-thread append, cap const,
  `reattach_terminal_inner` return shape, `terminal_reattach` snapshot send,
  tests)

The frontend now owns screen restore, so the backend ring buffer is dead weight
and its raw replay is the artifact source. Revert it to the pre-Fix-B shape.

- [ ] **Step 1: Remove the buffer field and constant**

In `state.rs`, delete `output_buffer: Arc<Mutex<Vec<u8>>>` from `TerminalHandle`
and its initialization at the construction site. In `terminal.rs`, delete
`const OUTPUT_BUFFER_CAP` and the reader-thread block that locks
`output_buffer`, `extend_from_slice`, and drains to the cap.

- [ ] **Step 2: Revert reattach to receiver-only**

`reattach_terminal_inner` returns `Result<broadcast::Receiver<TerminalChunk>>`
(not the `(Vec<u8>, Receiver)` tuple). `terminal_reattach` no longer sends a
snapshot `TerminalChunk::Bytes` — it just resolves the receiver and calls
`forward_to_channel`, exactly as before Fix B.

- [ ] **Step 3: Update tests**

Delete the two Fix B tests (`reattach_replays_recent_output_buffer`,
`reattach_terminal_inner_returns_snapshot_and_receiver`). Restore the prior
reattach test that asserts a reattached receiver observes live output written
after re-subscribe (receiver-only shape). Any other caller of
`reattach_terminal_inner` updated to bind a single `rx` (no tuple).

- [ ] **Step 4: Run backend tests + clippy**

Run:
`cd src-tauri && cargo test --lib terminal && cargo clippy --lib --all-targets -- -D warnings`
Expected: terminal tests PASS; clippy clean. Full `cargo test --lib` back to the
pre-Fix-B count minus/plus net test changes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/terminal.rs
git commit -m "refactor(terminal): drop backend output ring buffer (superseded by frontend serialized restore)"
```

---

## Task 4: Full-suite gate + journal

**Files:**

- Modify: `journal/2026-05-27-terminal-multitab.md` (correct the Fix B
  description)

- [ ] **Step 1: Run all gates**

Run: `bun run check && bun run vitest run` (expect green) and
`cd src-tauri && cargo test --lib && cargo clippy --lib --all-targets -- -D warnings`
(expect green).

- [ ] **Step 2: Update the journal's Cross-workspace scrollback bullet**

Rewrite the "Cross-workspace scrollback (Fix B)" bullet to describe the final
mechanism: serialized-screen snapshot on destroy + restore on mount (frontend),
backend reattach is receiver-only. Note the dispose→reattach gap trade-off. Note
that raw-byte replay was tried first and discarded because it garbled
full-screen programs (vite) on remount.

- [ ] **Step 3: Commit**

```bash
git add journal/2026-05-27-terminal-multitab.md
git commit -m "docs(journal): record serialized-snapshot terminal restore"
```

---

## Self-review notes

- Type consistency: `terminalSnapshots.set/take/drop/reset` are the only store
  methods, used identically in Task 1, 2.
- No placeholder: every code step has full code or an exact, adapt-to-existing
  instruction tied to the current test harness.
- Spec coverage: serialize-on-destroy (T2), restore-on-mount (T2), one-shot
  stash (T1), close drops stash (T2.5), backend revert (T3), gates+journal (T4).
- YAGNI: no persistence across app restart, no scrollback cap tuning, no backend
  changes beyond the revert.
