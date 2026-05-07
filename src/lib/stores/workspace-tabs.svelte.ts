import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import type { WorkspaceTabId } from '$lib/types';

/** Per-workspace tab state. Persisted only in memory — recomputing it on
 *  app restart is cheap (default to 'chat', no expanded directories) so
 *  there's no localStorage round trip.
 *
 *  Two backing stores by design:
 *
 *   - `activeTabs` (SvelteMap) holds the active tab. Reads + writes from
 *     here are tracked, so the tab strip re-renders when `setActive`
 *     fires.
 *
 *   - `expandedSets` (plain Map of SvelteSet) holds expanded directory
 *     paths. The outer Map is intentionally NOT reactive — wrapping it in
 *     a SvelteMap would cause `expanded()` to mutate the outer map on
 *     first read, which throws `effect_update_depth_exceeded` when called
 *     inside a `$derived` (the very thing FileBrowser does on every
 *     render). The inner SvelteSet handles its own reactivity for `has`
 *     / `add` / `delete`, which is all the UI actually depends on. */
const activeTabs = new SvelteMap<string, WorkspaceTabId>();
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const expandedSets = new Map<string, SvelteSet<string>>();

export const workspaceTabs = {
  /** Returns the active tab for the workspace. Defaults to 'chat' for any
   *  workspace the store hasn't seen before. Pure read — does not mutate. */
  active(workspaceId: string): WorkspaceTabId {
    return activeTabs.get(workspaceId) ?? 'chat';
  },

  /** Set the active tab. The mutation triggers a reactive update via
   *  the SvelteMap. */
  setActive(workspaceId: string, tab: WorkspaceTabId): void {
    if (activeTabs.get(workspaceId) === tab) return;
    activeTabs.set(workspaceId, tab);
  },

  /** The reactive set of expanded directory paths for the workspace.
   *  First call creates the SvelteSet — but the *outer* Map is plain, so
   *  this read doesn't pollute any tracked dependency graph. The returned
   *  SvelteSet is the live, mutable, reactive container. */
  expanded(workspaceId: string): SvelteSet<string> {
    let set = expandedSets.get(workspaceId);
    if (!set) {
      set = new SvelteSet<string>();
      expandedSets.set(workspaceId, set);
    }
    return set;
  },

  /** Toggle the expanded state for a directory path. Returns the new
   *  state ('expanded' | 'collapsed') so callers can branch on it. */
  toggleExpanded(workspaceId: string, path: string): 'expanded' | 'collapsed' {
    const set = workspaceTabs.expanded(workspaceId);
    if (set.has(path)) {
      set.delete(path);
      return 'collapsed';
    }
    set.add(path);
    return 'expanded';
  },

  /** Drop all tab state for a workspace. Called when the workspace is
   *  removed so the store doesn't leak entries for deleted workspaces. */
  forget(workspaceId: string): void {
    activeTabs.delete(workspaceId);
    expandedSets.delete(workspaceId);
  },

  /** Clear the entire store. Used by tests to keep cases isolated. */
  reset(): void {
    activeTabs.clear();
    expandedSets.clear();
  },
};
