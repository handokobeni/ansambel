// src/lib/keyboard.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ShortcutRegistry } from './keyboard';

describe('ShortcutRegistry', () => {
  let registry: ShortcutRegistry;
  let listeners: Array<(e: KeyboardEvent) => void> = [];

  beforeEach(() => {
    listeners = [];
    vi.spyOn(window, 'addEventListener').mockImplementation(
      (type: string, listener: EventListenerOrEventListenerObject) => {
        if (type === 'keydown') {
          listeners.push(listener as (e: KeyboardEvent) => void);
        }
      }
    );
    vi.spyOn(window, 'removeEventListener').mockImplementation(() => {});
    registry = new ShortcutRegistry();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function fire(key: string, opts: { ctrlKey?: boolean; metaKey?: boolean } = {}) {
    const event = new KeyboardEvent('keydown', {
      key,
      ctrlKey: opts.ctrlKey ?? false,
      metaKey: opts.metaKey ?? false,
      bubbles: true,
    });
    listeners.forEach((l) => l(event));
  }

  it('registers a handler that fires on Ctrl+key', () => {
    const handler = vi.fn();
    registry.register('ctrl+1', handler);
    fire('1', { ctrlKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('registers a handler that fires on Meta+key (macOS Cmd)', () => {
    const handler = vi.fn();
    registry.register('ctrl+1', handler);
    fire('1', { metaKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does not fire when modifier is absent', () => {
    const handler = vi.fn();
    registry.register('ctrl+n', handler);
    fire('n');
    expect(handler).not.toHaveBeenCalled();
  });

  it('unregister stops the handler from firing', () => {
    const handler = vi.fn();
    const unregister = registry.register('ctrl+,', handler);
    unregister();
    fire(',', { ctrlKey: true });
    expect(handler).not.toHaveBeenCalled();
  });

  it('multiple shortcuts coexist independently', () => {
    const h1 = vi.fn();
    const h2 = vi.fn();
    registry.register('ctrl+1', h1);
    registry.register('ctrl+2', h2);
    fire('1', { ctrlKey: true });
    expect(h1).toHaveBeenCalledTimes(1);
    expect(h2).not.toHaveBeenCalled();
    fire('2', { ctrlKey: true });
    expect(h2).toHaveBeenCalledTimes(1);
  });
});
