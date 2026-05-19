import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import TeamActivityPanel from './TeamActivityPanel.svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';
import type { TeamActivityRow } from '$lib/types';

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_a',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Fix login',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: Date.now() - 5 * 60 * 1000,
    last_message_preview: '',
    branch_name: 'feat/login',
    diff_summary: '',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

describe('TeamActivityPanel', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    teamActivity.rows.clear();
    teamActivity.status = 'idle';
    teamActivity.error = null;
    teamActivity.selectedWorkspaceId = null;
    localStorage.removeItem('ansambel-team-activity-collapsed');
  });

  it('renders disabled hint when status is disabled', () => {
    teamActivity.status = 'disabled';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/Configure Team Activity in Settings/i)).toBeTruthy();
  });

  it('renders machine_label hint when status is machine_label_empty', () => {
    teamActivity.status = 'machine_label_empty';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/Set your machine label/i)).toBeTruthy();
  });

  it('renders add-repo hint when status is no_overlap_repos', () => {
    teamActivity.status = 'no_overlap_repos';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/Add a repo to see team activity/i)).toBeTruthy();
  });

  it('renders "No team activity" copy when idle with zero rows', () => {
    teamActivity.status = 'idle';
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/No team activity in your repos/i)).toBeTruthy();
  });

  it('groups rows by repo_display_name alphabetically', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_z', row({ workspace_id: 'ws_z', repo_display_name: 'zeta' }));
    teamActivity.rows.set('ws_a', row({ workspace_id: 'ws_a', repo_display_name: 'alpha' }));
    const { container } = render(TeamActivityPanel);
    const groups = container.querySelectorAll('[data-testid="team-activity-repo-group"]');
    expect(groups.length).toBe(2);
    expect(groups[0].getAttribute('data-repo')).toBe('alpha');
    expect(groups[1].getAttribute('data-repo')).toBe('zeta');
  });

  it('renders a status dot whose color reflects ansambel_status', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_r', row({ ansambel_status: 'running' }));
    const { container } = render(TeamActivityPanel);
    const dot = container.querySelector('[data-testid="team-activity-status-dot"]');
    expect(dot?.getAttribute('data-status')).toBe('running');
  });

  it('renders relative last_activity_at time', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_t', row({ last_activity_at: Date.now() - 5 * 60 * 1000 }));
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/5m ago|just now/i)).toBeTruthy();
  });

  it('click on a row calls teamActivity.select with the workspace_id', async () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_c', row({ workspace_id: 'ws_c' }));
    const { container } = render(TeamActivityPanel);
    const button = container.querySelector(
      '[data-testid="team-activity-row"][data-workspace-id="ws_c"]'
    ) as HTMLButtonElement;
    await fireEvent.click(button);
    expect(teamActivity.selectedWorkspaceId).toBe('ws_c');
  });

  it('Refresh button calls teamActivity.refresh', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    const { getByLabelText } = render(TeamActivityPanel);
    const button = getByLabelText(/refresh team activity/i);
    await fireEvent.click(button);
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
  });

  it('renders an error banner with a Retry button when status is error', async () => {
    teamActivity.status = 'error';
    teamActivity.error = 'offline';
    vi.mocked(invoke).mockResolvedValueOnce({ kind: 'rows', rows: [] });
    const { getByText, getByRole } = render(TeamActivityPanel);
    expect(getByText(/offline/i)).toBeTruthy();
    const retry = getByRole('button', { name: /retry/i });
    await fireEvent.click(retry);
    expect(invoke).toHaveBeenCalledWith('fetch_team_activity_rows');
  });

  it('persists collapse state to localStorage', async () => {
    const { getByLabelText } = render(TeamActivityPanel);
    const toggle = getByLabelText(/collapse team activity|expand team activity/i);
    await fireEvent.click(toggle);
    expect(localStorage.getItem('ansambel-team-activity-collapsed')).toBe('true');
    await fireEvent.click(toggle);
    expect(localStorage.getItem('ansambel-team-activity-collapsed')).toBe('false');
  });

  it('shows relative time in hours for activity older than 60 minutes', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_h', row({ last_activity_at: Date.now() - 3 * 60 * 60 * 1000 }));
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/3h ago/i)).toBeTruthy();
  });

  it('shows relative time in days for activity older than 24 hours', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_d', row({ last_activity_at: Date.now() - 2 * 24 * 60 * 60 * 1000 }));
    const { getByText } = render(TeamActivityPanel);
    expect(getByText(/2d ago/i)).toBeTruthy();
  });

  it('shows empty string for a zero epoch timestamp', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_z', row({ last_activity_at: 0 }));
    const { container } = render(TeamActivityPanel);
    // The time cell should be present but empty for epoch=0.
    const timeSpans = container.querySelectorAll('[data-testid="team-activity-row"] span.shrink-0');
    expect(timeSpans.length).toBeGreaterThan(0);
    expect(timeSpans[0]?.textContent?.trim()).toBe('');
  });

  it('falls back to "—" as the repo group label when repo_display_name is empty', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_nd', row({ repo_display_name: '' }));
    const { container } = render(TeamActivityPanel);
    const group = container.querySelector('[data-testid="team-activity-repo-group"]');
    expect(group?.getAttribute('data-repo')).toBe('—');
  });

  it('falls back to "idle" status dot when ansambel_status is empty', () => {
    teamActivity.status = 'idle';
    teamActivity.rows.set('ws_ns', row({ ansambel_status: '' }));
    const { container } = render(TeamActivityPanel);
    const dot = container.querySelector('[data-testid="team-activity-status-dot"]');
    expect(dot?.getAttribute('data-status')).toBe('idle');
  });
});
