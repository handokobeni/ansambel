import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { tooltip } from './actions';

const TOOLTIP_DELAY_MS = 100;
const TOOLTIP_CLASS = 'ansambel-tooltip';
const KBD_CLASS = 'ansambel-tooltip-kbd';

function fireEvent(node: HTMLElement, type: string): void {
  // jsdom doesn't fully implement PointerEvent. Plain Event with bubble is
  // sufficient for our handlers, which only need the type and target.
  node.dispatchEvent(new Event(type, { bubbles: true }));
}

describe('tooltip action', () => {
  let host: HTMLElement;

  beforeEach(() => {
    vi.useFakeTimers();
    host = document.createElement('button');
    host.textContent = 'host';
    document.body.appendChild(host);
  });

  afterEach(() => {
    document.body.replaceChildren();
    vi.useRealTimers();
  });

  it('removes any pre-existing native title attribute', () => {
    host.setAttribute('title', 'native title that should be killed');
    tooltip(host, { text: 'real tooltip' });
    expect(host.hasAttribute('title')).toBe(false);
  });

  it('shows a tooltip element on pointer enter after delay', () => {
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const el = document.querySelector(`.${TOOLTIP_CLASS}`);
    expect(el).not.toBeNull();
    expect(el?.textContent).toContain('hello');
    // Tooltip is attached to body so it can never be clipped by overflow.
    expect(el?.parentElement).toBe(document.body);
  });

  it('renders a kbd badge when shortcut is provided', () => {
    tooltip(host, { text: 'hello', shortcut: '⌘K' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const kbd = document.querySelector(`.${KBD_CLASS}`);
    expect(kbd).not.toBeNull();
    expect(kbd?.textContent).toBe('⌘K');
  });

  it('does not render kbd when shortcut is omitted', () => {
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${KBD_CLASS}`)).toBeNull();
  });

  it('removes the tooltip on pointer leave', () => {
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).not.toBeNull();
    fireEvent(host, 'pointerleave');
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
  });

  it('cancels pending show on early pointer leave', () => {
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    fireEvent(host, 'pointerleave');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS * 2);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
  });

  it('removes tooltip on pointer down (click dismiss)', () => {
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).not.toBeNull();
    fireEvent(host, 'pointerdown');
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
  });

  it('does nothing for empty text', () => {
    tooltip(host, { text: '' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
  });

  it('update() refreshes label and shortcut on the live tooltip', () => {
    const handle = tooltip(host, { text: 'old' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    handle.update({ text: 'new', shortcut: '⌘P' });
    const el = document.querySelector(`.${TOOLTIP_CLASS}`);
    expect(el?.textContent).toContain('new');
    expect(document.querySelector(`.${KBD_CLASS}`)?.textContent).toBe('⌘P');
    handle.update({ text: 'no shortcut' });
    expect(document.querySelector(`.${KBD_CLASS}`)).toBeNull();
  });

  it('update() also strips a freshly-set native title attribute', () => {
    const handle = tooltip(host, { text: 'hello' });
    host.setAttribute('title', 'should be killed');
    handle.update({ text: 'hello' });
    expect(host.hasAttribute('title')).toBe(false);
  });

  it('destroy() removes the live tooltip and detaches listeners', () => {
    const handle = tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).not.toBeNull();
    handle.destroy();
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
  });

  it('positions tooltip relative to host (writes inline left/top)', () => {
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const el = document.querySelector<HTMLElement>(`.${TOOLTIP_CLASS}`);
    expect(el?.style.left).not.toBe('');
    expect(el?.style.top).not.toBe('');
    expect(['top', 'bottom']).toContain(el?.dataset.placement);
  });

  it('honors placement=bottom when there is room below', () => {
    // Force the host to report a top-of-viewport rect so the natural
    // placement is bottom and the bottom-branch in position() runs.
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({
      top: 10,
      bottom: 30,
      left: 100,
      right: 180,
      width: 80,
      height: 20,
      x: 100,
      y: 10,
      toJSON: () => ({}),
    } as DOMRect);
    tooltip(host, { text: 'hello', placement: 'bottom' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const el = document.querySelector<HTMLElement>(`.${TOOLTIP_CLASS}`);
    expect(el?.dataset.placement).toBe('bottom');
  });

  it('flips placement=bottom to top when bottom would clip the viewport', () => {
    // Pin host to the bottom of the viewport so placement=bottom would
    // put the tooltip below the visible area, forcing the flip branch.
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      writable: true,
      value: 400,
    });
    vi.spyOn(host, 'getBoundingClientRect').mockReturnValue({
      top: 360,
      bottom: 395,
      left: 50,
      right: 130,
      width: 80,
      height: 35,
      x: 50,
      y: 360,
      toJSON: () => ({}),
    } as DOMRect);
    tooltip(host, { text: 'hello', placement: 'bottom' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    const el = document.querySelector<HTMLElement>(`.${TOOLTIP_CLASS}`);
    expect(el?.dataset.placement).toBe('top');
  });

  it('update() before show is a safe no-op (no live tooltip)', () => {
    const handle = tooltip(host, { text: 'first' });
    // Update without ever firing pointerenter — branch where el is null.
    handle.update({ text: 'second', shortcut: '⌘B' });
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)).toBeNull();
    // Subsequent show should reflect the latest options.
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${TOOLTIP_CLASS}`)?.textContent).toContain('second');
    expect(document.querySelector(`.${KBD_CLASS}`)?.textContent).toBe('⌘B');
  });

  it('a second enter while a tooltip is already shown does not duplicate it', () => {
    // Covers the `if (el ...) return;` early-return in show(): a stray
    // enter from a child element shouldn't stack tooltips on top of each
    // other.
    tooltip(host, { text: 'hello' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelectorAll(`.${TOOLTIP_CLASS}`).length).toBe(1);
  });

  it('update() rewrites the kbd badge text in place when one already exists', () => {
    // Covers the `existingKbd.textContent = opts.shortcut` branch: the
    // tooltip stays mounted while a parent re-renders the same shortcut
    // with a different label.
    const handle = tooltip(host, { text: 'hi', shortcut: '⌘A' });
    fireEvent(host, 'pointerenter');
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    expect(document.querySelector(`.${KBD_CLASS}`)?.textContent).toBe('⌘A');
    handle.update({ text: 'hi', shortcut: '⌘B' });
    // Same kbd node, new text — branch confirms we did not destroy + recreate.
    const kbds = document.querySelectorAll(`.${KBD_CLASS}`);
    expect(kbds.length).toBe(1);
    expect(kbds[0].textContent).toBe('⌘B');
  });
});
