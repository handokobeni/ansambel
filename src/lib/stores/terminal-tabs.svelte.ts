import { SvelteMap } from 'svelte/reactivity';

/** One terminal session tab. `id` is the stable backend terminal_id; the
 *  PTY for it lives in AppState until killed. */
export interface TerminalTab {
  id: string;
  label: string;
}

interface WorkspaceTerminals {
  tabs: TerminalTab[];
  active: string | null;
  /** Monotonic counter for "Terminal N" labels; never reused. */
  counter: number;
}

const MAX_TERMINALS = 6;

const states = new SvelteMap<string, WorkspaceTerminals>();

function ensure(workspaceId: string): WorkspaceTerminals {
  let s = states.get(workspaceId);
  if (!s) {
    s = { tabs: [], active: null, counter: 0 };
    states.set(workspaceId, s);
  }
  return s;
}

function newId(): string {
  return `term_${Math.random().toString(36).slice(2, 10)}${Date.now().toString(36)}`;
}

export const terminalTabs = {
  list(workspaceId: string): TerminalTab[] {
    return states.get(workspaceId)?.tabs ?? [];
  },

  activeId(workspaceId: string): string | null {
    return states.get(workspaceId)?.active ?? null;
  },

  /** Add a terminal tab. Returns its id, or `null` when at the cap. */
  add(workspaceId: string): string | null {
    const s = ensure(workspaceId);
    if (s.tabs.length >= MAX_TERMINALS) return null;
    const counter = s.counter + 1;
    const tab: TerminalTab = { id: newId(), label: `Terminal ${counter}` };
    states.set(workspaceId, { tabs: [...s.tabs, tab], active: tab.id, counter });
    return tab.id;
  },

  setActive(workspaceId: string, id: string): void {
    const s = ensure(workspaceId);
    if (!s.tabs.some((t) => t.id === id) || s.active === id) return;
    states.set(workspaceId, { ...s, active: id });
  },

  /** Close a tab. Activates a neighbour; active becomes null when the
   *  last tab closes. Caller is responsible for killing the PTY. */
  close(workspaceId: string, id: string): void {
    const s = ensure(workspaceId);
    const idx = s.tabs.findIndex((t) => t.id === id);
    if (idx === -1) return;
    const tabs = s.tabs.filter((t) => t.id !== id);
    let active = s.active;
    if (active === id) {
      active = tabs[idx]?.id ?? tabs[idx - 1]?.id ?? null;
    }
    states.set(workspaceId, { ...s, tabs, active });
  },

  /** Drop all terminal-tab state for a workspace (on workspace remove). */
  forget(workspaceId: string): void {
    states.delete(workspaceId);
  },

  reset(): void {
    states.clear();
  },
};
