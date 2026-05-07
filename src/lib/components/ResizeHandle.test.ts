// src/lib/components/ResizeHandle.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import ResizeHandle from './ResizeHandle.svelte';

function dispatch(node: Element, type: string, init: PointerEventInit = {}): void {
  const ev = new Event(type, { bubbles: true });
  // jsdom does not implement PointerEvent; copy clientX/Y on the bare Event so
  // handlers reading them get sensible numbers.
  Object.assign(ev, init, { pointerId: init.pointerId ?? 1 });
  node.dispatchEvent(ev);
}

describe('ResizeHandle', () => {
  it('renders with role=separator and the requested orientation', () => {
    const { container } = render(ResizeHandle, {
      props: { onResize: vi.fn(), direction: 'horizontal' },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]');
    expect(handle).not.toBeNull();
    expect(handle?.getAttribute('aria-orientation')).toBe('horizontal');
    expect(handle?.classList.contains('horizontal')).toBe(true);
  });

  it('defaults to horizontal orientation when direction is omitted', () => {
    const { container } = render(ResizeHandle, { props: { onResize: vi.fn() } });
    const handle = container.querySelector<HTMLElement>('[role="separator"]');
    expect(handle?.getAttribute('aria-orientation')).toBe('horizontal');
  });

  it('renders vertically when direction=vertical', () => {
    const { container } = render(ResizeHandle, {
      props: { onResize: vi.fn(), direction: 'vertical' },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]');
    expect(handle?.getAttribute('aria-orientation')).toBe('vertical');
    expect(handle?.classList.contains('vertical')).toBe(true);
  });

  it('reports incremental delta via onResize while dragging horizontally', () => {
    const onResize = vi.fn();
    const { container } = render(ResizeHandle, {
      props: { onResize, direction: 'horizontal' },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    // Stub setPointerCapture / releasePointerCapture which jsdom omits.
    handle.setPointerCapture = vi.fn();
    handle.releasePointerCapture = vi.fn();

    dispatch(handle, 'pointerdown', { clientX: 100 });
    dispatch(handle, 'pointermove', { clientX: 130 });
    dispatch(handle, 'pointermove', { clientX: 145 });
    dispatch(handle, 'pointerup', { clientX: 145 });

    expect(onResize).toHaveBeenNthCalledWith(1, 30);
    expect(onResize).toHaveBeenNthCalledWith(2, 15);
    expect(onResize).toHaveBeenCalledTimes(2);
  });

  it('reports vertical delta when direction=vertical', () => {
    const onResize = vi.fn();
    const { container } = render(ResizeHandle, {
      props: { onResize, direction: 'vertical' },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    handle.setPointerCapture = vi.fn();
    handle.releasePointerCapture = vi.fn();

    dispatch(handle, 'pointerdown', { clientY: 200 });
    dispatch(handle, 'pointermove', { clientY: 220 });
    dispatch(handle, 'pointerup', { clientY: 220 });

    expect(onResize).toHaveBeenCalledWith(20);
  });

  it('ignores pointermove events that arrive before pointerdown', () => {
    const onResize = vi.fn();
    const { container } = render(ResizeHandle, { props: { onResize } });
    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    dispatch(handle, 'pointermove', { clientX: 50 });
    expect(onResize).not.toHaveBeenCalled();
  });

  it('calls onResizeEnd exactly once when the drag completes', () => {
    const onResize = vi.fn();
    const onResizeEnd = vi.fn();
    const { container } = render(ResizeHandle, {
      props: { onResize, onResizeEnd, direction: 'horizontal' },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    handle.setPointerCapture = vi.fn();
    handle.releasePointerCapture = vi.fn();

    dispatch(handle, 'pointerdown', { clientX: 0 });
    dispatch(handle, 'pointerup', { clientX: 10 });
    expect(onResizeEnd).toHaveBeenCalledTimes(1);
  });

  it('does not invoke onResizeEnd when pointerup arrives without pointerdown', () => {
    const onResizeEnd = vi.fn();
    const { container } = render(ResizeHandle, {
      props: { onResize: vi.fn(), onResizeEnd },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    dispatch(handle, 'pointerup', { clientX: 0 });
    expect(onResizeEnd).not.toHaveBeenCalled();
  });

  it('handles release when releasePointerCapture is missing', () => {
    const onResizeEnd = vi.fn();
    const { container } = render(ResizeHandle, {
      props: { onResize: vi.fn(), onResizeEnd },
    });
    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    handle.setPointerCapture = vi.fn();
    // Simulate environments where releasePointerCapture throws — handler
    // must swallow it so the drag still cleans up.
    handle.releasePointerCapture = vi.fn(() => {
      throw new Error('release failed');
    });
    dispatch(handle, 'pointerdown', { clientX: 0 });
    expect(() => dispatch(handle, 'pointerup', { clientX: 5 })).not.toThrow();
    expect(onResizeEnd).toHaveBeenCalledTimes(1);
  });
});
