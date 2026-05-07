// Phase 2a — unified-diff parser.
//
// Splits a `git diff` text blob into per-file blocks of hunks. A pure
// function so the DiffView can hand it the buffered stream output and
// render typed structures instead of raw lines. Robust against the
// shapes `git diff --no-color HEAD` produces, plus the synthesized
// untracked-file blocks the backend prepends.

export type DiffLineKind = 'add' | 'del' | 'ctx' | 'meta';

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
}

export interface DiffHunk {
  /** Original content of the unchanged hunk header line, e.g.
   *  `@@ -1,3 +1,3 @@ fn main() {`. The trailing context (after the
   *  second `@@`) helps users orient themselves at a glance. */
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface DiffFile {
  /** Best-effort display path. Prefers the `b/` (new) side, falls back
   *  to the `a/` (old) side, then to whatever the parser could pick from
   *  the `diff --git` header. */
  path: string;
  oldPath: string | null;
  newPath: string | null;
  isBinary: boolean;
  hunks: DiffHunk[];
}

export interface ParsedDiff {
  files: DiffFile[];
}

const FILE_HEADER_RE = /^diff --git a\/(.+?) b\/(.+)$/;
const HUNK_HEADER_RE = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

export function parseUnifiedDiff(text: string): ParsedDiff {
  const files: DiffFile[] = [];
  if (!text) return { files };

  const lines = text.split('\n');
  let current: DiffFile | null = null;
  let currentHunk: DiffHunk | null = null;

  const startFile = (oldPath: string | null, newPath: string | null) => {
    current = {
      path: newPath ?? oldPath ?? '<unknown>',
      oldPath,
      newPath,
      isBinary: false,
      hunks: [],
    };
    files.push(current);
    currentHunk = null;
  };

  for (const line of lines) {
    if (line.startsWith('diff --git')) {
      const m = line.match(FILE_HEADER_RE);
      if (m) {
        startFile(m[1], m[2]);
      } else {
        startFile(null, null);
      }
      continue;
    }

    if (current === null) {
      // Pre-header noise (e.g. an empty leading line) is ignored.
      continue;
    }

    if (line.startsWith('Binary file ') && line.endsWith(' not shown')) {
      (current as DiffFile).isBinary = true;
      continue;
    }

    // Re-bind to a const so TS's flow analysis treats the rest of this
    // iteration as `DiffFile`-narrowed without confusing it via the
    // outer `let current: DiffFile | null` declaration that closures
    // (startFile) keep alive.
    const file: DiffFile = current;

    if (line.startsWith('--- ')) {
      const after = line.slice(4).trim();
      file.oldPath = after === '/dev/null' ? null : after.replace(/^a\//, '');
      continue;
    }
    if (line.startsWith('+++ ')) {
      const after = line.slice(4).trim();
      file.newPath = after === '/dev/null' ? null : after.replace(/^b\//, '');
      // Refresh display path now that we know the new side.
      file.path = file.newPath ?? file.oldPath ?? file.path;
      continue;
    }

    if (line.startsWith('@@')) {
      const m = line.match(HUNK_HEADER_RE);
      if (m) {
        currentHunk = {
          header: line,
          oldStart: parseInt(m[1], 10),
          oldLines: m[2] ? parseInt(m[2], 10) : 1,
          newStart: parseInt(m[3], 10),
          newLines: m[4] ? parseInt(m[4], 10) : 1,
          lines: [],
        };
        file.hunks.push(currentHunk);
      } else {
        currentHunk = null;
      }
      continue;
    }

    if (currentHunk === null) {
      // Lines between the file header and the first hunk are metadata
      // (e.g. `index abc123..def456 100644`, `new file mode 100644`).
      // We ignore them — the renderer doesn't need them and they don't
      // map cleanly onto any line kind.
      continue;
    }

    if (line.startsWith('+')) {
      currentHunk.lines.push({ kind: 'add', text: line.slice(1) });
    } else if (line.startsWith('-')) {
      currentHunk.lines.push({ kind: 'del', text: line.slice(1) });
    } else if (line.startsWith(' ')) {
      currentHunk.lines.push({ kind: 'ctx', text: line.slice(1) });
    } else if (line === '\\ No newline at end of file') {
      currentHunk.lines.push({ kind: 'meta', text: line });
    }
    // Unknown leading char: skip silently. Diff text from real `git diff`
    // never produces these, but defensiveness here keeps a malformed
    // synthesized block from sinking the whole render.
  }

  return { files };
}
