// src/lib/sanitize.test.ts
//
// Mirror of `src-tauri/src/sanitize.rs#tests` — each Rust case has a TS twin
// so the two implementations stay in lock-step. If you add a case here,
// add the equivalent there (and vice versa).

import { describe, it, expect } from 'vitest';

import { sanitizeMessagePreview } from './sanitize';

describe('sanitizeMessagePreview', () => {
  it('redacts openai-style api key', () => {
    const s = 'Token: sk-proj-abcdefghijklmnopqrstuvwx';
    expect(sanitizeMessagePreview(s, 200)).toBe('Token: [REDACTED-API-KEY]');
  });

  it('redacts bearer token', () => {
    const s = 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9';
    const out = sanitizeMessagePreview(s, 200);
    expect(out).toContain('Bearer [REDACTED]');
  });

  it('redacts named credentials case-insensitively', () => {
    const s = 'api_key: secret123 and password = topsecret';
    const out = sanitizeMessagePreview(s, 200);
    expect(out).not.toContain('secret123');
    expect(out).not.toContain('topsecret');
  });

  it('truncates to max chars with ellipsis', () => {
    const s = 'a'.repeat(500);
    const out = sanitizeMessagePreview(s, 200);
    // 200 chars + 1 ellipsis = 201 code points
    expect([...out].length).toBe(201);
    expect(out.endsWith('…')).toBe(true);
  });

  it('passes short strings through untruncated', () => {
    const s = 'Hello world';
    expect(sanitizeMessagePreview(s, 200)).toBe('Hello world');
  });

  it('redacts multiple credentials in one message (NAMED carve-out)', () => {
    // Verifies the NAMED `[^\[\s]\S*` carve-out: after API_KEY rewrites the
    // sk-... value to [REDACTED-API-KEY], NAMED must NOT match that marker
    // (its value class excludes leading `[`), so the more specific marker is
    // preserved AND the original separator `=` is left intact.
    const s = 'call with Bearer abc.def and api_key=sk-proj-aaaaaaaaaaaaaaaaaaaa';
    const out = sanitizeMessagePreview(s, 200);
    expect(out).toBe('call with Bearer [REDACTED] and api_key=[REDACTED-API-KEY]');
  });

  it('truncation runs after redaction', () => {
    // A long Bearer prefix is fully redacted before length is measured, so
    // the trailing " tail" should still appear in the output. If truncation
    // ran first, "tail" would be lost.
    const s = `Bearer ${'a'.repeat(500)} tail`;
    const out = sanitizeMessagePreview(s, 50);
    expect(out).toContain('Bearer [REDACTED]');
    expect(out).toContain('tail');
  });

  it('returns empty string for empty input', () => {
    expect(sanitizeMessagePreview('', 200)).toBe('');
  });

  it('named keyword only matches at word boundary', () => {
    // "secret" is part of "mysecret" / "username=secret123" — the \b in NAMED
    // must prevent the keyword from being recognised mid-word.
    const s = 'Set username=secret123 and mysecret: foo';
    const out = sanitizeMessagePreview(s, 200);
    expect(out).toBe('Set username=secret123 and mysecret: foo');
  });

  it('named redaction preserves keyword case and format', () => {
    // Locks in the exact output shape — would have failed under the
    // original `\S+` regex (would have produced double redactions).
    const s = 'api_key: secret123 and password = topsecret';
    const out = sanitizeMessagePreview(s, 200);
    expect(out).toBe('api_key: [REDACTED] and password: [REDACTED]');
  });
});
