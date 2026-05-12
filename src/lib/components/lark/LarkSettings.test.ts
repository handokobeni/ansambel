// src/lib/components/lark/LarkSettings.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import LarkSettings from './LarkSettings.svelte';
import type { LarkStatus } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

import { invoke } from '@tauri-apps/api/core';

const statusUnconfigured: LarkStatus = {
  configured: false,
  app_id: null,
  app_token: null,
  table_id: null,
  base_url: 'https://open.larksuite.com',
  has_secret: false,
};

const statusConfigured: LarkStatus = {
  configured: true,
  app_id: 'cli_test',
  app_token: 'bascntest',
  table_id: 'tbltest',
  base_url: 'https://open.larksuite.com',
  has_secret: true,
};

async function flush() {
  // Two ticks: one for onMount resolution, one for state propagation.
  await tick();
  await tick();
}

function mockGetStatus(status: LarkStatus): void {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'get_lark_status') return Promise.resolve(status);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('LarkSettings', () => {
  it('renders Loading initially', () => {
    mockGetStatus(statusUnconfigured);
    render(LarkSettings);
    expect(screen.getByTestId('lark-loading')).toBeTruthy();
  });

  it('renders form fields after status loads (unconfigured)', async () => {
    mockGetStatus(statusUnconfigured);
    render(LarkSettings);
    await flush();
    expect(screen.getByLabelText(/app id/i)).toBeTruthy();
    expect(screen.getByLabelText(/app secret/i)).toBeTruthy();
    expect(screen.getByLabelText(/app token/i)).toBeTruthy();
    expect(screen.getByLabelText(/table id/i)).toBeTruthy();
    expect(screen.getByLabelText(/base url/i)).toBeTruthy();
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Not configured');
  });

  it('pre-populates form from configured status, leaves secret blank', async () => {
    mockGetStatus(statusConfigured);
    render(LarkSettings);
    await flush();
    expect((screen.getByLabelText(/app id/i) as HTMLInputElement).value).toBe('cli_test');
    expect((screen.getByLabelText(/app token/i) as HTMLInputElement).value).toBe('bascntest');
    expect((screen.getByLabelText(/table id/i) as HTMLInputElement).value).toBe('tbltest');
    expect((screen.getByLabelText(/app secret/i) as HTMLInputElement).value).toBe('');
    expect(screen.getByTestId('lark-secret-stored')).toBeTruthy();
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Configured');
  });

  it('Save button is disabled until all four required fields have values', async () => {
    mockGetStatus(statusUnconfigured);
    render(LarkSettings);
    await flush();
    const saveBtn = screen.getByTestId('lark-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(true);

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.input(screen.getByLabelText(/app token/i), { target: { value: 'bascn_x' } });
    expect((screen.getByTestId('lark-save') as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.input(screen.getByLabelText(/table id/i), { target: { value: 'tbl_x' } });
    expect((screen.getByTestId('lark-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('Test connection is disabled when not configured', async () => {
    mockGetStatus(statusUnconfigured);
    render(LarkSettings);
    await flush();
    const testBtn = screen.getByTestId('lark-test') as HTMLButtonElement;
    expect(testBtn.disabled).toBe(true);
  });

  it('Test connection is enabled when configured', async () => {
    mockGetStatus(statusConfigured);
    render(LarkSettings);
    await flush();
    const testBtn = screen.getByTestId('lark-test') as HTMLButtonElement;
    expect(testBtn.disabled).toBe(false);
  });

  it('Save calls set_lark_credentials and shows success banner', async () => {
    const calls: Array<[string, unknown]> = [];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'get_lark_status') return Promise.resolve(statusUnconfigured);
      if (cmd === 'set_lark_credentials') return Promise.resolve(statusConfigured);
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.input(screen.getByLabelText(/app token/i), { target: { value: 'bascn_x' } });
    await fireEvent.input(screen.getByLabelText(/table id/i), { target: { value: 'tbl_x' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();

    const setCall = calls.find((c) => c[0] === 'set_lark_credentials');
    expect(setCall).toBeDefined();
    expect(setCall?.[1]).toMatchObject({
      appId: 'cli_x',
      appSecret: 'shh',
      appToken: 'bascn_x',
      tableId: 'tbl_x',
    });
    expect(screen.getByTestId('lark-banner').textContent).toContain('saved');
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Configured');
  });

  it('Save error surfaces in banner without throwing', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusUnconfigured);
      if (cmd === 'set_lark_credentials') return Promise.reject(new Error('Lark API: bad app_id'));
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.input(screen.getByLabelText(/app token/i), { target: { value: 'bascn_x' } });
    await fireEvent.input(screen.getByLabelText(/table id/i), { target: { value: 'tbl_x' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();

    expect(screen.getByTestId('lark-banner').textContent).toContain('Save failed');
    expect(screen.getByTestId('lark-banner').textContent).toContain('bad app_id');
  });

  it('Test connection success shows OK banner', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusConfigured);
      if (cmd === 'test_lark_connection') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();

    await fireEvent.click(screen.getByTestId('lark-test'));
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('Connection OK');
  });

  it('Test connection error surfaces in banner', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusConfigured);
      if (cmd === 'test_lark_connection')
        return Promise.reject(new Error('Lark API: code 99991663: app not found'));
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();

    await fireEvent.click(screen.getByTestId('lark-test'));
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('Connection failed');
    expect(screen.getByTestId('lark-banner').textContent).toContain('app not found');
  });

  it('Clear calls clear_lark_credentials then reloads status', async () => {
    const calls: string[] = [];
    let nextStatus: LarkStatus = statusConfigured;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      calls.push(cmd);
      if (cmd === 'get_lark_status') return Promise.resolve(nextStatus);
      if (cmd === 'clear_lark_credentials') {
        nextStatus = statusUnconfigured;
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Configured');

    await fireEvent.click(screen.getByTestId('lark-clear'));
    await flush();

    expect(calls).toContain('clear_lark_credentials');
    expect(screen.getByTestId('lark-banner').textContent).toContain('cleared');
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Not configured');
  });

  it('Clear error surfaces in banner', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusConfigured);
      if (cmd === 'clear_lark_credentials')
        return Promise.reject(new Error('keyring delete failed'));
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();

    await fireEvent.click(screen.getByTestId('lark-clear'));
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('Clear failed');
  });

  it('falls back to default base URL when input left blank', async () => {
    const calls: Array<[string, unknown]> = [];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'get_lark_status') return Promise.resolve(statusUnconfigured);
      if (cmd === 'set_lark_credentials') return Promise.resolve(statusConfigured);
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.input(screen.getByLabelText(/app token/i), { target: { value: 'bascn_x' } });
    await fireEvent.input(screen.getByLabelText(/table id/i), { target: { value: 'tbl_x' } });
    // Clear the baseUrl field (it was pre-populated with the default).
    await fireEvent.input(screen.getByLabelText(/base url/i), { target: { value: '   ' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();

    const setCall = calls.find((c) => c[0] === 'set_lark_credentials');
    // Blank → undefined → backend default.
    expect((setCall?.[1] as { baseUrl: unknown }).baseUrl).toBeNull();
  });

  it('shows error banner when initial getStatus fails', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.reject(new Error('store poisoned'));
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('Failed to load status');
  });
});
