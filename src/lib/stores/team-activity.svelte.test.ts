import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { teamActivity, friendlyFetchError } from './team-activity.svelte';
import { removeToast, getToasts } from './toasts.svelte';
import type { FetchResult, TeamActivityRow } from '../types';

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_x',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Task X',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: 1_700_000_000_000,
    last_message_preview: 'doing things',
    branch_name: 'feat/x',
    diff_summary: '',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

function clearToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

describe('TeamActivityStore — core state', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.error = null;
    teamActivity.selectedWorkspaceId = null;
    clearToasts();
  });

  it('sets status disabled on FetchResult disabled', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'disabled' });
    await teamActivity.refresh();
    expect(teamActivity.status).toBe('disabled');
    expect(teamActivity.rows.size).toBe(0);
  });

  it('sets status machine_label_empty on FetchResult machine_label_empty', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'machine_label_empty' });
    await teamActivity.refresh();
    expect(teamActivity.status).toBe('machine_label_empty');
  });

  it('sets status no_overlap_repos on FetchResult no_overlap_repos', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'no_overlap_repos' });
    await teamActivity.refresh();
    expect(teamActivity.status).toBe('no_overlap_repos');
  });

  it('reconciles new rows into the SvelteMap', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' }), row({ workspace_id: 'ws_b' })],
    });
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(2);
    expect(teamActivity.rows.has('ws_a')).toBe(true);
    expect(teamActivity.rows.has('ws_b')).toBe(true);
  });

  it('removes rows that were dropped from the server response', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' }), row({ workspace_id: 'ws_b' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(1);
    expect(teamActivity.rows.has('ws_b')).toBe(false);
  });

  it('updates existing rows in place when the server response changes them', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a', ansambel_status: 'running' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a', ansambel_status: 'waiting' })],
    });
    await teamActivity.refresh();
    expect(teamActivity.rows.get('ws_a')?.ansambel_status).toBe('waiting');
  });

  it('clears rows when a disabled response arrives after a rows response', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'disabled' });
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(0);
    expect(teamActivity.status).toBe('disabled');
  });

  it('keeps prior rows when a network error happens', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    vi.mocked(invoke).mockRejectedValueOnce(new Error('offline'));
    await teamActivity.refresh();
    expect(teamActivity.rows.size).toBe(1);
    expect(teamActivity.status).toBe('error');
    // Friendly copy, not the raw error string.
    expect(teamActivity.error).toMatch(/connection/i);
  });

  it('select sets selectedWorkspaceId', () => {
    teamActivity.select('ws_a');
    expect(teamActivity.selectedWorkspaceId).toBe('ws_a');
    teamActivity.select(null);
    expect(teamActivity.selectedWorkspaceId).toBe(null);
  });

  it('auto-clears selection and emits toast when the selected row is removed', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: 'rows',
      rows: [row({ workspace_id: 'ws_a' })],
    });
    await teamActivity.refresh();
    teamActivity.select('ws_a');
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    await teamActivity.refresh();
    expect(teamActivity.selectedWorkspaceId).toBe(null);
    expect(getToasts().size).toBe(1);
  });
});

describe('TeamActivityStore — poll loop + visibility', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockReset();
    teamActivity.stop();
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.error = null;
    teamActivity.selectedWorkspaceId = null;
  });

  afterEach(() => {
    teamActivity.stop();
    vi.useRealTimers();
  });

  it('start triggers an immediate fetch (does not wait 10s)', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
  });

  it('polls every 10 seconds after the first fetch', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsAfterStart = vi.mocked(invoke).mock.calls.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart + 1);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart + 2);
  });

  it('skips the tick when document.visibilityState is hidden', async () => {
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'hidden',
    });
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsAfterStart = vi.mocked(invoke).mock.calls.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart);
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'visible',
    });
  });

  it('fetches immediately when visibility flips to visible', async () => {
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'visible',
    });
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsBefore = vi.mocked(invoke).mock.calls.length;
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.runOnlyPendingTimersAsync();
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsBefore + 1);
  });

  it('skips overlapping ticks when a previous fetch is inflight', async () => {
    let resolveFirst: (v: FetchResult) => void = () => {};
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise<FetchResult>((res) => {
          resolveFirst = res;
        })
    );
    teamActivity.start();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(invoke).toHaveBeenCalledTimes(1);
    resolveFirst({ kind: 'rows', rows: [] });
  });

  it('start is idempotent when called twice (no double interval)', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('stop clears the interval and removes the visibility listener', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'rows', rows: [] });
    teamActivity.start();
    await vi.runOnlyPendingTimersAsync();
    const callsAfterStart = vi.mocked(invoke).mock.calls.length;
    teamActivity.stop();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart);
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.runOnlyPendingTimersAsync();
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsAfterStart);
  });
});

describe('friendlyFetchError', () => {
  it('maps a connection failure (offline) to a connection hint', () => {
    expect(friendlyFetchError('offline')).toMatch(/connection/i);
    expect(
      friendlyFetchError(
        'Lark API: tenant_access_token request: error sending request for url (https://open.larksuite.com/...)'
      )
    ).toMatch(/connection/i);
    // The raw internal URL must not leak into the friendly copy.
    expect(
      friendlyFetchError('error sending request for url (https://open.larksuite.com/x)')
    ).not.toMatch(/https?:\/\//);
  });

  it('maps a Lark auth/credential failure to a settings hint', () => {
    expect(friendlyFetchError('Lark API: code 99991663 invalid app_secret')).toMatch(
      /credential|settings/i
    );
  });

  it('falls back to a generic retry message for unknown errors', () => {
    const msg = friendlyFetchError('something weird happened');
    expect(msg.length).toBeGreaterThan(0);
    expect(msg).not.toMatch(/something weird/);
  });
});
