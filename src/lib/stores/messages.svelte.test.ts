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

  it('apply Compact event inserts a synthetic system marker into the message list', () => {
    // Use a clearly-earlier created_at so the marker sorts after this turn.
    messages.upsert({
      id: 'msg_pre',
      workspace_id: 'ws_a',
      role: 'assistant',
      text: 'old reply',
      is_partial: false,
      tool_use: null,
      tool_result: null,
      created_at: 100,
    });
    messages.apply({ type: 'compact', trigger: 'auto', pre_tokens: 45000 }, 'ws_a');
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(2);
    const marker = list.find((m) => m.role === 'system');
    expect(marker).toBeTruthy();
    expect(marker!.text).toMatch(/compact/i);
    // Token count rounded to k for readability.
    expect(marker!.text).toMatch(/45k|45,000/);
  });

  it('apply Compact event without pre_tokens still produces a marker', () => {
    messages.apply({ type: 'compact', trigger: 'manual', pre_tokens: null }, 'ws_a');
    const list = messages.listForWorkspace('ws_a');
    expect(list.find((m) => m.role === 'system')).toBeTruthy();
  });

  it('apply Thinking event upserts a thinking-style marker in the list', () => {
    messages.apply(
      { type: 'thinking', message_id: 'msg_t', text: 'Inspecting auth flow', is_partial: false },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    const marker = list.find((m) => m.id.startsWith('thinking_msg_t'));
    expect(marker).toBeTruthy();
    expect(marker!.role).toBe('system');
    expect(marker!.text).toMatch(/inspecting auth flow/i);
  });

  it('apply Thinking partial updates the same marker in place', () => {
    messages.apply(
      { type: 'thinking', message_id: 'msg_t2', text: 'Let me', is_partial: true },
      'ws_a'
    );
    messages.apply(
      {
        type: 'thinking',
        message_id: 'msg_t2',
        text: 'Let me check the file',
        is_partial: true,
      },
      'ws_a'
    );
    const markers = messages.listForWorkspace('ws_a').filter((m) => m.id.startsWith('thinking_'));
    expect(markers).toHaveLength(1);
    expect(markers[0].text).toMatch(/let me check the file/i);
  });

  it('apply Thinking truncates very long thinking text in the marker preview', () => {
    const long = 'x'.repeat(600);
    messages.apply(
      { type: 'thinking', message_id: 'msg_long', text: long, is_partial: false },
      'ws_a'
    );
    const marker = messages.listForWorkspace('ws_a').find((m) => m.id.startsWith('thinking_'));
    expect(marker!.text.length).toBeLessThan(long.length);
  });

  it('apply Compact emits unique ids so concurrent events do not collide', () => {
    messages.apply({ type: 'compact', trigger: 'auto', pre_tokens: 1000 }, 'ws_a');
    messages.apply({ type: 'compact', trigger: 'auto', pre_tokens: 2000 }, 'ws_a');
    const markers = messages.listForWorkspace('ws_a').filter((m) => m.role === 'system');
    expect(markers).toHaveLength(2);
    expect(markers[0].id).not.toBe(markers[1].id);
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

  it('apply ToolUse with no prior message creates a synthetic assistant message', () => {
    messages.apply(
      {
        type: 'tool_use',
        message_id: 'msg_orphan',
        tool_use: { id: 'toolu_b', name: 'Write', input: {} },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].role).toBe('assistant');
    expect(list[0].tool_use?.name).toBe('Write');
    expect(list[0].text).toBe('');
  });

  it('apply Message event preserves created_at on subsequent updates', () => {
    messages.apply(
      {
        type: 'message',
        id: 'msg_a',
        role: 'assistant',
        text: 'first',
        is_partial: true,
      },
      'ws_a'
    );
    const initialCreatedAt = messages.listForWorkspace('ws_a')[0].created_at;
    // Wait a tick so Date.now() would differ.
    const sleep = new Promise((r) => setTimeout(r, 5));
    return sleep.then(() => {
      messages.apply(
        {
          type: 'message',
          id: 'msg_a',
          role: 'assistant',
          text: 'second',
          is_partial: false,
        },
        'ws_a'
      );
      const list = messages.listForWorkspace('ws_a');
      expect(list[0].text).toBe('second');
      expect(list[0].created_at).toBe(initialCreatedAt);
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

  describe('hydrate', () => {
    it('loads a batch of messages into a workspace', () => {
      const batch = [make('msg_1', 'ws_h'), make('msg_2', 'ws_h')];
      messages.hydrate('ws_h', batch);
      expect(messages.listForWorkspace('ws_h')).toHaveLength(2);
    });

    it('preserves chronological order via created_at', () => {
      const m1 = { ...make('msg_old', 'ws_h2'), created_at: 100 };
      const m2 = { ...make('msg_new', 'ws_h2'), created_at: 200 };
      messages.hydrate('ws_h2', [m2, m1]);
      const list = messages.listForWorkspace('ws_h2');
      expect(list[0].id).toBe('msg_old');
      expect(list[1].id).toBe('msg_new');
    });

    it('does not duplicate messages already in the store', () => {
      messages.upsert(make('msg_dup', 'ws_h3'));
      messages.hydrate('ws_h3', [make('msg_dup', 'ws_h3'), make('msg_new', 'ws_h3')]);
      expect(messages.listForWorkspace('ws_h3')).toHaveLength(2);
    });

    it('handles empty batches without error', () => {
      messages.hydrate('ws_h4', []);
      expect(messages.listForWorkspace('ws_h4')).toEqual([]);
    });
  });
});
