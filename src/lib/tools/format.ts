import type { ToolUse } from '$lib/types';

export interface ToolFormat {
  /** Short visual prefix (emoji or unicode glyph). Always set. */
  icon: string;
  /** Tool family name as shown to the user — usually equal to ToolUse.name. */
  label: string;
  /** One-line summary of what the call targets. Omitted when the tool's
   *  inputs don't yield anything useful (e.g. unknown tool, empty input). */
  detail?: string;
}

const BASH_MAX = 60;

/** Pulls a string field out of an unknown input bag, returning undefined for
 *  anything that isn't a non-empty string. */
function str(input: unknown, key: string): string | undefined {
  if (input == null || typeof input !== 'object') return undefined;
  const v = (input as Record<string, unknown>)[key];
  return typeof v === 'string' && v.length > 0 ? v : undefined;
}

function num(input: unknown, key: string): number | undefined {
  if (input == null || typeof input !== 'object') return undefined;
  const v = (input as Record<string, unknown>)[key];
  return typeof v === 'number' ? v : undefined;
}

function basename(path: string): string {
  // Tools always use POSIX-style paths even on Windows hosts because the
  // Claude CLI normalises file_path before invocation. A single slash split
  // is enough.
  const i = path.lastIndexOf('/');
  return i >= 0 ? path.slice(i + 1) : path;
}

function truncate(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max)}…` : s;
}

export function formatToolUse(tu: ToolUse): ToolFormat {
  switch (tu.name) {
    case 'Read': {
      const path = str(tu.input, 'file_path');
      if (!path) return { icon: '📖', label: 'Read' };
      const offset = num(tu.input, 'offset');
      const limit = num(tu.input, 'limit');
      const range =
        offset !== undefined && limit !== undefined ? `:${offset}-${offset + limit - 1}` : '';
      return { icon: '📖', label: 'Read', detail: `${basename(path)}${range}` };
    }
    case 'Edit':
    case 'MultiEdit': {
      const path = str(tu.input, 'file_path');
      return path
        ? { icon: '✏', label: tu.name, detail: basename(path) }
        : { icon: '✏', label: tu.name };
    }
    case 'Write':
    case 'NotebookEdit': {
      const path = str(tu.input, 'file_path') ?? str(tu.input, 'notebook_path');
      return path
        ? { icon: '📝', label: tu.name, detail: basename(path) }
        : { icon: '📝', label: tu.name };
    }
    case 'Bash':
    case 'BashOutput':
    case 'KillBash': {
      const cmd = str(tu.input, 'command');
      return cmd
        ? { icon: '$', label: tu.name, detail: `$ ${truncate(cmd, BASH_MAX)}` }
        : { icon: '$', label: tu.name };
    }
    case 'Glob': {
      const pattern = str(tu.input, 'pattern');
      return pattern
        ? { icon: '🔍', label: 'Glob', detail: pattern }
        : { icon: '🔍', label: 'Glob' };
    }
    case 'Grep': {
      const pattern = str(tu.input, 'pattern');
      return pattern
        ? { icon: '🔎', label: 'Grep', detail: `"${pattern}"` }
        : { icon: '🔎', label: 'Grep' };
    }
    case 'Task': {
      const detail = str(tu.input, 'description') ?? str(tu.input, 'prompt')?.split('\n')[0];
      return detail ? { icon: '🤖', label: 'Task', detail } : { icon: '🤖', label: 'Task' };
    }
    case 'WebFetch': {
      const url = str(tu.input, 'url');
      return url
        ? { icon: '🌐', label: 'WebFetch', detail: url }
        : { icon: '🌐', label: 'WebFetch' };
    }
    case 'WebSearch': {
      const query = str(tu.input, 'query');
      return query
        ? { icon: '🌐', label: 'WebSearch', detail: `"${query}"` }
        : { icon: '🌐', label: 'WebSearch' };
    }
    case 'TodoWrite': {
      return { icon: '✓', label: 'TodoWrite' };
    }
    default:
      return { icon: '⚙', label: tu.name };
  }
}
