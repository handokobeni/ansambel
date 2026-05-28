import { api } from '$lib/ipc';
import type { SlashCommand } from '$lib/types';

export class SlashCommandsStore {
  commands = $state<SlashCommand[]>([]);

  async load(): Promise<void> {
    try {
      this.commands = await api.slashCommands.list();
    } catch (err) {
      // Discovery is fail-soft on the backend; if the IPC itself fails,
      // log + leave commands empty so the picker simply shows the
      // empty-state hint.
      console.error('slashCommands.load failed', err);
      this.commands = [];
    }
  }

  filtered(prefix: string): SlashCommand[] {
    if (prefix.length === 0) return this.commands;
    const lower = prefix.toLowerCase();
    return this.commands.filter((c) => c.name.toLowerCase().startsWith(lower));
  }
}

export const slashCommands = new SlashCommandsStore();
