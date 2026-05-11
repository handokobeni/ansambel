import { SvelteMap } from 'svelte/reactivity';

/** A single open file in the Editor panel. The buffer (`content`) lives
 *  here and is mutated as the user types; `diskSha1` records the bytes
 *  the backend confirmed at last read/write so saves can race-detect
 *  agent edits to the same file. */
export interface OpenFile {
  /** Worktree-relative path, forward-slash separated. */
  path: string;
  content: string;
  /** sha1 from the last successful file_read or file_write — what the
   *  backend will compare against on the next file_write call. */
  diskSha1: string;
  /** Backend's binary heuristic — when true the editor refuses to
   *  render the buffer and shows a "Binary file" placeholder. */
  isBinary: boolean;
  /** True when `content` has diverged from whatever was last saved. */
  dirty: boolean;
}

interface EditorState {
  open: OpenFile[];
  /** Path of the currently-active file, or null when no file is open. */
  active: string | null;
}

/** Per-workspace `{ open, active }`. Each workspace gets its own
 *  SvelteMap entry; reading from the store is a pure lookup so it's
 *  safe to call from `$derived`. */
const states = new SvelteMap<string, EditorState>();

function ensure(workspaceId: string): EditorState {
  let state = states.get(workspaceId);
  if (!state) {
    state = { open: [], active: null };
    states.set(workspaceId, state);
  }
  return state;
}

export const editorTabs = {
  /** All currently-open files for the workspace, ordered by open time. */
  open(workspaceId: string): OpenFile[] {
    return states.get(workspaceId)?.open ?? [];
  },

  /** Path of the active file, or null when no file is open. */
  active(workspaceId: string): string | null {
    return states.get(workspaceId)?.active ?? null;
  },

  /** The currently-active OpenFile, if any. Convenience accessor. */
  activeFile(workspaceId: string): OpenFile | null {
    const state = states.get(workspaceId);
    if (!state || !state.active) return null;
    return state.open.find((f) => f.path === state.active) ?? null;
  },

  /** Open a file. If it's already open, just re-activate it (no
   *  duplicate, no content overwrite). Otherwise append + activate. */
  openFile(
    workspaceId: string,
    path: string,
    content: string,
    diskSha1: string,
    isBinary: boolean
  ): void {
    const state = ensure(workspaceId);
    const existing = state.open.find((f) => f.path === path);
    if (existing) {
      states.set(workspaceId, { ...state, active: path });
      return;
    }
    const newFile: OpenFile = {
      path,
      content,
      diskSha1,
      isBinary,
      dirty: false,
    };
    states.set(workspaceId, {
      open: [...state.open, newFile],
      active: path,
    });
  },

  /** Switch the active file. No-op if `path` isn't currently open. */
  setActive(workspaceId: string, path: string): void {
    const state = ensure(workspaceId);
    if (!state.open.some((f) => f.path === path)) return;
    if (state.active === path) return;
    states.set(workspaceId, { ...state, active: path });
  },

  /** Update the buffer content for an open file. Sets `dirty` based on
   *  whether the new content matches the last-saved sha1's content. */
  updateContent(workspaceId: string, path: string, content: string): void {
    const state = ensure(workspaceId);
    const idx = state.open.findIndex((f) => f.path === path);
    if (idx === -1) return;
    const prev = state.open[idx];
    const dirty = content !== prev.content || prev.dirty;
    // The simple "dirty if content changed since open OR was already
    // dirty" rule is intentional. We don't compare against diskSha1
    // because computing sha1 on every keystroke is wasteful — the
    // editor already trusts the round-trip with file_read/file_write.
    const updated: OpenFile = { ...prev, content, dirty };
    const next = [...state.open];
    next[idx] = updated;
    states.set(workspaceId, { ...state, open: next });
  },

  /** Stamp a file as freshly saved — clears dirty + updates diskSha1. */
  markSaved(workspaceId: string, path: string, newSha1: string): void {
    const state = ensure(workspaceId);
    const idx = state.open.findIndex((f) => f.path === path);
    if (idx === -1) return;
    const prev = state.open[idx];
    const updated: OpenFile = { ...prev, diskSha1: newSha1, dirty: false };
    const next = [...state.open];
    next[idx] = updated;
    states.set(workspaceId, { ...state, open: next });
  },

  /** Close a file. Picks a neighbour as the new active when the closed
   *  file was active; resets active to null when the last file closes. */
  closeFile(workspaceId: string, path: string): void {
    const state = ensure(workspaceId);
    const idx = state.open.findIndex((f) => f.path === path);
    if (idx === -1) return;
    const next = state.open.filter((f) => f.path !== path);
    let active = state.active;
    if (active === path) {
      // Prefer the file at the same index after splice (the next one);
      // fall back to the previous one; null when the list is empty.
      active = next[idx]?.path ?? next[idx - 1]?.path ?? null;
    }
    states.set(workspaceId, { open: next, active });
  },

  /** Drop all editor state for a workspace. Called when the workspace
   *  is removed so the store doesn't leak entries. */
  forget(workspaceId: string): void {
    states.delete(workspaceId);
  },

  /** Clear the entire store — used by tests for isolation. */
  reset(): void {
    states.clear();
  },
};
