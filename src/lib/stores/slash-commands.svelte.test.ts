import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    slashCommands: {
      list: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { SlashCommandsStore } from './slash-commands.svelte';
import type { SlashCommand } from '$lib/types';

const sample: SlashCommand[] = [
  { name: 'help', description: 'Show help', source: { kind: 'builtin' } },
  {
    name: 'writing-plans',
    description: 'Use when you have a spec',
    source: { kind: 'plugin', plugin: 'superpowers' },
  },
  { name: 'deploy', description: 'Deploy staging', source: { kind: 'user' } },
];

beforeEach(() => {
  vi.clearAllMocks();
});

describe('SlashCommandsStore', () => {
  it('load: populates commands from api.slashCommands.list', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.commands).toEqual(sample);
  });

  it('filtered: returns prefix-matched entries case-insensitive', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.filtered('hel').map((c) => c.name)).toEqual(['help']);
    expect(store.filtered('WRI').map((c) => c.name)).toEqual(['writing-plans']);
  });

  it('filtered with empty string returns the full list', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.filtered('').length).toBe(sample.length);
  });

  it('filtered returns [] when no entry matches', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.filtered('zzz')).toEqual([]);
  });
});
