import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// import after mock so ipc uses it
import { api } from './ipc';

describe('api.system.getAppVersion', () => {
  beforeEach(() => invokeMock.mockReset());

  it('calls invoke with get_app_version and no args', async () => {
    invokeMock.mockResolvedValueOnce('0.1.0-pre');
    const v = await api.system.getAppVersion();
    expect(invokeMock).toHaveBeenCalledWith('get_app_version');
    expect(v).toBe('0.1.0-pre');
  });

  it('rejects on backend error', async () => {
    invokeMock.mockRejectedValueOnce(new Error('boom'));
    await expect(api.system.getAppVersion()).rejects.toThrow('boom');
  });
});
