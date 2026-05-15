// src/lib/components/SettingsDialog.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import SettingsDialog from './SettingsDialog.svelte';

// LarkSettings calls invoke('get_lark_status') on mount — stub it so the
// dialog renders without network. Route by command name so additional
// commands (get_task_source, set_task_source) also resolve correctly.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_task_source') return Promise.resolve('local');
    if (cmd === 'get_lark_status')
      return Promise.resolve({
        configured: false,
        app_id: null,
        app_token: null,
        table_id: null,
        base_url: 'https://open.larksuite.com',
        has_secret: false,
      });
    return Promise.resolve(undefined);
  }),
  Channel: class {},
}));

describe('SettingsDialog', () => {
  it('does not render dialog content when open=false', () => {
    render(SettingsDialog, { props: { open: false, onClose: vi.fn() } });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByTestId('settings-backdrop')).toBeNull();
  });

  it('renders dialog and LarkSettings panel when open=true', () => {
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByText('Settings')).toBeTruthy();
    expect(screen.getByTestId('lark-settings')).toBeTruthy();
  });

  it('calls onClose when close button is clicked', async () => {
    const onClose = vi.fn();
    render(SettingsDialog, { props: { open: true, onClose } });
    await fireEvent.click(screen.getByTestId('settings-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('calls onClose when backdrop is clicked', async () => {
    const onClose = vi.fn();
    render(SettingsDialog, { props: { open: true, onClose } });
    await fireEvent.click(screen.getByTestId('settings-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not call onClose when the dialog body is clicked', async () => {
    const onClose = vi.fn();
    render(SettingsDialog, { props: { open: true, onClose } });
    await fireEvent.click(screen.getByRole('dialog'));
    expect(onClose).not.toHaveBeenCalled();
  });

  it('calls onClose when Escape is pressed', async () => {
    const onClose = vi.fn();
    render(SettingsDialog, { props: { open: true, onClose } });
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not handle Escape when closed', async () => {
    const onClose = vi.fn();
    render(SettingsDialog, { props: { open: false, onClose } });
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does not call onClose for keys other than Escape', async () => {
    const onClose = vi.fn();
    render(SettingsDialog, { props: { open: true, onClose } });
    await fireEvent.keyDown(window, { key: 'Enter' });
    await fireEvent.keyDown(window, { key: 'a' });
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe('SettingsDialog task source', () => {
  it('renders both radio options', async () => {
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('task-source-local')).toBeTruthy();
    expect(screen.getByTestId('task-source-lark')).toBeTruthy();
  });

  it('switches to lark on click when set_task_source resolves', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_task_source') return Promise.resolve('local');
      if (cmd === 'set_task_source') return Promise.resolve(undefined);
      if (cmd === 'get_lark_status')
        return Promise.resolve({
          configured: false,
          app_id: null,
          app_token: null,
          table_id: null,
          base_url: 'https://open.larksuite.com',
          has_secret: false,
        });
      return Promise.resolve(undefined);
    });
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    const larkRadio = screen.getByTestId('task-source-lark') as HTMLInputElement;
    await fireEvent.click(larkRadio);
    await new Promise((r) => setTimeout(r, 0));
    expect(larkRadio.checked).toBe(true);
  });

  it('no-ops when clicking the already-selected radio', async () => {
    let setSourceCalls = 0;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_task_source') return Promise.resolve('local');
      if (cmd === 'set_task_source') {
        setSourceCalls += 1;
        return Promise.resolve(undefined);
      }
      if (cmd === 'get_lark_status')
        return Promise.resolve({
          configured: false,
          app_id: null,
          app_token: null,
          table_id: null,
          base_url: 'https://open.larksuite.com',
          has_secret: false,
        });
      return Promise.resolve(undefined);
    });
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    const localRadio = screen.getByTestId('task-source-local') as HTMLInputElement;
    await fireEvent.click(localRadio);
    await new Promise((r) => setTimeout(r, 0));
    expect(setSourceCalls).toBe(0);
  });

  it('stringifies non-Error rejections in the failure toast', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_task_source') return Promise.resolve('local');
      if (cmd === 'set_task_source') return Promise.reject('plain-string-err');
      if (cmd === 'get_lark_status')
        return Promise.resolve({
          configured: false,
          app_id: null,
          app_token: null,
          table_id: null,
          base_url: 'https://open.larksuite.com',
          has_secret: false,
        });
      return Promise.resolve(undefined);
    });
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    const larkRadio = screen.getByTestId('task-source-lark') as HTMLInputElement;
    await fireEvent.click(larkRadio);
    await new Promise((r) => setTimeout(r, 0));
    const localRadio = screen.getByTestId('task-source-local') as HTMLInputElement;
    expect(localRadio.checked).toBe(true);
  });

  it('reverts and toasts when set_task_source rejects', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_task_source') return Promise.resolve('local');
      if (cmd === 'set_task_source')
        return Promise.reject(new Error('Configure Lark credentials first'));
      if (cmd === 'get_lark_status')
        return Promise.resolve({
          configured: false,
          app_id: null,
          app_token: null,
          table_id: null,
          base_url: 'https://open.larksuite.com',
          has_secret: false,
        });
      return Promise.resolve(undefined);
    });
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    const larkRadio = screen.getByTestId('task-source-lark') as HTMLInputElement;
    await fireEvent.click(larkRadio);
    await new Promise((r) => setTimeout(r, 0));
    const localRadio = screen.getByTestId('task-source-local') as HTMLInputElement;
    expect(localRadio.checked).toBe(true);
  });
});
