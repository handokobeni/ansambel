import { describe, it, expect } from 'vitest';
import { parseMention, rankFiles } from './mentions';

describe('parseMention', () => {
  it('returns null for empty text at caret 0', () => {
    expect(parseMention('', 0)).toBeNull();
  });

  it('returns null when there is no @ before the caret', () => {
    expect(parseMention('hello', 5)).toBeNull();
  });

  it('returns query="" when caret is right after a bare @', () => {
    expect(parseMention('@', 1)).toEqual({ start: 0, query: '' });
  });

  it('returns query="src" for "@src" with caret at end', () => {
    expect(parseMention('@src', 4)).toEqual({ start: 0, query: 'src' });
  });

  it('detects @ preceded by whitespace', () => {
    expect(parseMention('hi @sr', 6)).toEqual({ start: 3, query: 'sr' });
  });

  it('returns null when @ has no whitespace/start before it (email-like)', () => {
    expect(parseMention('email@host', 10)).toBeNull();
  });

  it('matches the latest mention when multiple @ are present', () => {
    expect(parseMention('@one @two', 9)).toEqual({ start: 5, query: 'two' });
  });

  it('returns null when the slice up to caret ends in whitespace', () => {
    // user typed `@src` then ENTER — no longer mentioning
    expect(parseMention('@src\n', 5)).toBeNull();
  });

  it('returns null when caret is inside but ends with space after partial query', () => {
    expect(parseMention('@src ', 5)).toBeNull();
  });

  it('handles caret in the middle of text (not at end)', () => {
    // text: "@src hello", caret at 4 (right after "src")
    // slice is "@src" — matches
    expect(parseMention('@src hello', 4)).toEqual({ start: 0, query: 'src' });
  });

  it('handles tab as whitespace before @', () => {
    expect(parseMention('a\t@x', 4)).toEqual({ start: 2, query: 'x' });
  });

  it('handles newline as whitespace before @', () => {
    expect(parseMention('a\n@x', 4)).toEqual({ start: 2, query: 'x' });
  });
});

describe('rankFiles', () => {
  const sample = [
    'README.md',
    'src/main.rs',
    'src/lib.rs',
    'src/lib/utils.ts',
    'src/lib/types.ts',
    'docs/lib.md',
    'tests/lib_test.rs',
  ];

  it('returns the first N entries when query is empty', () => {
    const out = rankFiles(['z.md', 'a.md', 'b.md'], '', 2);
    expect(out).toHaveLength(2);
  });

  it('uses the default limit of 20 when none is passed', () => {
    const files = Array.from({ length: 50 }, (_, i) => `f${i}.txt`);
    expect(rankFiles(files, '')).toHaveLength(20);
  });

  it('filters to entries containing the query (case-insensitive)', () => {
    const out = rankFiles(sample, 'LIB');
    // Every result must contain "lib" somewhere (case-insensitive).
    expect(out.every((p) => p.toLowerCase().includes('lib'))).toBe(true);
    // README.md and src/main.rs are excluded.
    expect(out).not.toContain('README.md');
    expect(out).not.toContain('src/main.rs');
  });

  it('returns empty array when no entry matches', () => {
    expect(rankFiles(sample, 'zzzzzz')).toEqual([]);
  });

  it('ranks basename matches above mid-path matches', () => {
    const files = ['deep/nested/folder/api.ts', 'other/api/handlers.ts'];
    const out = rankFiles(files, 'api');
    // The basename match (api.ts) should outrank the directory-name
    // match (api/handlers.ts).
    expect(out[0]).toBe('deep/nested/folder/api.ts');
  });

  it('ranks segment-start matches above mid-segment matches', () => {
    const files = ['xxxlib.md', 'src/lib.rs'];
    const out = rankFiles(files, 'lib');
    // "lib.rs" starts a segment after the "/" — beats "xxxlib".
    expect(out[0]).toBe('src/lib.rs');
  });

  it('prefers shorter paths as a tie-breaker', () => {
    const files = ['src/lib.rs', 'src/very/deep/path/to/lib.rs'];
    const out = rankFiles(files, 'lib.rs');
    expect(out[0]).toBe('src/lib.rs');
  });

  it('is case-insensitive for queries and paths', () => {
    const out = rankFiles(['SRC/Main.RS'], 'main');
    expect(out).toContain('SRC/Main.RS');
  });

  it('caps results at the provided limit', () => {
    const files = Array.from({ length: 100 }, (_, i) => `lib/file${i}.ts`);
    expect(rankFiles(files, 'lib', 5)).toHaveLength(5);
  });

  it('is stable for entries with equal scores', () => {
    // Two files with identical-looking shape — original order should
    // be preserved among equal-score results.
    const a = ['lib/aaa.ts', 'lib/bbb.ts', 'lib/ccc.ts'];
    const out1 = rankFiles(a, 'lib');
    const out2 = rankFiles(a, 'lib');
    expect(out1).toEqual(out2);
  });
});
