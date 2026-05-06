import { describe, it, expect } from 'vitest';
import { formatToolUse } from './format';
import type { ToolUse } from '$lib/types';

const tu = (name: string, input: unknown): ToolUse => ({
  id: 'toolu_x',
  name,
  input,
});

describe('formatToolUse', () => {
  it('formats Read with file_path and a line range', () => {
    const out = formatToolUse(tu('Read', { file_path: '/repo/src/foo.ts', offset: 1, limit: 50 }));
    expect(out.label).toBe('Read');
    expect(out.detail).toBe('foo.ts:1-50');
  });

  it('formats Read without offset/limit as plain basename', () => {
    const out = formatToolUse(tu('Read', { file_path: '/repo/src/foo.ts' }));
    expect(out.detail).toBe('foo.ts');
  });

  it('formats Edit with file basename', () => {
    const out = formatToolUse(
      tu('Edit', { file_path: '/repo/x/y/comp.svelte', old_string: 'a', new_string: 'b' })
    );
    expect(out.label).toBe('Edit');
    expect(out.detail).toBe('comp.svelte');
  });

  it('formats Write with file basename', () => {
    const out = formatToolUse(tu('Write', { file_path: '/tmp/log.txt', content: '...' }));
    expect(out.label).toBe('Write');
    expect(out.detail).toBe('log.txt');
  });

  it('formats Bash with the command, truncated past 60 chars', () => {
    const cmd = 'echo hello';
    const out = formatToolUse(tu('Bash', { command: cmd, description: 'greet' }));
    expect(out.label).toBe('Bash');
    expect(out.detail).toBe('$ echo hello');
  });

  it('truncates long Bash commands with an ellipsis', () => {
    const long = 'a'.repeat(120);
    const out = formatToolUse(tu('Bash', { command: long }));
    expect(out.detail!.length).toBeLessThanOrEqual(64); // "$ " + 60 + "…"
    expect(out.detail!.endsWith('…')).toBe(true);
  });

  it('formats Glob with the pattern', () => {
    const out = formatToolUse(tu('Glob', { pattern: '**/*.ts' }));
    expect(out.label).toBe('Glob');
    expect(out.detail).toBe('**/*.ts');
  });

  it('formats Grep with the search query', () => {
    const out = formatToolUse(tu('Grep', { pattern: 'fn main', path: '/repo' }));
    expect(out.label).toBe('Grep');
    expect(out.detail).toBe('"fn main"');
  });

  it('formats Task with the description (or first line of prompt)', () => {
    const out = formatToolUse(
      tu('Task', {
        description: 'Audit auth flow',
        subagent_type: 'general-purpose',
        prompt: '...',
      })
    );
    expect(out.label).toBe('Task');
    expect(out.detail).toBe('Audit auth flow');
  });

  it('falls back to the tool name when input is unrecognised', () => {
    const out = formatToolUse(tu('FrobulateWidget', { knob: 1 }));
    expect(out.label).toBe('FrobulateWidget');
    expect(out.detail).toBeUndefined();
  });

  it('handles null/empty input without throwing', () => {
    const out = formatToolUse(tu('Read', null));
    expect(out.label).toBe('Read');
    expect(out.detail).toBeUndefined();
  });

  it('returns an icon string per tool family', () => {
    expect(formatToolUse(tu('Read', null)).icon).toBeTruthy();
    expect(formatToolUse(tu('Edit', null)).icon).toBeTruthy();
    expect(formatToolUse(tu('Bash', null)).icon).toBeTruthy();
    expect(formatToolUse(tu('Mystery', null)).icon).toBeTruthy(); // fallback icon
  });

  // The remaining tests cover the no-input / fallback branches across every
  // tool family, plus aliases (MultiEdit / NotebookEdit / BashOutput /
  // KillBash) so a refactor of the dispatch table can't silently drop them.

  it('Read without file_path returns no detail', () => {
    expect(formatToolUse(tu('Read', { offset: 0, limit: 1 })).detail).toBeUndefined();
  });

  it('Read with offset but no limit omits the range suffix', () => {
    const out = formatToolUse(tu('Read', { file_path: '/x/foo.ts', offset: 5 }));
    expect(out.detail).toBe('foo.ts');
  });

  it('formats MultiEdit with file basename, falls back without path', () => {
    expect(formatToolUse(tu('MultiEdit', { file_path: '/a/b.ts', edits: [] })).detail).toBe('b.ts');
    expect(formatToolUse(tu('MultiEdit', {})).detail).toBeUndefined();
  });

  it('formats NotebookEdit using notebook_path when file_path is absent', () => {
    expect(
      formatToolUse(tu('NotebookEdit', { notebook_path: '/x/y.ipynb', new_source: '' })).detail
    ).toBe('y.ipynb');
    expect(formatToolUse(tu('NotebookEdit', {})).detail).toBeUndefined();
  });

  it('formats Write without file_path returns no detail', () => {
    expect(formatToolUse(tu('Write', {})).detail).toBeUndefined();
  });

  it('formats BashOutput and KillBash like Bash', () => {
    expect(formatToolUse(tu('BashOutput', { command: 'ls' })).detail).toBe('$ ls');
    expect(formatToolUse(tu('KillBash', {})).detail).toBeUndefined();
  });

  it('Bash without command returns no detail', () => {
    expect(formatToolUse(tu('Bash', {})).detail).toBeUndefined();
  });

  it('Glob without pattern returns no detail', () => {
    expect(formatToolUse(tu('Glob', {})).detail).toBeUndefined();
  });

  it('Grep without pattern returns no detail', () => {
    expect(formatToolUse(tu('Grep', {})).detail).toBeUndefined();
  });

  it('Task without description falls back to first line of prompt', () => {
    const out = formatToolUse(tu('Task', { prompt: 'first line\nsecond line' }));
    expect(out.detail).toBe('first line');
  });

  it('Task with neither description nor prompt returns no detail', () => {
    expect(formatToolUse(tu('Task', {})).detail).toBeUndefined();
  });

  it('formats WebFetch with the url, omits when missing', () => {
    expect(formatToolUse(tu('WebFetch', { url: 'https://example.com' })).detail).toBe(
      'https://example.com'
    );
    expect(formatToolUse(tu('WebFetch', {})).detail).toBeUndefined();
  });

  it('formats WebSearch with the query, omits when missing', () => {
    expect(formatToolUse(tu('WebSearch', { query: 'rust async' })).detail).toBe('"rust async"');
    expect(formatToolUse(tu('WebSearch', {})).detail).toBeUndefined();
  });

  it('TodoWrite has a label and icon but no detail', () => {
    const out = formatToolUse(tu('TodoWrite', { todos: [] }));
    expect(out.label).toBe('TodoWrite');
    expect(out.detail).toBeUndefined();
  });
});
