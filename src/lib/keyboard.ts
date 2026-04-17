// src/lib/keyboard.ts

/**
 * Cross-platform keyboard shortcut registry.
 *
 * Combo format: "ctrl+<key>" — matches both Ctrl (Windows/Linux) and
 * Meta/Cmd (macOS) so a single registration covers all platforms.
 *
 * Usage:
 *   const registry = new ShortcutRegistry();
 *   const unregister = registry.register('ctrl+n', () => openNewTask());
 *   // On cleanup:
 *   unregister();
 */

type ShortcutHandler = (event: KeyboardEvent) => void;

interface Shortcut {
  key: string; // lowercase single key character, e.g. 'n', ',', '1'
  ctrl: boolean;
  handler: ShortcutHandler;
}

export class ShortcutRegistry {
  private shortcuts: Map<string, Shortcut> = new Map();
  private listener: (e: KeyboardEvent) => void;

  constructor() {
    this.listener = (e: KeyboardEvent) => this.handleKeydown(e);
    window.addEventListener('keydown', this.listener);
  }

  /**
   * Register a shortcut.
   *
   * @param combo - Format: "ctrl+<key>", e.g. "ctrl+n", "ctrl+1", "ctrl+,"
   * @param handler - Called when the shortcut fires.
   * @returns Unregister function — call on component unmount.
   */
  register(combo: string, handler: ShortcutHandler): () => void {
    const parsed = this.parse(combo);
    if (!parsed) {
      throw new Error(`Invalid shortcut combo: "${combo}"`);
    }
    const id = combo.toLowerCase();
    this.shortcuts.set(id, { ...parsed, handler });
    return () => this.shortcuts.delete(id);
  }

  /** Remove all registered shortcuts and detach the global listener. */
  destroy(): void {
    this.shortcuts.clear();
    window.removeEventListener('keydown', this.listener);
  }

  private parse(combo: string): { key: string; ctrl: boolean } | null {
    const parts = combo.toLowerCase().split('+');
    if (parts.length < 2) return null;
    const modifiers = parts.slice(0, -1);
    const key = parts[parts.length - 1];
    const ctrl = modifiers.includes('ctrl') || modifiers.includes('cmd');
    return { key, ctrl };
  }

  private handleKeydown(e: KeyboardEvent): void {
    const hasModifier = e.ctrlKey || e.metaKey;
    if (!hasModifier) return;
    for (const shortcut of this.shortcuts.values()) {
      if (shortcut.ctrl && e.key.toLowerCase() === shortcut.key) {
        shortcut.handler(e);
      }
    }
  }
}
