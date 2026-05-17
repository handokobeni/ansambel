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

  it('Backdrop click fires onClose', async () => {
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose },
    });
    await fireEvent.click(screen.getByTestId('repo-settings-backdrop'));
    expect(onClose).toHaveBeenCalled();
  });

  it('Escape on root (no sub-state) fires onClose', async () => {
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose },
    });
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('Escape while editing wizard closes wizard, not dialog', async () => {
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose },
    });
    await fireEvent.click(screen.getByTestId('connect-binding'));
    expect(screen.getByTestId('lark-binding-wizard')).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('lark-binding-wizard')).toBeNull();
  });

  it('Escape while disconnect-confirm is open closes confirm, not dialog', async () => {
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
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose },
    });
    await fireEvent.click(screen.getByTestId('disconnect-binding'));
    expect(screen.getByTestId('disconnect-confirm-backdrop')).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('disconnect-confirm-backdrop')).toBeNull();
  });

  it('handleSaveBinding sets binding and adds success toast', async () => {
    const { api } = await import('$lib/ipc');
    const { addToast } = await import('$lib/stores/toasts.svelte');
    vi.mocked(api.lark.setRepoBinding).mockResolvedValue(undefined);
    const noStatusProposal = {
      fields: [{ field_id: 'fld_t', field_name: 'Task name', type: 1, is_primary: true }],
      suggested: {
        title: { field_id: 'fld_t', field_name: 'Task name' },
        description: null,
        status: null,
        order: null,
      },
      status_options: [],
      suggested_status_values: { entries: {}, default_column: 'todo' },
    };
    vi.mocked(api.lark.detectSchema).mockResolvedValueOnce(noStatusProposal as never);
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose: vi.fn() },
    });
    // Open the wizard
    await fireEvent.click(screen.getByTestId('connect-binding'));
    expect(screen.getByTestId('lark-binding-wizard')).toBeTruthy();
    await fireEvent.input(screen.getByTestId('wizard-app-token'), {
      target: { value: 'bascn_tok' },
    });
    await fireEvent.input(screen.getByTestId('wizard-table-id'), {
      target: { value: 'tbl_1' },
    });
    await fireEvent.click(screen.getByTestId('wizard-detect'));
    await new Promise((r) => setTimeout(r, 0));
    // Step 2 rendered; status is not single-select so "Save & Sync" goes directly
    await fireEvent.click(screen.getByTestId('wizard-continue'));
    await new Promise((r) => setTimeout(r, 0));
    // Wizard should be closed and toast shown
    expect(screen.queryByTestId('lark-binding-wizard')).toBeNull();
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('my-repo'), 'success');
  });

  it('Disconnect error path does not crash the dialog', async () => {
    const { api } = await import('$lib/ipc');
    vi.mocked(api.lark.deleteRepoBinding).mockRejectedValue(new Error('IPC fail'));
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
      props: { repoId: 'repo_x', repoName: 'my-repo', open: true, onClose: vi.fn() },
    });
    await fireEvent.click(screen.getByTestId('disconnect-binding'));
    expect(screen.getByTestId('disconnect-confirm-backdrop')).toBeTruthy();
    await fireEvent.click(screen.getByTestId('disconnect-confirm'));
    await new Promise((r) => setTimeout(r, 0));
    // Dialog still open (no crash) — the catch in handleDisconnect swallowed the error
    expect(screen.getByTestId('repo-settings-backdrop')).toBeTruthy();
  });

  it('does not register keydown handler when open=false', async () => {
    const onClose = vi.fn();
    render(RepoSettingsDialog, {
      props: { repoId: 'repo_x', repoName: 'my-repo', open: false, onClose },
    });
    // dialog is not rendered; Escape should not call onClose
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });
});
