// src/lib/components/lark/TeamActivitySettings.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import TeamActivitySettings from './TeamActivitySettings.svelte';
import type { TeamActivityConfig } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

// Toast mock — capture calls so we can assert without rendering Toasts.
const addToastMock = vi.fn();
vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: (...args: unknown[]) => addToastMock(...args),
}));

import { invoke } from '@tauri-apps/api/core';

const configFixture: TeamActivityConfig = {
  app_token: 'bascn_demo',
  table_id: 'tblABC',
  machine_label: 'handoko@laptop-1',
};

async function flush() {
  // Two ticks: one for onMount resolution, one for state propagation.
  await tick();
  await tick();
}

beforeEach(() => {
  vi.clearAllMocks();
  addToastMock.mockClear();
});

describe('TeamActivitySettings', () => {
  it('renders empty form with default machine_label when no config exists', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();
    expect((screen.getByTestId('team-activity-app-token') as HTMLInputElement).value).toBe('');
    expect((screen.getByTestId('team-activity-table-id') as HTMLInputElement).value).toBe('');
    expect((screen.getByTestId('team-activity-machine-label') as HTMLInputElement).value).toBe(
      'me@machine'
    );
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Not configured');
  });

  it('pre-fills form when api.teamActivity.get returns a config', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(configFixture);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();
    expect((screen.getByTestId('team-activity-app-token') as HTMLInputElement).value).toBe(
      'bascn_demo'
    );
    expect((screen.getByTestId('team-activity-table-id') as HTMLInputElement).value).toBe('tblABC');
    expect((screen.getByTestId('team-activity-machine-label') as HTMLInputElement).value).toBe(
      'handoko@laptop-1'
    );
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Active');
  });

  it('Save button is disabled until app_token and table_id are non-empty', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    const saveBtn = screen.getByTestId('team-activity-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(true);

    await fireEvent.input(screen.getByTestId('team-activity-app-token'), {
      target: { value: 'bascn_x' },
    });
    expect((screen.getByTestId('team-activity-save') as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.input(screen.getByTestId('team-activity-table-id'), {
      target: { value: 'tbl_y' },
    });
    expect((screen.getByTestId('team-activity-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('Save calls api.teamActivity.set with the form values and toasts success', async () => {
    const calls: Array<[string, unknown]> = [];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'get_team_activity_config') return Promise.resolve(null);
      if (cmd === 'set_team_activity_config') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    await fireEvent.input(screen.getByTestId('team-activity-app-token'), {
      target: { value: 'bascn_x' },
    });
    await fireEvent.input(screen.getByTestId('team-activity-table-id'), {
      target: { value: 'tbl_y' },
    });
    await fireEvent.input(screen.getByTestId('team-activity-machine-label'), {
      target: { value: 'handoko@desk' },
    });
    await fireEvent.click(screen.getByTestId('team-activity-save'));
    await flush();

    const setCall = calls.find((c) => c[0] === 'set_team_activity_config');
    expect(setCall).toBeDefined();
    expect(setCall?.[1]).toMatchObject({
      cfg: {
        app_token: 'bascn_x',
        table_id: 'tbl_y',
        machine_label: 'handoko@desk',
      },
    });
    expect(addToastMock).toHaveBeenCalledWith(expect.stringContaining('Restart app'), 'success');
    // Status flips to Active after a successful save.
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Active');
  });

  it('Setup table schema button calls api.teamActivity.setupTable with current app_token + table_id', async () => {
    const calls: Array<[string, unknown]> = [];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'get_team_activity_config') return Promise.resolve(null);
      if (cmd === 'setup_team_activity_table')
        return Promise.resolve(['workspace_id', 'task_title']);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    // Button disabled while app_token is empty.
    expect((screen.getByTestId('team-activity-setup') as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.input(screen.getByTestId('team-activity-app-token'), {
      target: { value: 'bascn_x' },
    });
    await fireEvent.input(screen.getByTestId('team-activity-table-id'), {
      target: { value: 'tbl_y' },
    });
    expect((screen.getByTestId('team-activity-setup') as HTMLButtonElement).disabled).toBe(false);

    await fireEvent.click(screen.getByTestId('team-activity-setup'));
    await flush();

    const setupCall = calls.find((c) => c[0] === 'setup_team_activity_table');
    expect(setupCall).toBeDefined();
    expect(setupCall?.[1]).toMatchObject({ appToken: 'bascn_x', tableId: 'tbl_y' });
    expect(addToastMock).toHaveBeenCalledWith(expect.stringContaining('2'), 'success');
  });

  it('Disconnect clears the config and re-shows the empty form', async () => {
    let stored: TeamActivityConfig | null = configFixture;
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(stored);
      if (cmd === 'set_team_activity_config') {
        const cfg = (args as { cfg: TeamActivityConfig }).cfg;
        // Backend treats empty token as "publisher disabled" → next get returns null.
        stored = cfg.app_token.trim().length === 0 ? null : cfg;
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();
    expect((screen.getByTestId('team-activity-app-token') as HTMLInputElement).value).toBe(
      'bascn_demo'
    );
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Active');

    // First click reveals the inline confirm.
    await fireEvent.click(screen.getByTestId('team-activity-disconnect'));
    await flush();
    expect(screen.getByTestId('team-activity-disconnect-confirm')).toBeTruthy();

    // Confirm performs the clear.
    await fireEvent.click(screen.getByTestId('team-activity-disconnect-confirm'));
    await flush();

    expect(addToastMock).toHaveBeenCalledWith(
      expect.stringContaining('disconnected'),
      expect.any(String)
    );
    expect((screen.getByTestId('team-activity-app-token') as HTMLInputElement).value).toBe('');
    expect((screen.getByTestId('team-activity-table-id') as HTMLInputElement).value).toBe('');
    expect((screen.getByTestId('team-activity-machine-label') as HTMLInputElement).value).toBe(
      'me@machine'
    );
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Not configured');
  });

  it('Save failure toasts an error and does not flip status', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(null);
      if (cmd === 'set_team_activity_config') return Promise.reject(new Error('disk write failed'));
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    await fireEvent.input(screen.getByTestId('team-activity-app-token'), {
      target: { value: 'bascn_x' },
    });
    await fireEvent.input(screen.getByTestId('team-activity-table-id'), {
      target: { value: 'tbl_y' },
    });
    await fireEvent.click(screen.getByTestId('team-activity-save'));
    await flush();

    expect(addToastMock).toHaveBeenCalledWith(
      expect.stringContaining('disk write failed'),
      'error'
    );
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Not configured');
  });

  it('Setup table failure surfaces a toast', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(null);
      if (cmd === 'setup_team_activity_table')
        return Promise.reject(new Error('Lark API: table not found'));
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    await fireEvent.input(screen.getByTestId('team-activity-app-token'), {
      target: { value: 'bascn_x' },
    });
    await fireEvent.input(screen.getByTestId('team-activity-table-id'), {
      target: { value: 'tbl_y' },
    });
    await fireEvent.click(screen.getByTestId('team-activity-setup'));
    await flush();

    expect(addToastMock).toHaveBeenCalledWith(expect.stringContaining('table not found'), 'error');
  });

  it('Cancel button on inline confirm dismisses without disconnecting', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(configFixture);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    await fireEvent.click(screen.getByTestId('team-activity-disconnect'));
    await flush();
    expect(screen.queryByTestId('team-activity-disconnect-confirm')).toBeTruthy();

    await fireEvent.click(screen.getByTestId('team-activity-disconnect-cancel'));
    await flush();

    expect(screen.queryByTestId('team-activity-disconnect-confirm')).toBeNull();
    // set_team_activity_config should NOT have been called by cancel.
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith(
      'set_team_activity_config',
      expect.anything()
    );
    expect(screen.getByTestId('team-activity-status').textContent).toContain('Active');
  });

  it('Setup table with zero new columns toasts the verified-message', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_team_activity_config') return Promise.resolve(configFixture);
      if (cmd === 'setup_team_activity_table') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(TeamActivitySettings);
    await flush();

    await fireEvent.click(screen.getByTestId('team-activity-setup'));
    await flush();

    // 0 created → toast still surfaces, with "0" count.
    expect(addToastMock).toHaveBeenCalledWith(expect.stringContaining('0'), 'success');
  });
});
