import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import RepoSettingsDialog from './RepoSettingsDialog.svelte';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      listRepoBindings: vi.fn().mockResolvedValue({}),
      setRepoBinding: vi.fn(),
      deleteRepoBinding: vi.fn(),
      detectSchema: vi.fn(),
    },
  },
}));

vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
}));

import { larkBindings } from '$lib/stores/lark-bindings.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  larkBindings.bindings.clear();
});

describe('RepoSettingsDialog', () => {
  it('renders "Not connected" empty state by default', () => {
    render(RepoSettingsDialog, {
      props: {
        repoId: 'repo_x',
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    expect(screen.getByTestId('binding-empty')).toBeTruthy();
    expect(screen.getByTestId('connect-binding')).toBeTruthy();
  });

  it('renders connected state when binding exists', () => {
    larkBindings.bindings.set('repo_x', {
      app_token: 'bascntest12345',
      table_id: 'tbltest',
      field_mapping: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
      },
      status_value_mapping: { entries: {}, default_column: 'todo' },
      created_at: 0,
      updated_at: 0,
    });
    render(RepoSettingsDialog, {
      props: {
        repoId: 'repo_x',
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    expect(screen.getByTestId('binding-summary')).toBeTruthy();
    expect(screen.getByTestId('edit-binding')).toBeTruthy();
    expect(screen.getByTestId('disconnect-binding')).toBeTruthy();
  });

  it('Connect button opens wizard', async () => {
    render(RepoSettingsDialog, {
      props: {
        repoId: 'repo_x',
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    await fireEvent.click(screen.getByTestId('connect-binding'));
    expect(screen.getByTestId('lark-binding-wizard')).toBeTruthy();
  });

  it('Disconnect shows confirm dialog, calling delete on confirm', async () => {
    larkBindings.bindings.set('repo_x', {
      app_token: 'bascntest',
      table_id: 'tbltest',
      field_mapping: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
      },
      status_value_mapping: { entries: {}, default_column: 'todo' },
      created_at: 0,
      updated_at: 0,
    });
    render(RepoSettingsDialog, {
      props: {
        repoId: 'repo_x',
        repoName: 'my-repo',
        open: true,
        onClose: vi.fn(),
      },
    });
    await fireEvent.click(screen.getByTestId('disconnect-binding'));
    expect(screen.getByTestId('disconnect-confirm-backdrop')).toBeTruthy();
    const { api } = await import('$lib/ipc');
    vi.mocked(api.lark.deleteRepoBinding).mockResolvedValue(undefined);
    await fireEvent.click(screen.getByTestId('disconnect-confirm'));
    expect(api.lark.deleteRepoBinding).toHaveBeenCalledWith('repo_x');
  });

  it('Close button fires onClose', async () => {
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose },
    });
    await fireEvent.click(screen.getByTestId('repo-settings-close'));
    expect(onClose).toHaveBeenCalled();
  });
});
