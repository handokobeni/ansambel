import { describe, it, expect } from 'vitest';
import { parseUnifiedDiff } from './diff';

describe('parseUnifiedDiff', () => {
  it('handles empty input', () => {
    const out = parseUnifiedDiff('');
    expect(out.files).toEqual([]);
  });

  it('parses a single file with one hunk', () => {
    const text = [
      'diff --git a/foo.txt b/foo.txt',
      'index 0123456..789abcd 100644',
      '--- a/foo.txt',
      '+++ b/foo.txt',
      '@@ -1,1 +1,1 @@',
      '-old',
      '+new',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    expect(out.files).toHaveLength(1);
    const f = out.files[0];
    expect(f.path).toBe('foo.txt');
    expect(f.oldPath).toBe('foo.txt');
    expect(f.newPath).toBe('foo.txt');
    expect(f.isBinary).toBe(false);
    expect(f.hunks).toHaveLength(1);
    const h = f.hunks[0];
    expect(h.oldStart).toBe(1);
    expect(h.oldLines).toBe(1);
    expect(h.newStart).toBe(1);
    expect(h.newLines).toBe(1);
    expect(h.lines).toEqual([
      { kind: 'del', text: 'old' },
      { kind: 'add', text: 'new' },
    ]);
  });

  it('parses multi-file diff and surfaces both paths', () => {
    const text = [
      'diff --git a/a.ts b/a.ts',
      '--- a/a.ts',
      '+++ b/a.ts',
      '@@ -1 +1 @@',
      '-let x = 1;',
      '+let x = 2;',
      'diff --git a/b.ts b/b.ts',
      '--- a/b.ts',
      '+++ b/b.ts',
      '@@ -10,1 +10,2 @@',
      ' const y = 3;',
      '+const z = 4;',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    expect(out.files.map((f) => f.path)).toEqual(['a.ts', 'b.ts']);
    expect(out.files[1].hunks[0].oldStart).toBe(10);
    expect(out.files[1].hunks[0].newLines).toBe(2);
    // Context lines preserved.
    expect(out.files[1].hunks[0].lines[0]).toEqual({ kind: 'ctx', text: 'const y = 3;' });
  });

  it('marks binary files via the "not shown" marker', () => {
    const text = ['diff --git a/img.png b/img.png', 'Binary file img.png not shown', ''].join('\n');
    const out = parseUnifiedDiff(text);
    expect(out.files).toHaveLength(1);
    expect(out.files[0].isBinary).toBe(true);
    expect(out.files[0].hunks).toHaveLength(0);
  });

  it('handles untracked-file synthesized blocks', () => {
    // The backend synthesizes these for `git ls-files --others`
    const text = [
      'diff --git a/untracked.txt b/untracked.txt',
      'new file mode 100644',
      '--- /dev/null',
      '+++ b/untracked.txt',
      '@@ -0,0 +1,2 @@',
      '+hello',
      '+world',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    expect(out.files).toHaveLength(1);
    expect(out.files[0].oldPath).toBeNull(); // /dev/null
    expect(out.files[0].newPath).toBe('untracked.txt');
    expect(out.files[0].path).toBe('untracked.txt');
    expect(out.files[0].hunks[0].lines).toEqual([
      { kind: 'add', text: 'hello' },
      { kind: 'add', text: 'world' },
    ]);
  });

  it('captures the no-newline-at-eof meta line', () => {
    const text = [
      'diff --git a/foo b/foo',
      '--- a/foo',
      '+++ b/foo',
      '@@ -1 +1 @@',
      '-old',
      '+new',
      '\\ No newline at end of file',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    const lines = out.files[0].hunks[0].lines;
    expect(lines.at(-1)).toEqual({
      kind: 'meta',
      text: '\\ No newline at end of file',
    });
  });

  it('treats content before the first diff header as no-op', () => {
    // Defensive: extra noise before `diff --git` should not throw.
    const text = ['warning: some preamble', '', 'diff --git a/x b/x', '--- a/x', '+++ b/x'].join(
      '\n'
    );
    const out = parseUnifiedDiff(text);
    expect(out.files).toHaveLength(1);
    expect(out.files[0].path).toBe('x');
  });

  it('falls back when hunk header is malformed', () => {
    const text = [
      'diff --git a/foo b/foo',
      '--- a/foo',
      '+++ b/foo',
      '@@ totally not a hunk header @@',
      '+ignored', // dropped because no current hunk
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    // File is still recorded but with no hunks.
    expect(out.files).toHaveLength(1);
    expect(out.files[0].hunks).toHaveLength(0);
  });

  it('handles a malformed `diff --git` header by recording a placeholder file', () => {
    // Defensive: a header that doesn't match `a/<x> b/<y>` shouldn't throw.
    // Subsequent --- / +++ lines still recover the old/new paths.
    const text = [
      'diff --git missing-prefix',
      '--- a/foo',
      '+++ b/foo',
      '@@ -1 +1 @@',
      '-x',
      '+y',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    expect(out.files).toHaveLength(1);
    expect(out.files[0].path).toBe('foo');
  });

  it('sets newPath to null for deleted file (+++ /dev/null)', () => {
    const text = [
      'diff --git a/deleted.txt b/deleted.txt',
      'deleted file mode 100644',
      '--- a/deleted.txt',
      '+++ /dev/null',
      '@@ -1,2 +0,0 @@',
      '-hello',
      '-world',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    expect(out.files).toHaveLength(1);
    expect(out.files[0].newPath).toBeNull();
    expect(out.files[0].oldPath).toBe('deleted.txt');
    expect(out.files[0].path).toBe('deleted.txt');
  });

  it('omits oldLines/newLines defaults of 1 when shorthand is used', () => {
    const text = [
      'diff --git a/foo b/foo',
      '--- a/foo',
      '+++ b/foo',
      '@@ -5 +5 @@',
      '-x',
      '+y',
      '',
    ].join('\n');
    const out = parseUnifiedDiff(text);
    const h = out.files[0].hunks[0];
    expect(h.oldStart).toBe(5);
    expect(h.oldLines).toBe(1);
    expect(h.newStart).toBe(5);
    expect(h.newLines).toBe(1);
  });
});
