// Phase 2c — @-file mention helpers.
//
// `parseMention` detects whether the caret is currently inside an
// active "@<partial>" token; `rankFiles` scores file paths against
// the typed query. Both are pure — no DOM, no IPC, no Svelte runes.
// Heavy unit-test coverage lives in `mentions.test.ts`.

export interface MentionMatch {
  /** Index of the `@` character in the original text. */
  start: number;
  /** Characters typed after the `@`, up to the caret. May be empty. */
  query: string;
}

/**
 * Inspect the substring of `text` ending at `caret` and return a
 * mention match if the caret is currently inside an `@<partial>`
 * token. The `@` must be at the start of the text OR preceded by
 * whitespace — `email@host` does NOT match.
 *
 * Returns null if no mention is in progress.
 */
export function parseMention(text: string, caret: number): MentionMatch | null {
  if (caret < 1 || caret > text.length) return null;
  const slice = text.slice(0, caret);
  // `(^|\s)` — start of slice OR a whitespace char (space, tab, newline).
  // `@` — literal trigger.
  // `([^\s]*)` — query: zero or more non-whitespace.
  // `$` — anchored to the end of the slice (= caret position).
  const m = slice.match(/(^|\s)@([^\s]*)$/);
  if (!m) return null;
  const leading = m[1]; // empty when matched at start-of-text
  const query = m[2];
  const matchIndex = m.index ?? 0;
  const start = matchIndex + leading.length;
  return { start, query };
}

/**
 * Rank `files` against a (possibly empty) query. Returns up to `limit`
 * paths in score-descending order. Ties keep the original input order
 * (stable). The query is matched case-insensitively as a substring;
 * matches in the basename outrank matches in directory segments, and
 * matches at the start of a segment outrank mid-segment matches.
 */
export function rankFiles(files: string[], query: string, limit = 20): string[] {
  if (query === '') {
    return files.slice(0, limit);
  }
  const q = query.toLowerCase();
  // Pre-score each path. tieBreak = original index so stable sort
  // works even though JS doesn't guarantee stability historically (it
  // does in ES2019+ but we don't rely on the JS engine — we encode
  // the tie-break explicitly).
  const scored = files
    .map((path, idx) => ({ path, score: scoreMatch(path, q), tieBreak: idx }))
    .filter((s) => s.score > 0);
  scored.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.tieBreak - b.tieBreak;
  });
  return scored.slice(0, limit).map((s) => s.path);
}

/**
 * Compute a match score for `path` against `query`. Both are expected
 * lowercased by the caller (rankFiles enforces). Returns 0 when there
 * is no substring match; otherwise a positive number, higher = better.
 *
 * Heuristic:
 *   - Path must contain query as substring → base score.
 *   - Match inside basename adds a large bonus.
 *   - Basename starting with query adds another bonus.
 *   - Exact basename match is the largest bonus.
 *   - Match at a segment start (start-of-path or after '/') adds a
 *     smaller bonus.
 *   - Shorter paths get a tiny edge as tie-breaker.
 */
function scoreMatch(rawPath: string, query: string): number {
  const path = rawPath.toLowerCase();
  const idx = path.indexOf(query);
  if (idx < 0) return 0;

  let score = 100;

  const lastSlash = path.lastIndexOf('/');
  const basename = lastSlash >= 0 ? path.slice(lastSlash + 1) : path;
  const baseIdx = basename.indexOf(query);
  if (baseIdx >= 0) {
    score += 50;
    if (basename === query) {
      score += 200; // exact basename match — strongest signal
    } else if (baseIdx === 0) {
      score += 50; // basename starts with query
    }
  }

  // Segment-start match (start-of-path or right after '/').
  if (idx === 0 || path[idx - 1] === '/') {
    score += 20;
  }

  // Slight penalty for longer paths so equal-quality matches prefer
  // the shorter file.
  score -= path.length * 0.1;

  return score;
}
