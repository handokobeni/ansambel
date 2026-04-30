import { SvelteMap } from 'svelte/reactivity';
import type { AgentEvent, AgentStatus, Message, MessageRole } from '../types';

class MessagesStore {
  readonly byWorkspace = new SvelteMap<string, SvelteMap<string, Message>>();
  readonly status = new SvelteMap<string, AgentStatus>();
  readonly error = new SvelteMap<string, string>();

  private getOrCreate(wsId: string): SvelteMap<string, Message> {
    let map = this.byWorkspace.get(wsId);
    if (!map) {
      map = new SvelteMap<string, Message>();
      this.byWorkspace.set(wsId, map);
    }
    return map;
  }

  upsert(msg: Message): void {
    this.getOrCreate(msg.workspace_id).set(msg.id, msg);
  }

  hydrate(wsId: string, batch: Message[]): void {
    const map = this.getOrCreate(wsId);
    for (const msg of batch) {
      if (!map.has(msg.id)) {
        map.set(msg.id, msg);
      }
    }
  }

  listForWorkspace(wsId: string): Message[] {
    const map = this.byWorkspace.get(wsId);
    if (!map) return [];
    return [...map.values()].sort((a, b) => a.created_at - b.created_at);
  }

  statusFor(wsId: string): AgentStatus | undefined {
    return this.status.get(wsId);
  }

  errorFor(wsId: string): string | undefined {
    return this.error.get(wsId);
  }

  apply(ev: AgentEvent, wsId: string): void {
    switch (ev.type) {
      case 'init':
        return;
      case 'message': {
        const existing = this.byWorkspace.get(wsId)?.get(ev.id);
        const created_at = existing?.created_at ?? Date.now();
        this.upsert({
          id: ev.id,
          workspace_id: wsId,
          role: ev.role,
          text: ev.text,
          is_partial: ev.is_partial,
          tool_use: existing?.tool_use ?? null,
          tool_result: existing?.tool_result ?? null,
          created_at,
        });
        return;
      }
      case 'tool_use': {
        const existing = this.byWorkspace.get(wsId)?.get(ev.message_id);
        if (existing) {
          this.upsert({ ...existing, tool_use: ev.tool_use });
        } else {
          this.upsert({
            id: ev.message_id,
            workspace_id: wsId,
            role: 'assistant',
            text: '',
            is_partial: false,
            tool_use: ev.tool_use,
            tool_result: null,
            created_at: Date.now(),
          });
        }
        return;
      }
      case 'tool_result': {
        const role: MessageRole = 'tool';
        this.upsert({
          id: ev.message_id,
          workspace_id: wsId,
          role,
          text: '',
          is_partial: false,
          tool_use: null,
          tool_result: ev.tool_result,
          created_at: Date.now(),
        });
        return;
      }
      case 'status':
        this.status.set(wsId, ev.status);
        return;
      case 'error':
        this.error.set(wsId, ev.message);
        return;
    }
  }

  reset(): void {
    this.byWorkspace.clear();
    this.status.clear();
    this.error.clear();
  }
}

export const messages = new MessagesStore();
