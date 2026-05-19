import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { teamActivity } from './team-activity.svelte';
import { removeToast, getToasts } from './toasts.svelte';
import type { TeamActivityRow } from '../types';

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
    expect(teamActivity.error).toContain('offline');
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
