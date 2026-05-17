// src/lib/components/lark/LarkGlobalSettings.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import LarkGlobalSettings from './LarkGlobalSettings.svelte';
import type { LarkStatus } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

import { invoke } from '@tauri-apps/api/core';

const statusUnconfigured: LarkStatus = {
  configured: false,
  app_id: null,
  base_url: 'https://open.larksuite.com',
  has_secret: false,
};

const statusConfigured: LarkStatus = {
  configured: true,
  app_id: 'cli_test',
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

describe('LarkGlobalSettings', () => {
  it('renders Loading initially', () => {
    mockGetStatus(statusUnconfigured);
    render(LarkGlobalSettings);
    expect(screen.getByTestId('lark-loading')).toBeTruthy();
  });

  it('renders form fields after status loads (unconfigured)', async () => {
    mockGetStatus(statusUnconfigured);
    render(LarkGlobalSettings);
    await flush();
    expect(screen.getByLabelText(/app id/i)).toBeTruthy();
    expect(screen.getByLabelText(/app secret/i)).toBeTruthy();
    expect(screen.getByLabelText(/base url/i)).toBeTruthy();
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Not configured');
  });

  it('pre-populates form from configured status, leaves secret blank', async () => {
    mockGetStatus(statusConfigured);
    render(LarkGlobalSettings);
    await flush();
    expect((screen.getByLabelText(/app id/i) as HTMLInputElement).value).toBe('cli_test');
    expect((screen.getByLabelText(/app secret/i) as HTMLInputElement).value).toBe('');
    expect(screen.getByTestId('lark-secret-stored')).toBeTruthy();
    expect(screen.getByTestId('lark-status-pill').textContent).toContain('Configured');
  });

  it('Save button is disabled until app_id and app_secret are filled', async () => {
    mockGetStatus(statusUnconfigured);
    render(LarkGlobalSettings);
    await flush();
    const saveBtn = screen.getByTestId('lark-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(true);

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    expect((screen.getByTestId('lark-save') as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    expect((screen.getByTestId('lark-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('Save calls set_lark_credentials with app_id/secret/base_url and shows success banner', async () => {
    const calls: Array<[string, unknown]> = [];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'get_lark_status') return Promise.resolve(statusUnconfigured);
      if (cmd === 'set_lark_credentials') return Promise.resolve(statusConfigured);
      return Promise.resolve(undefined);
    });
    render(LarkGlobalSettings);
    await flush();

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();

    const setCall = calls.find((c) => c[0] === 'set_lark_credentials');
    expect(setCall).toBeDefined();
    expect(setCall?.[1]).toMatchObject({
      appId: 'cli_x',
      appSecret: 'shh',
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
    render(LarkGlobalSettings);
    await flush();

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();

    expect(screen.getByTestId('lark-banner').textContent).toContain('Save failed');
    expect(screen.getByTestId('lark-banner').textContent).toContain('bad app_id');
  });

  it('falls back to null base_url when input left blank', async () => {
    const calls: Array<[string, unknown]> = [];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'get_lark_status') return Promise.resolve(statusUnconfigured);
      if (cmd === 'set_lark_credentials') return Promise.resolve(statusConfigured);
      return Promise.resolve(undefined);
    });
    render(LarkGlobalSettings);
    await flush();

    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    // Clear the baseUrl field (it was pre-populated with the default).
    await fireEvent.input(screen.getByLabelText(/base url/i), { target: { value: '   ' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();

    const setCall = calls.find((c) => c[0] === 'set_lark_credentials');
    // Blank → undefined → backend default (IPC wrapper converts to null).
    expect((setCall?.[1] as { baseUrl: unknown }).baseUrl).toBeNull();
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
    render(LarkGlobalSettings);
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
    render(LarkGlobalSettings);
    await flush();

    await fireEvent.click(screen.getByTestId('lark-clear'));
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('Clear failed');
  });

  it('shows error banner when initial getStatus fails', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.reject(new Error('store poisoned'));
      return Promise.resolve(undefined);
    });
    render(LarkGlobalSettings);
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('Failed to load status');
  });

  it('Save error with a string-type rejection shows the string in banner', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusUnconfigured);
      if (cmd === 'set_lark_credentials') return Promise.reject('plain string error');
      return Promise.resolve(undefined);
    });
    render(LarkGlobalSettings);
    await flush();
    await fireEvent.input(screen.getByLabelText(/app id/i), { target: { value: 'cli_x' } });
    await fireEvent.input(screen.getByLabelText(/app secret/i), { target: { value: 'shh' } });
    await fireEvent.click(screen.getByTestId('lark-save'));
    await flush();
    expect(screen.getByTestId('lark-banner').textContent).toContain('plain string error');
  });

  it('Load error with circular-reference non-stringifiable rejection falls back to "unknown error"', async () => {
    // Create a circular reference that JSON.stringify cannot serialize
    const circular: Record<string, unknown> = {};
    circular['self'] = circular;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.reject(circular);
      return Promise.resolve(undefined);
    });
    render(LarkGlobalSettings);
    await flush();
    // The describe() function's JSON.stringify catch path fires → 'unknown error'
    // The banner will contain "Failed to load status: unknown error"
    expect(screen.getByTestId('lark-banner').textContent).toContain('Failed to load status');
    expect(screen.getByTestId('lark-banner').textContent).toContain('unknown error');
  });

  it('form submit while canSave is false (guard branch) is a no-op', async () => {
    mockGetStatus(statusUnconfigured);
    const { container } = render(LarkGlobalSettings);
    await flush();
    // canSave is false — fields are empty. Directly submit the form to hit the !canSave guard.
    const form = container.querySelector('form');
    if (form) {
      await fireEvent.submit(form);
    }
    await flush();
    // No set_lark_credentials invoked
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith('set_lark_credentials', expect.anything());
  });
});
