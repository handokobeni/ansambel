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

  it('apply ToolUse creates a separate tool bubble alongside the parent text', () => {
    // Live behaviour must match what the disk persister produces (separate
    // Message per tool call) so hydration is idempotent and multi-tool
    // turns render every call rather than collapsing into one bubble.
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
        tool_use: { id: 'toolu_a', name: 'Read', input: { file_path: '/etc/hosts' } },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(2);
    const text = list.find((m) => m.id === 'msg_a');
    const tool = list.find((m) => m.id === 'msg_a/tool_use/toolu_a');
    expect(text!.text).toBe('using tool');
    expect(text!.tool_use).toBeNull();
    expect(tool!.role).toBe('tool');
    expect(tool!.tool_use?.name).toBe('Read');
  });

  it('apply ToolUse keeps each tool call distinct when one turn fires multiple', () => {
    // Claude commonly issues Read + Bash + Edit in a single assistant
    // message — the old keying-by-parent-message_id collapsed them all
    // into one bubble showing only the last tool. Disk gets it right via
    // unique ids; live must too.
    const tools = [
      { id: 'toolu_1', name: 'Read', input: { file_path: '/a' } },
      { id: 'toolu_2', name: 'Bash', input: { command: 'ls' } },
      { id: 'toolu_3', name: 'Edit', input: { file_path: '/b', old_string: 'x', new_string: 'y' } },
    ];
    for (const tu of tools) {
      messages.apply({ type: 'tool_use', message_id: 'msg_multi', tool_use: tu }, 'ws_a');
    }
    const list = messages.listForWorkspace('ws_a').filter((m) => m.tool_use);
    expect(list).toHaveLength(3);
    expect(list.map((m) => m.tool_use!.name).sort()).toEqual(['Bash', 'Edit', 'Read']);
  });

  it('apply ToolUse with no prior message still produces a tool bubble', () => {
    // Tool-only assistant turns (no text) — the disk persister writes them
    // as a Tool-role Message; live must do the same.
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
    expect(list[0].role).toBe('tool');
    expect(list[0].tool_use?.name).toBe('Write');
    expect(list[0].id).toBe('msg_orphan/tool_use/toolu_b');
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

  it('apply ToolResult creates a tool message keyed by tool_use_id', () => {
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
    expect(list[0].id).toBe('msg_r/tool_result/toolu_a');
    expect(list[0].tool_result?.content).toBe('ok');
  });

  it('apply ToolResult keeps each tool_use_id in its own bubble', () => {
    messages.apply(
      {
        type: 'tool_result',
        message_id: 'msg_user',
        tool_result: { tool_use_id: 'toolu_1', content: 'a', is_error: false },
      },
      'ws_a'
    );
    messages.apply(
      {
        type: 'tool_result',
        message_id: 'msg_user',
        tool_result: { tool_use_id: 'toolu_2', content: 'b', is_error: false },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(2);
    expect(list.map((m) => m.tool_result!.content).sort()).toEqual(['a', 'b']);
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

  describe('turn state', () => {
    it('starts a turn on status:running and clears it on status:waiting', () => {
      const before = Date.now();
      messages.apply({ type: 'status', status: 'running' }, 'ws_t');
      const turn = messages.turnFor('ws_t');
      expect(turn).not.toBeNull();
      expect(turn!.startedAt).toBeGreaterThanOrEqual(before);
      expect(turn!.inputTokens).toBe(0);
      expect(turn!.outputTokens).toBe(0);

      messages.apply({ type: 'status', status: 'waiting' }, 'ws_t');
      expect(messages.turnFor('ws_t')).toBeNull();
    });

    it('clears turn state on status:stopped and status:error too', () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_t1');
      messages.apply({ type: 'status', status: 'stopped' }, 'ws_t1');
      expect(messages.turnFor('ws_t1')).toBeNull();

      messages.apply({ type: 'status', status: 'running' }, 'ws_t2');
      messages.apply({ type: 'status', status: 'error' }, 'ws_t2');
      expect(messages.turnFor('ws_t2')).toBeNull();
    });

    it('accumulates Usage events into the active turn', () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_u');
      messages.apply(
        {
          type: 'usage',
          message_id: 'msg_a',
          input_tokens: 12,
          cache_creation_input_tokens: 0,
          cache_read_input_tokens: 4500,
          output_tokens: 100,
          total_input: 4512,
        },
        'ws_u'
      );
      messages.apply(
        {
          type: 'usage',
          message_id: 'msg_b',
          input_tokens: 0,
          cache_creation_input_tokens: 0,
          cache_read_input_tokens: 4600,
          output_tokens: 200,
          total_input: 4600,
        },
        'ws_u'
      );
      const turn = messages.turnFor('ws_u');
      expect(turn).not.toBeNull();
      expect(turn!.inputTokens).toBe(4512 + 4600);
      expect(turn!.outputTokens).toBe(100 + 200);
    });

    it('drops Usage events that arrive while no turn is active', () => {
      // The CLI sometimes echoes a final usage line just after status:waiting
      // — without a guard those numbers would leak into the next turn.
      messages.apply(
        {
          type: 'usage',
          message_id: 'msg_orphan',
          input_tokens: 100,
          cache_creation_input_tokens: 0,
          cache_read_input_tokens: 0,
          output_tokens: 50,
          total_input: 100,
        },
        'ws_orphan'
      );
      expect(messages.turnFor('ws_orphan')).toBeNull();
    });

    it('reset() clears turn state too so workspace switches start fresh', () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_r');
      messages.reset();
      expect(messages.turnFor('ws_r')).toBeNull();
    });

    it('re-applying a tool_use with the same id preserves created_at', () => {
      // Covers the existing?.created_at ?? Date.now() truthy branch.
      messages.apply(
        {
          type: 'tool_use',
          message_id: 'msg_p',
          tool_use: { id: 'toolu_p', name: 'Read', input: { file_path: '/x' } },
        },
        'ws_pp'
      );
      const id = `msg_p/tool_use/toolu_p`;
      const first = messages.listForWorkspace('ws_pp').find((m) => m.id === id)!;
      const firstCreated = first.created_at;
      // Re-apply (e.g. retry / replay) — created_at must be preserved.
      messages.apply(
        {
          type: 'tool_use',
          message_id: 'msg_p',
          tool_use: { id: 'toolu_p', name: 'Read', input: { file_path: '/y' } },
        },
        'ws_pp'
      );
      const second = messages.listForWorkspace('ws_pp').find((m) => m.id === id)!;
      expect(second.created_at).toBe(firstCreated);
    });

    it('re-applying a tool_result with the same id preserves created_at', () => {
      messages.apply(
        {
          type: 'tool_result',
          message_id: 'msg_q',
          tool_result: { tool_use_id: 'toolu_q', content: 'ok', is_error: false },
        },
        'ws_qq'
      );
      const id = `msg_q/tool_result/toolu_q`;
      const first = messages.listForWorkspace('ws_qq').find((m) => m.id === id)!;
      const firstCreated = first.created_at;
      messages.apply(
        {
          type: 'tool_result',
          message_id: 'msg_q',
          tool_result: { tool_use_id: 'toolu_q', content: 'ok2', is_error: false },
        },
        'ws_qq'
      );
      const second = messages.listForWorkspace('ws_qq').find((m) => m.id === id)!;
      expect(second.created_at).toBe(firstCreated);
    });

    it('attachToMessage on an unknown message id is a no-op', () => {
      // Covers the early-return branch in attachToMessage.
      messages.attachToMessage('ws_unknown', 'msg_nonexistent', [
        { kind: 'image', media_type: 'image/png', path: '/x.png', filename: 'x.png' },
      ]);
      expect(messages.listForWorkspace('ws_unknown')).toEqual([]);
    });

    it('a second status:running starts a fresh turn (zeros tokens, advances startedAt)', async () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_re');
      messages.apply(
        {
          type: 'usage',
          message_id: 'msg_x',
          input_tokens: 10,
          cache_creation_input_tokens: 0,
          cache_read_input_tokens: 0,
          output_tokens: 5,
          total_input: 10,
        },
        'ws_re'
      );
      const first = messages.turnFor('ws_re')!;
      // Tiny delay so startedAt is observably distinct.
      await new Promise((r) => setTimeout(r, 5));
      messages.apply({ type: 'status', status: 'waiting' }, 'ws_re');
      messages.apply({ type: 'status', status: 'running' }, 'ws_re');
      const second = messages.turnFor('ws_re')!;
      expect(second.inputTokens).toBe(0);
      expect(second.outputTokens).toBe(0);
      expect(second.startedAt).toBeGreaterThan(first.startedAt);
    });
  });
});
