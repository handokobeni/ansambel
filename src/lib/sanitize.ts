// src/lib/sanitize.ts
//
// Frontend mirror of `src-tauri/src/sanitize.rs`. Redacts common credential
// patterns before message previews leave the frontend (e.g., before being
// persisted to `messages.jsonl` from the backend). The backend's
// `sanitize_message_preview` (Task 2) is the second line of defence — the two
// must stay in sync.
//
// Order matters: API_KEY → BEARER → JWT → NAMED, identical to Rust. The
// NAMED carve-out `[^\[\s]\S*` (not `\S+`) prevents double-redaction of
// `[REDACTED-API-KEY]` markers an earlier rule may have written.

const API_KEY = /sk-[A-Za-z0-9_-]{20,}/g;
const BEARER = /Bearer\s+[A-Za-z0-9._-]+/g;
const JWT = /eyJ[A-Za-z0-9._-]{20,}/g;
// `[^[\s]\S*` ensures we don't re-match an already-redacted `[REDACTED…]`
// marker left behind by an earlier rule (API_KEY / BEARER / JWT all run
// first and may have rewritten the value into a `[…]` bracket form).
const NAMED = /\b(api[_-]?key|secret|password|token)\s*[:=]\s*[^[\s]\S*/gi;

/**
 * Redacts common credential patterns from `input`, then truncates the result
 * to `maxChars` Unicode code points, appending `…` when truncated.
 *
 * Behaviour mirrors the Rust `sanitize_message_preview` byte-for-byte:
 *   - `sk-[A-Za-z0-9_-]{20,}` → `[REDACTED-API-KEY]`
 *   - `Bearer\s+[A-Za-z0-9._-]+` → `Bearer [REDACTED]`
 *   - `eyJ[A-Za-z0-9._-]{20,}` → `[REDACTED-JWT]`
 *   - `\b(api[_-]?key|secret|password|token)\s*[:=]\s*[^\[\s]\S*` (case-insensitive)
 *     → `<name>: [REDACTED]`
 *
 * Truncation runs AFTER all redactions so a long secret prefix doesn't push
 * the trailing context out of view.
 */
export function sanitizeMessagePreview(input: string, maxChars: number): string {
  let s = input.replace(API_KEY, '[REDACTED-API-KEY]');
  s = s.replace(BEARER, 'Bearer [REDACTED]');
  s = s.replace(JWT, '[REDACTED-JWT]');
  s = s.replace(NAMED, (_match, name: string) => `${name}: [REDACTED]`);
  return truncateChars(s, maxChars);
}

/**
 * Unicode-safe truncation. Iterates over code points (not UTF-16 code units),
 * so a surrogate-pair character at the boundary is never split. Appends `…`
 * iff the string was actually truncated.
 *
 * `[...str]` and `Array.from(str)` both iterate by code point. We use the
 * spread form because it's the marginally more idiomatic shape.
 */
function truncateChars(s: string, maxChars: number): string {
  const chars = [...s];
  if (chars.length <= maxChars) {
    return s;
  }
  return chars.slice(0, maxChars).join('') + '…';
}
