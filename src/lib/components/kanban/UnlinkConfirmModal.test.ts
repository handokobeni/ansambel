// src/lib/components/kanban/UnlinkConfirmModal.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import UnlinkConfirmModal from './UnlinkConfirmModal.svelte';

describe('UnlinkConfirmModal', () => {
  it('renders the workspace title and warning text', () => {
    const { getByTestId } = render(UnlinkConfirmModal, {
      props: {
        open: true,
        workspaceTitle: 'payment-refactor',
        onConfirm: vi.fn(),
        onCancel: vi.fn(),
      },
    });
    expect(getByTestId('unlink-modal-text').textContent).toMatch(/payment-refactor/);
    expect(getByTestId('unlink-modal-text').textContent).toMatch(/empty/i);
  });

  it('Confirm fires onConfirm; Cancel fires onCancel', async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    const { getByTestId } = render(UnlinkConfirmModal, {
      props: { open: true, workspaceTitle: 'W', onConfirm, onCancel },
    });
    await fireEvent.click(getByTestId('unlink-modal-confirm'));
    expect(onConfirm).toHaveBeenCalled();
    await fireEvent.click(getByTestId('unlink-modal-cancel'));
    expect(onCancel).toHaveBeenCalled();
  });
});
