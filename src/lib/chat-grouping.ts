import type { Message } from './types';

/** A render slot in the chat list: either a single message bubble or a
 *  collapsible group of consecutive tool calls. */
export type ChatRenderItem =
  | { kind: 'message'; message: Message }
  | { kind: 'group'; id: string; messages: Message[] };

const DEFAULT_TOOL_GROUP_THRESHOLD = 3;

/**
 * Walk `list` and collapse runs of `>= threshold` consecutive `role='tool'`
 * messages into a single `group` slot. Other messages pass through as
 * `message` slots. Group ids are derived from the first and last message
 * id in the run so the value is stable across re-renders for the same
 * input — Svelte's `{#each ... (key)}` keying needs that.
 */
export function groupConsecutiveTools(
  list: Message[],
  threshold: number = DEFAULT_TOOL_GROUP_THRESHOLD
): ChatRenderItem[] {
  const out: ChatRenderItem[] = [];
  let runStart: number | null = null;

  function flushRun(endExclusive: number): void {
    if (runStart === null) return;
    const run = list.slice(runStart, endExclusive);
    if (run.length >= threshold) {
      out.push({
        kind: 'group',
        id: `group:${run[0].id}..${run[run.length - 1].id}`,
        messages: run,
      });
    } else {
      for (const m of run) out.push({ kind: 'message', message: m });
    }
    runStart = null;
  }

  for (let i = 0; i < list.length; i++) {
    if (list[i].role === 'tool') {
      if (runStart === null) runStart = i;
      continue;
    }
    flushRun(i);
    out.push({ kind: 'message', message: list[i] });
  }
  flushRun(list.length);
  return out;
}
