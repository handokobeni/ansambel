import { SvelteMap } from 'svelte/reactivity';
import type { AgentEvent, AgentStatus, Message } from '../types';

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
        // One bubble per tool call. Keying by parent message_id (the old
        // behaviour) collapsed multi-tool turns — Claude commonly fires
        // Read+Bash+Edit in a single assistant message and the live store
        // would overwrite the tool_use field on each ToolUse event,
        // showing only the last (or sometimes none, when the bubble had
        // already been claimed by a text Message). The disk persister
        // already uses this `${message_id}/tool_use/${tool_use.id}` shape,
        // so aligning live with it also makes hydration idempotent.
        const id = `${ev.message_id}/tool_use/${ev.tool_use.id}`;
        const existing = this.byWorkspace.get(wsId)?.get(id);
        const created_at = existing?.created_at ?? Date.now();
        this.upsert({
          id,
          workspace_id: wsId,
          role: 'tool',
          text: '',
          is_partial: false,
          tool_use: ev.tool_use,
          tool_result: null,
          created_at,
        });
        return;
      }
      case 'tool_result': {
        // Same id scheme as the persister so a tool_use bubble and its
        // matching tool_result bubble line up live and after reload.
        const id = `${ev.message_id}/tool_result/${ev.tool_result.tool_use_id}`;
        const existing = this.byWorkspace.get(wsId)?.get(id);
        const created_at = existing?.created_at ?? Date.now();
        this.upsert({
          id,
          workspace_id: wsId,
          role: 'tool',
          text: '',
          is_partial: false,
          tool_use: null,
          tool_result: ev.tool_result,
          created_at,
        });
        return;
      }
      case 'status':
        this.status.set(wsId, ev.status);
        return;
      case 'error':
        this.error.set(wsId, ev.message);
        return;
      case 'thinking': {
        // Thinking blocks render as a thin "Claude is thinking…" marker so
        // the user has visibility into what the model is doing between
        // text and tool calls. The id is derived from the owning assistant
        // message so streaming partials land on the same marker.
        const THINKING_PREVIEW = 280;
        const trimmed =
          ev.text.length > THINKING_PREVIEW ? `${ev.text.slice(0, THINKING_PREVIEW)}…` : ev.text;
        const id = `thinking_${ev.message_id}`;
        const existing = this.byWorkspace.get(wsId)?.get(id);
        const created_at = existing?.created_at ?? Date.now();
        this.upsert({
          id,
          workspace_id: wsId,
          role: 'system',
          text: `✻ Thinking — ${trimmed}`,
          is_partial: ev.is_partial,
          tool_use: null,
          tool_result: null,
          created_at,
        });
        return;
      }
      case 'compact': {
        // Render compaction as a thin system marker between turns. The
        // marker is just a Message with role=system; the bubble component
        // styles it as a centred notice instead of a chat bubble.
        const tokens =
          typeof ev.pre_tokens === 'number'
            ? `≈${Math.round(ev.pre_tokens / 1000)}k tokens`
            : 'auto';
        const text = `Compacted earlier conversation (${ev.trigger}, ${tokens})`;
        // Random suffix avoids id collisions when two compacts arrive in
        // the same millisecond — Date.now() is too coarse on its own.
        const id = `compact_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        this.upsert({
          id,
          workspace_id: wsId,
          role: 'system',
          text,
          is_partial: false,
          tool_use: null,
          tool_result: null,
          created_at: Date.now(),
        });
        return;
      }
    }
  }

  reset(): void {
    this.byWorkspace.clear();
    this.status.clear();
    this.error.clear();
  }
}

export const messages = new MessagesStore();
