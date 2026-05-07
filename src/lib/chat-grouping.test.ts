import { describe, it, expect } from 'vitest';
import { groupConsecutiveTools, type ChatRenderItem } from './chat-grouping';
import type { Message } from './types';

function msg(id: string, role: Message['role'], extra: Partial<Message> = {}): Message {
  return {
    id,
    workspace_id: 'ws',
    role,
    text: '',
    is_partial: false,
    tool_use: null,
    tool_result: null,
    created_at: 0,
    ...extra,
  };
}

function ids(items: ChatRenderItem[]): string[] {
  return items.map((it) =>
    it.kind === 'message'
      ? `msg:${it.message.id}`
      : `group:${it.messages.map((m) => m.id).join(',')}`
  );
}

describe('groupConsecutiveTools', () => {
  it('returns an empty array for an empty list', () => {
    expect(groupConsecutiveTools([])).toEqual([]);
  });

  it('passes lone messages through as message items', () => {
    const list = [msg('a', 'user'), msg('b', 'assistant')];
    expect(ids(groupConsecutiveTools(list))).toEqual(['msg:a', 'msg:b']);
  });

  it('keeps runs below the threshold as individual messages', () => {
    const list = [msg('t1', 'tool'), msg('t2', 'tool')];
    // Default threshold = 3 → two consecutive tools render individually.
    expect(ids(groupConsecutiveTools(list))).toEqual(['msg:t1', 'msg:t2']);
  });

  it('groups runs of >= threshold consecutive tool messages', () => {
    const list = [msg('t1', 'tool'), msg('t2', 'tool'), msg('t3', 'tool')];
    expect(ids(groupConsecutiveTools(list))).toEqual(['group:t1,t2,t3']);
  });

  it('keeps text messages out of groups', () => {
    const list = [
      msg('u', 'user'),
      msg('t1', 'tool'),
      msg('t2', 'tool'),
      msg('t3', 'tool'),
      msg('a', 'assistant'),
    ];
    expect(ids(groupConsecutiveTools(list))).toEqual(['msg:u', 'group:t1,t2,t3', 'msg:a']);
  });

  it('breaks groups when a non-tool message interrupts the run', () => {
    const list = [
      msg('t1', 'tool'),
      msg('t2', 'tool'),
      msg('a', 'assistant'),
      msg('t3', 'tool'),
      msg('t4', 'tool'),
      msg('t5', 'tool'),
    ];
    expect(ids(groupConsecutiveTools(list))).toEqual([
      'msg:t1',
      'msg:t2',
      'msg:a',
      'group:t3,t4,t5',
    ]);
  });

  it('emits multiple groups when separated by a non-tool', () => {
    const list = [
      msg('t1', 'tool'),
      msg('t2', 'tool'),
      msg('t3', 'tool'),
      msg('a', 'assistant'),
      msg('t4', 'tool'),
      msg('t5', 'tool'),
      msg('t6', 'tool'),
    ];
    expect(ids(groupConsecutiveTools(list))).toEqual(['group:t1,t2,t3', 'msg:a', 'group:t4,t5,t6']);
  });

  it('respects a custom threshold', () => {
    const list = [msg('t1', 'tool'), msg('t2', 'tool')];
    expect(ids(groupConsecutiveTools(list, 2))).toEqual(['group:t1,t2']);
  });

  it('group ids are stable between calls for the same input', () => {
    const list = [msg('t1', 'tool'), msg('t2', 'tool'), msg('t3', 'tool')];
    const a = groupConsecutiveTools(list);
    const b = groupConsecutiveTools(list);
    expect(a[0].kind === 'group' && b[0].kind === 'group').toBe(true);
    if (a[0].kind === 'group' && b[0].kind === 'group') {
      expect(a[0].id).toBe(b[0].id);
    }
  });
});
