import { describe, it, expect, beforeEach } from 'vitest';
import { messages } from './messages.svelte';
import type { Message } from '../types';

const make = (id: string, ws = 'ws_a', text = 'body'): Message => ({
  id,
  workspace_id: ws,
  role: 'assistant',
  text,
  is_partial: false,
  tool_use: null,
  tool_result: null,
  created_at: 0,
});

describe('MessagesStore', () => {
  beforeEach(() => {
    messages.reset();
  });

  it('starts empty', () => {
    expect(messages.listForWorkspace('ws_a')).toEqual([]);
  });

  it('upsert adds a new message', () => {
    messages.upsert(make('msg_a'));
    expect(messages.listForWorkspace('ws_a')).toHaveLength(1);
  });

  it('upsert updates an existing message in place', () => {
    messages.upsert(make('msg_a', 'ws_a', 'first'));
    messages.upsert(make('msg_a', 'ws_a', 'second'));
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].text).toBe('second');
  });

  it('listForWorkspace sorts by created_at ascending', () => {
    messages.upsert({ ...make('msg_2'), created_at: 200 });
    messages.upsert({ ...make('msg_1'), created_at: 100 });
    messages.upsert({ ...make('msg_3'), created_at: 300 });
    const list = messages.listForWorkspace('ws_a');
    expect(list.map((m) => m.id)).toEqual(['msg_1', 'msg_2', 'msg_3']);
  });

  it('separates messages by workspace', () => {
    messages.upsert(make('msg_a', 'ws_x'));
    messages.upsert(make('msg_b', 'ws_y'));
    expect(messages.listForWorkspace('ws_x')).toHaveLength(1);
    expect(messages.listForWorkspace('ws_y')).toHaveLength(1);
  });

  it('apply Init event is a no-op for messages', () => {
    messages.apply({ type: 'init', session_id: 'ses_a', model: 'claude' }, 'ws_a');
    expect(messages.listForWorkspace('ws_a')).toEqual([]);
  });

  it('apply Message event upserts assistant message', () => {
    messages.apply(
      {
        type: 'message',
        id: 'msg_a',
        role: 'assistant',
        text: 'hello',
        is_partial: false,
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].text).toBe('hello');
    expect(list[0].is_partial).toBe(false);
  });

  it('apply Status event sets status without touching messages', () => {
    messages.apply({ type: 'status', status: 'running' }, 'ws_a');
    expect(messages.statusFor('ws_a')).toBe('running');
    expect(messages.listForWorkspace('ws_a')).toEqual([]);
  });

  it('apply Error event captures latest error string', () => {
    messages.apply({ type: 'error', message: 'spawn failed' }, 'ws_a');
    expect(messages.errorFor('ws_a')).toBe('spawn failed');
  });

  it('apply ToolUse attaches tool_use to existing message', () => {
    messages.apply(
      {
        type: 'message',
        id: 'msg_a',
        role: 'assistant',
        text: 'using tool',
        is_partial: false,
      },
      'ws_a'
    );
    messages.apply(
      {
        type: 'tool_use',
        message_id: 'msg_a',
        tool_use: {
          id: 'toolu_a',
          name: 'Read',
          input: { path: '/etc/hosts' },
        },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list[0].tool_use).toEqual({
      id: 'toolu_a',
      name: 'Read',
      input: { path: '/etc/hosts' },
    });
  });

  it('apply ToolResult creates a tool message', () => {
    messages.apply(
      {
        type: 'tool_result',
        message_id: 'msg_r',
        tool_result: { tool_use_id: 'toolu_a', content: 'ok', is_error: false },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].role).toBe('tool');
    expect(list[0].tool_result?.content).toBe('ok');
  });

  it('reset clears all workspaces', () => {
    messages.upsert(make('msg_a', 'ws_x'));
    messages.upsert(make('msg_b', 'ws_y'));
    messages.reset();
    expect(messages.listForWorkspace('ws_x')).toEqual([]);
    expect(messages.listForWorkspace('ws_y')).toEqual([]);
  });
});
