// Svelte actions for ansambel UI. Hard rule: tooltips must use the
// `tooltip` action and append to document.body, never the native `title`
// attribute, so they cannot be clipped by overflow:hidden ancestors.

export interface TooltipOptions {
  /** Primary label text. Falsy text is a no-op (nothing renders). */
  text: string;
  /** Optional keyboard shortcut shown as a kbd badge to the right. */
  shortcut?: string;
  /** Preferred placement; flips automatically if it would clip the viewport. */
  placement?: 'top' | 'bottom';
}

const TOOLTIP_DELAY_MS = 100;
const TOOLTIP_CLASS = 'ansambel-tooltip';
const KBD_CLASS = 'ansambel-tooltip-kbd';
const TEXT_CLASS = 'ansambel-tooltip-text';
const GAP = 6;
const EDGE_PAD = 8;

export function tooltip(node: HTMLElement, options: TooltipOptions) {
  let opts = options;
  let el: HTMLDivElement | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  // Native title would still trigger the OS tooltip on hover, doubling
  // up on our custom one — strip it eagerly.
  node.removeAttribute('title');

  function show(): void {
    if (el || !opts.text) return;
    el = document.createElement('div');
    el.className = TOOLTIP_CLASS;

    const label = document.createElement('span');
    label.className = TEXT_CLASS;
    label.textContent = opts.text;
    el.appendChild(label);

    if (opts.shortcut) {
      const kbd = document.createElement('kbd');
      kbd.className = KBD_CLASS;
      kbd.textContent = opts.shortcut;
      el.appendChild(kbd);
    }

    document.body.appendChild(el);
    position();
  }

  function position(): void {
    /* v8 ignore next 1 — defensive guard: position() is only reachable
       from show()/update() after the tooltip element has been mounted. */
    if (!el) return;
    const rect = node.getBoundingClientRect();
    const tt = el.getBoundingClientRect();
    const placement = opts.placement ?? 'top';

    /* v8 ignore next 2 — SSR/Node guard: not reachable in jsdom. */
    const viewportW = typeof window === 'undefined' ? 1024 : window.innerWidth;
    const viewportH = typeof window === 'undefined' ? 768 : window.innerHeight;

    let left = rect.left + rect.width / 2 - tt.width / 2;
    left = Math.max(EDGE_PAD, Math.min(left, viewportW - tt.width - EDGE_PAD));

    let top: number;
    let actualPlacement = placement;
    if (placement === 'top') {
      top = rect.top - tt.height - GAP;
      if (top < EDGE_PAD) {
        top = rect.bottom + GAP;
        actualPlacement = 'bottom';
      }
    } else {
      top = rect.bottom + GAP;
      if (top + tt.height > viewportH - EDGE_PAD) {
        top = rect.top - tt.height - GAP;
        actualPlacement = 'top';
      }
    }

    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
    el.dataset.placement = actualPlacement;
  }

  function hide(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (el) {
      el.remove();
      el = null;
    }
  }

  function onEnter(): void {
    timer = setTimeout(show, TOOLTIP_DELAY_MS);
  }

  function onLeave(): void {
    hide();
  }

  function onPointerDown(): void {
    hide();
  }

  node.addEventListener('pointerenter', onEnter);
  node.addEventListener('pointerleave', onLeave);
  node.addEventListener('pointerdown', onPointerDown);

  return {
    update(newOpts: TooltipOptions): void {
      opts = newOpts;
      node.removeAttribute('title');
      if (!el) return;
      const label = el.querySelector<HTMLElement>(`.${TEXT_CLASS}`);
      if (label) label.textContent = opts.text;
      const existingKbd = el.querySelector<HTMLElement>(`.${KBD_CLASS}`);
      if (opts.shortcut) {
        if (existingKbd) {
          existingKbd.textContent = opts.shortcut;
        } else {
          const kbd = document.createElement('kbd');
          kbd.className = KBD_CLASS;
          kbd.textContent = opts.shortcut;
          el.appendChild(kbd);
        }
      } else if (existingKbd) {
        existingKbd.remove();
      }
      position();
    },
    destroy(): void {
      hide();
      node.removeEventListener('pointerenter', onEnter);
      node.removeEventListener('pointerleave', onLeave);
      node.removeEventListener('pointerdown', onPointerDown);
    },
  };
}
