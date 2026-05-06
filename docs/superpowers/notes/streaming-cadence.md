# Streaming cadence — notes for Phase 1e

## What we observe

Spawning Claude with
`-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages`
makes the CLI emit a stream of JSON records on stdout. For a single turn the
sequence we care about is roughly:

1. `system` `init` line — session id and model.
2. `stream_event` `message_start` — establishes a `message_id`.
3. `stream_event` `content_block_delta` × N — each carrying a text fragment; our
   `StreamParser` accumulates these against the message id.
4. `stream_event` `message_stop` — implicit close; the parser then waits for the
   authoritative non-partial line.
5. `assistant` line — full assistant message with the same id; replaces the
   accumulated partial.
6. (Optional) `tool_use` / `tool_result` lines for tool invocations.
7. `result` line on turn completion.

The frontend message store keys by `id`, so each delta upserts the same bubble
and the text grows in place. The streaming indicator (`▍`) is gated on
`is_partial`, so it disappears the moment the final non-partial arrives.

## Real CLI cadence (informal)

Empirically the CLI emits deltas at roughly 5–30 ms intervals on a good
connection, batched to keep packet count manageable. We do **not** depend on
exact timing: the only invariants we care about are

- All deltas for a single turn share one `message_id`.
- Text length is monotonically non-decreasing across deltas.
- A non-partial assistant line with the same id always closes the bubble.

## What the E2E test asserts

`tests/e2e/phase-1e/streaming.spec.ts` exercises the `replyProfile: 'streaming'`
mode of the Tauri shim, which emits 4 partial deltas at 30/60/90/120 ms followed
by a final non-partial at 150 ms — a deterministic stand-in for the cadence
above.

The test polls the assistant bubble's text every 20 ms and asserts:

1. At least 2 distinct length values are observed (proves real streaming, not a
   single drop).
2. Lengths never regress (`L[i] ≥ L[i-1]`).
3. The bubble ends on the final non-partial form
   (`Streaming reply to: hello claude`).
4. The streaming indicator is gone once the bubble closes.

## What the test cannot do

The shim emits structured `AgentEvent` records directly into the page-side
Channel — it bypasses the Rust agent reader, the `StreamParser`, and the
broadcast bridge. So the spec proves the **frontend** renders streaming
correctly; it does **not** prove the parser or transport.

The Rust unit suite (`commands::agent_stream::*`) covers `StreamParser` upserts.
The reader → broadcaster → channel path is covered by the broadcast unit tests
added in Task 1 of this phase.

## Manual smoke (user only)

To validate end-to-end against the real CLI:

1. Build the dev binary (`bun run tauri:dev`).
2. Add a repo, create a workspace, switch to Work mode.
3. Send a multi-paragraph prompt (e.g. "explain how a kernel scheduler works in
   4 paragraphs").
4. Observe the assistant bubble: text should grow chunk-by-chunk, the streaming
   indicator should be visible while partials arrive, and it should disappear
   the moment the bubble closes.

If the bubble appears in one shot, check that `--include-partial-messages` is
still in the CLI argv (`src-tauri/src/commands/agent_core.rs`) — the flag is the
contract that produces `content_block_delta` lines at all.
