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
});
