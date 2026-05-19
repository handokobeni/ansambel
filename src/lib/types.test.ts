import { describe, expect, it, expectTypeOf } from 'vitest';
import type {
  Message,
  MessageRole,
  ToolUse,
  ToolResult,
  AgentEvent,
  AgentStatus,
  BitableBinding,
  FilterSpec,
  FilterOperator,
  TeamActivityConfig,
} from './types';

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
    if (ev.type === 'init') {
      expectTypeOf(ev.session_id).toBeString();
      expectTypeOf(ev.model).toBeString();
    }
  });

  it('AgentEvent.Message carries id/role/text/is_partial', () => {
    const ev: AgentEvent = {
      type: 'message',
      id: 'msg_a',
      role: 'assistant',
      text: 'hi',
      is_partial: false,
    };
    if (ev.type === 'message') {
      expectTypeOf(ev.id).toBeString();
      expectTypeOf(ev.role).toEqualTypeOf<MessageRole>();
      expectTypeOf(ev.text).toBeString();
      expectTypeOf(ev.is_partial).toBeBoolean();
    }
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

describe('FilterSpec types', () => {
  it('BitableBinding includes filters field with default empty spec shape', () => {
    const empty: FilterSpec = { conjunction: 'and', conditions: [] };
    const b: BitableBinding = {
      app_token: 'app',
      table_id: 'tbl',
      filters: empty,
      field_mapping: {
        title: { field_id: 'f', field_name: 'F' },
        description: null,
        status: null,
        order: null,
        pic: null,
      },
      status_value_mapping: { entries: {}, default_column: 'todo' },
      created_at: 0,
      updated_at: 0,
    };
    expect(b.filters.conditions).toEqual([]);
    expect(b.filters.conjunction).toBe('and');
  });

  it('TeamActivityConfig has app_token / table_id / machine_label fields', () => {
    const cfg: TeamActivityConfig = {
      app_token: 'bascn_team',
      table_id: 'tbl_team',
      machine_label: 'handoko@laptop-1',
    };
    expectTypeOf(cfg.app_token).toBeString();
    expectTypeOf(cfg.table_id).toBeString();
    expectTypeOf(cfg.machine_label).toBeString();
    expect(cfg.app_token).toBe('bascn_team');
  });

  it('FilterOperator literal type accepts all 10 operators', () => {
    const ops: FilterOperator[] = [
      'is',
      'isNot',
      'contains',
      'doesNotContain',
      'isEmpty',
      'isNotEmpty',
      'isGreater',
      'isGreaterEqual',
      'isLess',
      'isLessEqual',
    ];
    expect(ops).toHaveLength(10);
  });
});
