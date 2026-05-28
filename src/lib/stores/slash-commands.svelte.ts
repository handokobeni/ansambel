import { api } from '$lib/ipc';
import type { SlashCommand } from '$lib/types';

export class SlashCommandsStore {
  commands = $state<SlashCommand[]>([]);

  async load(): Promise<void> {
    try {
      const result = await api.slashCommands.list();
      // Guard against IPC returning a non-array (e.g. an E2E shim that
      // doesn't implement list_slash_commands and falls through with
      // `undefined`). Without this, `filtered('')` returns `undefined`
      // and any consumer reading `.length` poisons the Svelte effect
      // batch, which silently breaks unrelated effects downstream
      // (e.g. Terminal.svelte's first-open auto-add).
      this.commands = Array.isArray(result) ? result : [];
    } catch (err) {
      // Discovery is fail-soft on the backend; if the IPC itself fails,
      // log + leave commands empty so the picker simply shows the
      // empty-state hint.
      console.error('slashCommands.load failed', err);
      this.commands = [];
    }
  }

  filtered(prefix: string): SlashCommand[] {
    // Belt-and-suspenders: callers depend on this returning an array even
    // if `commands` somehow ends up as a non-array (shouldn't happen given
    // load()'s guard, but keeps the picker safe under any future regression).
    const list = Array.isArray(this.commands) ? this.commands : [];
    if (prefix.length === 0) return list;
    const lower = prefix.toLowerCase();
    return list.filter((c) => c.name.toLowerCase().startsWith(lower));
  }
}

export const slashCommands = new SlashCommandsStore();
