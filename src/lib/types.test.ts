import { describe, it, expectTypeOf } from 'vitest';
import type { Message, MessageRole, ToolUse, ToolResult, AgentEvent, AgentStatus } from './types';

describe('Phase 1c types', () => {
  it('MessageRole is a union of the 4 roles', () => {
    expectTypeOf<MessageRole>().toEqualTypeOf<'user' | 'assistant' | 'system' | 'tool'>();
  });

  it('Message has all expected fields', () => {
    const m: Message = {
      id: 'msg_a',
      workspace_id: 'ws_x',
      role: 'assistant',
      text: 'hi',
      is_partial: false,
      tool_use: null,
      tool_result: null,
      created_at: 0,
    };
    expectTypeOf(m.id).toBeString();
    expectTypeOf(m.is_partial).toBeBoolean();
    expectTypeOf(m.tool_use).toEqualTypeOf<ToolUse | null>();
    expectTypeOf(m.tool_result).toEqualTypeOf<ToolResult | null>();
  });

  it('AgentStatus is the 4-variant union', () => {
    expectTypeOf<AgentStatus>().toEqualTypeOf<'running' | 'waiting' | 'error' | 'stopped'>();
  });

  it('AgentEvent.Init carries session_id and model', () => {
    const ev: AgentEvent = {
      type: 'init',
      session_id: 'ses_a',
      model: 'claude-sonnet-4-6',
    };
    expectTypeOf(ev).toMatchTypeOf<{ type: 'init' }>();
  });

  it('AgentEvent.Message carries id/role/text/is_partial', () => {
    const ev: AgentEvent = {
      type: 'message',
      id: 'msg_a',
      role: 'assistant',
      text: 'hi',
      is_partial: false,
    };
    expectTypeOf(ev).toMatchTypeOf<{ type: 'message' }>();
  });

  it('AgentEvent.Status carries status field', () => {
    const ev: AgentEvent = { type: 'status', status: 'running' };
    expectTypeOf(ev.status).toEqualTypeOf<AgentStatus>();
  });

  it('AgentEvent.Error carries message field', () => {
    const ev: AgentEvent = { type: 'error', message: 'spawn failed' };
    expectTypeOf(ev.message).toBeString();
  });
});
