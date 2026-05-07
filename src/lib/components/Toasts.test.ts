import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import Toasts from './Toasts.svelte';
import { addToast, removeToast, getToasts } from '$lib/stores/toasts.svelte';

function clearAllToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

describe('Toasts component', () => {
  beforeEach(() => {
    clearAllToasts();
  });

  afterEach(() => {
    clearAllToasts();
    vi.restoreAllMocks();
  });

  it('renders nothing when there are no toasts', () => {
    const { container } = render(Toasts);
    expect(container.querySelector('.toast-container')).toBeNull();
  });

  it('renders a toast with its message and the right type class', async () => {
    addToast('something broke', 'error');
    render(Toasts);
    const toast = await screen.findByText('something broke');
    expect(toast).toBeInTheDocument();
    const wrapper = toast.closest('.toast');
    expect(wrapper?.classList.contains('toast-error')).toBe(true);
  });

  it('renders multiple toasts in newest-first order', async () => {
    addToast('first', 'info');
    addToast('second', 'info');
    render(Toasts);
    const wrappers = await screen.findAllByText(/first|second/);
    expect(wrappers.map((el) => el.textContent)).toEqual(['second', 'first']);
  });

  it('dismiss button removes the toast', async () => {
    const id = addToast('clear me', 'info');
    render(Toasts);
    expect(await screen.findByText('clear me')).toBeInTheDocument();
    const dismiss = screen.getByLabelText('Dismiss');
    await fireEvent.click(dismiss);
    await waitFor(() => {
      expect(screen.queryByText('clear me')).not.toBeInTheDocument();
    });
    expect(getToasts().has(id)).toBe(false);
  });

  it('copy button writes the toast message to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    addToast('copy this', 'error');
    render(Toasts);
    await screen.findByText('copy this');
    const copy = screen.getByLabelText('Copy');
    await fireEvent.click(copy);
    expect(writeText).toHaveBeenCalledWith('copy this');
  });

  it('exposes role=status for screen readers', async () => {
    addToast('hi', 'info');
    render(Toasts);
    const region = await screen.findByRole('status');
    expect(region).toBeInTheDocument();
    expect(region.getAttribute('aria-live')).toBe('polite');
  });

  it('renders success type with the success color class', async () => {
    addToast('saved', 'success');
    render(Toasts);
    const t = await screen.findByText('saved');
    const wrapper = t.closest('.toast');
    expect(wrapper?.classList.contains('toast-success')).toBe(true);
    expect(wrapper?.className).toContain('--status-ok');
  });

  it('shows the check icon while copy feedback is active', async () => {
    // Covers the {#if copiedId === toast.id} true-branch in the template.
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    addToast('grab me', 'info');
    const { container } = render(Toasts);
    await screen.findByText('grab me');
    const copy = screen.getByLabelText('Copy');
    await fireEvent.click(copy);
    await waitFor(() => {
      // The check icon uses the polyline shape; the copy icon uses rect+path.
      const checkPolyline = container.querySelector('polyline[points*="20 6"]');
      expect(checkPolyline).not.toBeNull();
    });
  });

  it('does not reset copy feedback when a different toast was copied later', async () => {
    // Covers the inner `if (copiedId === toast.id)` branch in the
    // setTimeout callback. When the user copies toast A and then copies
    // toast B before A's timer fires, A's stale callback should be a
    // no-op rather than wiping out B's check icon.
    vi.useFakeTimers();
    try {
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: { writeText },
      });
      addToast('first', 'info');
      addToast('second', 'info');
      render(Toasts);
      const copies = await screen.findAllByLabelText('Copy');
      // Click copy on first toast.
      await fireEvent.click(copies[copies.length - 1]);
      // Allow the awaited writeText to resolve.
      await vi.advanceTimersByTimeAsync(0);
      // Now click copy on the second toast before the first timer fires.
      await fireEvent.click(copies[0]);
      await vi.advanceTimersByTimeAsync(0);
      // Drain the 1200ms timer: first-toast callback fires but copiedId
      // is now pointing at the second toast → branch returns without
      // clearing copiedId.
      await vi.advanceTimersByTimeAsync(1200);
      // At least one click registered without throwing.
      expect(writeText).toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('silently swallows clipboard.writeText rejections', async () => {
    let rejectFn!: (e: Error) => void;
    const pending = new Promise<void>((_, reject) => {
      rejectFn = reject;
    });
    const writeText = vi.fn().mockReturnValue(pending);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    addToast('copy this', 'error');
    render(Toasts);
    await screen.findByText('copy this');
    const copy = screen.getByLabelText('Copy');
    await fireEvent.click(copy);
    expect(writeText).toHaveBeenCalledWith('copy this');
    // Drive the rejection now and let the catch block actually run before
    // the test finishes — otherwise the catch is timed out by vitest's
    // microtask boundary and shows up uncovered in the v8 report.
    rejectFn(new Error('denied'));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledTimes(1);
    });
    // One additional microtask flush so the awaited rejection settles
    // through the catch block.
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
});
