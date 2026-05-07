<!-- src/lib/components/ResizeHandle.svelte -->
<script lang="ts">
  interface Props {
    /** Called with the incremental pixel delta since the last pointer move. */
    onResize: (delta: number) => void;
    /** Called once when the drag ends. */
    onResizeEnd?: () => void;
    /** Orientation of the handle; horizontal handles resize widths, vertical heights. */
    direction?: 'horizontal' | 'vertical';
  }

  const { onResize, onResizeEnd, direction = 'horizontal' }: Props = $props();

  let dragging = $state(false);
  let startPos = 0;

  function handlePointerDown(e: Event): void {
    e.preventDefault?.();
    const pe = e as PointerEvent;
    dragging = true;
    startPos = direction === 'horizontal' ? pe.clientX : pe.clientY;
    const target = e.currentTarget as HTMLElement;
    // Older test runtimes can omit pointer-capture APIs; guard so the
    // drag can still proceed.
    target.setPointerCapture?.(pe.pointerId);
  }

  function handlePointerMove(e: Event): void {
    if (!dragging) return;
    const pe = e as PointerEvent;
    const current = direction === 'horizontal' ? pe.clientX : pe.clientY;
    const delta = current - startPos;
    onResize(delta);
    startPos = current;
  }

  function handlePointerUp(e: Event): void {
    if (!dragging) return;
    dragging = false;
    const target = e.currentTarget as HTMLElement;
    try {
      target.releasePointerCapture?.((e as PointerEvent).pointerId);
    } catch {
      // Releasing an already-released or unknown pointer is fine; the
      // drag has ended either way.
    }
    onResizeEnd?.();
  }
</script>

<div
  class="resize-handle shrink-0 relative z-10 touch-none {direction === 'horizontal'
    ? 'horizontal w-px cursor-col-resize bg-[var(--border)]'
    : 'vertical h-px cursor-row-resize bg-[var(--border)]'} hover:bg-[var(--accent)]"
  class:dragging
  role="separator"
  aria-orientation={direction}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
></div>
