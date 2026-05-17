import { SvelteMap } from 'svelte/reactivity';

/**
 * Tracks per-repo Lark view-missing banners. Populated by the
 * `lark-view-missing` Tauri event listener; consumed by `TitleBar` to
 * render a non-blocking banner. Session-local — no persistence.
 */
export class ViewMissingStore {
  readonly entries = new SvelteMap<string, string>();

  report(repoId: string, viewId: string): void {
    this.entries.set(repoId, viewId);
  }

  dismiss(repoId: string): void {
    this.entries.delete(repoId);
  }

  clear(): void {
    this.entries.clear();
  }
}

export const viewMissing = new ViewMissingStore();
