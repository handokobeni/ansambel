// src/lib/components/SettingsDialog.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import SettingsDialog from './SettingsDialog.svelte';

// LarkSettings calls invoke('get_lark_status') on mount — stub it so the
// dialog renders without network.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() =>
    Promise.resolve({
      configured: false,
      app_id: null,
      app_token: null,
      table_id: null,
      base_url: 'https://open.larksuite.com',
      has_secret: false,
    })
  ),
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
