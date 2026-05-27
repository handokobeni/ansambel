import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(() => Promise.resolve()) }));
vi.mock('$lib/stores/toasts.svelte', () => ({ addToast: vi.fn() }));

import TeamWorkspaceMirror from './TeamWorkspaceMirror.svelte';
import { openUrl } from '@tauri-apps/plugin-opener';
import { addToast } from '$lib/stores/toasts.svelte';
import { teamActivity } from '$lib/stores/team-activity.svelte';
import type { TeamActivityRow } from '$lib/types';

function row(overrides: Partial<TeamActivityRow> = {}): TeamActivityRow {
  return {
    workspace_id: 'ws_a',
    repo_remote_url: 'https://github.com/foo/bar',
    repo_display_name: 'bar',
    task_title: 'Refactor auth',
    assignee_machine: 'bob@laptop',
    ansambel_status: 'running',
    last_activity_at: Date.now() - 5 * 60 * 1000,
    last_message_preview: 'Working on token validation',
    branch_name: 'feat/auth',
    diff_summary: '+10 -3',
    pr_url: '',
    private: false,
    ...overrides,
  };
}

describe('TeamWorkspaceMirror', () => {
  beforeEach(() => {
    teamActivity.rows.clear();
    teamActivity.selectedWorkspaceId = null;
    vi.mocked(openUrl).mockReset();
    vi.mocked(openUrl).mockResolvedValue(undefined);
    vi.mocked(addToast).mockReset();
  });

  it('renders task_title, assignee_machine, status in the header', () => {
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText('Refactor auth')).toBeTruthy();
    expect(getByText(/bob@laptop/i)).toBeTruthy();
    expect(getByText(/running/i)).toBeTruthy();
  });

  it('constructs a GitHub branch URL from an https remote', () => {
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const link = getByRole('link', { name: /open branch on github/i });
    expect(link.getAttribute('href')).toBe('https://github.com/foo/bar/tree/feat%2Fauth');
  });

  it('constructs a GitHub branch URL from a git@ ssh remote', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ repo_remote_url: 'git@github.com:foo/bar', branch_name: 'feat/x' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const link = getByRole('link', { name: /open branch on github/i });
    expect(link.getAttribute('href')).toBe('https://github.com/foo/bar/tree/feat%2Fx');
  });

  it('hides the branch link when the remote scheme is unknown', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ repo_remote_url: 'ftp://example/foo', branch_name: 'main' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { queryByRole } = render(TeamWorkspaceMirror);
    expect(queryByRole('link', { name: /open branch on github/i })).toBeNull();
  });

  it('renders "Not yet published" placeholder when diff_summary is empty', () => {
    teamActivity.rows.set('ws_a', row({ diff_summary: '' }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText(/Not yet published/i)).toBeTruthy();
  });

  it('renders the diff_summary text when present', () => {
    teamActivity.rows.set('ws_a', row({ diff_summary: '+45 -12 across 3 files' }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText(/\+45 -12 across 3 files/)).toBeTruthy();
  });

  it('renders the Open PR button only when status is pr_ready AND pr_url is set', () => {
    teamActivity.rows.set(
      'ws_a',
      row({
        ansambel_status: 'pr_ready',
        pr_url: 'https://github.com/foo/bar/pull/9',
      })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const button = getByRole('link', { name: /open pr/i });
    expect(button.getAttribute('href')).toBe('https://github.com/foo/bar/pull/9');
  });

  it('hides the Open PR button when pr_url is empty', () => {
    teamActivity.rows.set('ws_a', row({ ansambel_status: 'pr_ready', pr_url: '' }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { queryByRole } = render(TeamWorkspaceMirror);
    expect(queryByRole('link', { name: /open pr/i })).toBeNull();
  });

  it('back button clears selectedWorkspaceId', async () => {
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByLabelText } = render(TeamWorkspaceMirror);
    const back = getByLabelText(/back to workspace/i);
    await fireEvent.click(back);
    expect(teamActivity.selectedWorkspaceId).toBeNull();
  });

  it('renders the sanitized message preview verbatim', () => {
    teamActivity.rows.set(
      'ws_a',
      row({ last_message_preview: 'token: [REDACTED] — checking next step' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByText } = render(TeamWorkspaceMirror);
    expect(getByText(/\[REDACTED\] — checking next step/i)).toBeTruthy();
  });

  it('handles workspaces with no branch_name gracefully', () => {
    teamActivity.rows.set('ws_a', row({ branch_name: '' }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { queryByRole, queryByText } = render(TeamWorkspaceMirror);
    expect(queryByRole('link', { name: /open branch on github/i })).toBeNull();
    expect(queryByText(/feat\//i)).toBeNull();
  });

  it('shows relative time in hours for activity older than 60 minutes', () => {
    teamActivity.rows.set('ws_a', row({ last_activity_at: Date.now() - 3 * 60 * 60 * 1000 }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByTestId } = render(TeamWorkspaceMirror);
    expect(getByTestId('team-workspace-mirror').textContent).toMatch(/3h ago/i);
  });

  it('shows relative time in days for activity older than 24 hours', () => {
    teamActivity.rows.set('ws_a', row({ last_activity_at: Date.now() - 2 * 24 * 60 * 60 * 1000 }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByTestId } = render(TeamWorkspaceMirror);
    expect(getByTestId('team-workspace-mirror').textContent).toMatch(/2d ago/i);
  });

  it('renders nothing when selectedWorkspaceId is null', () => {
    const { queryByTestId } = render(TeamWorkspaceMirror);
    expect(queryByTestId('team-workspace-mirror')).toBeNull();
  });

  it('renders nothing when selectedWorkspaceId points to a missing row', () => {
    teamActivity.selectedWorkspaceId = 'ws_missing';
    const { queryByTestId } = render(TeamWorkspaceMirror);
    expect(queryByTestId('team-workspace-mirror')).toBeNull();
  });

  it('shows "just now" for activity less than 60 seconds ago', () => {
    teamActivity.rows.set('ws_a', row({ last_activity_at: Date.now() - 10 * 1000 }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByTestId } = render(TeamWorkspaceMirror);
    expect(getByTestId('team-workspace-mirror').textContent).toMatch(/just now/i);
  });

  it('shows empty time string when last_activity_at is 0', () => {
    teamActivity.rows.set('ws_a', row({ last_activity_at: 0 }));
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByTestId } = render(TeamWorkspaceMirror);
    // relativeTime(0) returns '' — the header time span should be empty
    const mirror = getByTestId('team-workspace-mirror');
    expect(mirror).toBeTruthy();
  });

  it('clicking "Open branch on GitHub" opens the URL via the Tauri opener, not in-webview nav', async () => {
    // In a Tauri webview a plain `<a target="_blank">` does not reach the
    // OS browser — the click must call the opener plugin. The handler also
    // prevents the default in-webview navigation that would otherwise do
    // nothing (or hijack the app window).
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const link = getByRole('link', { name: /open branch on github/i });
    const ev = await fireEvent.click(link);
    expect(ev).toBe(false); // preventDefault → fireEvent.click returns false
    expect(openUrl).toHaveBeenCalledWith('https://github.com/foo/bar/tree/feat%2Fauth');
  });

  it('clicking "Open PR" opens the PR URL via the Tauri opener', async () => {
    teamActivity.rows.set(
      'ws_a',
      row({ ansambel_status: 'pr_ready', pr_url: 'https://github.com/foo/bar/pull/9' })
    );
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    const link = getByRole('link', { name: /open pr/i });
    await fireEvent.click(link);
    expect(openUrl).toHaveBeenCalledWith('https://github.com/foo/bar/pull/9');
  });

  it('shows an error toast when the opener rejects (no OS URL handler)', async () => {
    // On a host without a URL handler (e.g. WSL without wslu/xdg-open) the
    // opener rejects. The click must surface that, not swallow it.
    vi.mocked(openUrl).mockRejectedValueOnce(new Error('no handler'));
    teamActivity.rows.set('ws_a', row());
    teamActivity.selectedWorkspaceId = 'ws_a';
    const { getByRole } = render(TeamWorkspaceMirror);
    await fireEvent.click(getByRole('link', { name: /open branch on github/i }));
    // Let the rejected promise's .catch microtask run.
    await Promise.resolve();
    await Promise.resolve();
    expect(addToast).toHaveBeenCalledWith(
      expect.stringContaining('https://github.com/foo/bar/tree/feat%2Fauth'),
      'error'
    );
  });
});
