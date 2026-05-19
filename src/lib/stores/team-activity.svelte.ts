import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import { addToast } from './toasts.svelte';
import type { FetchResult, TeamActivityRow } from '../types';

type Status =
  | 'idle'
  | 'loading'
  | 'error'
  | 'disabled'
  | 'machine_label_empty'
  | 'no_overlap_repos';

class TeamActivityStore {
  /** Active rows keyed by workspace_id. */
  readonly rows = new SvelteMap<string, TeamActivityRow>();

  /** UI state for the sidebar panel. See Phase 3a-4 spec's error
   *  handling matrix for the mapping from each value to user-visible
   *  copy. */
  status: Status = $state('idle');

  /** Last network / unexpected error message — surfaced as toast or
   *  banner copy. Cleared on the next successful fetch. */
  error: string | null = $state(null);

  /** When non-null, App.svelte routes to `TeamWorkspaceMirror` for this
   *  row. Cleared via `select(null)` from the back button OR
   *  automatically when reconcile detects the row vanished server-side. */
  selectedWorkspaceId: string | null = $state(null);

  /** Manual refresh trigger (Refresh button + visibility-change immediate
   *  fetch + first poll on mount). Also the unit-test entry point — the
   *  full `start()` / `stop()` lifecycle is tested separately in Task 8. */
  async refresh(): Promise<void> {
    let result: FetchResult;
    try {
      result = await api.teamActivity.fetchRows();
    } catch (err) {
      this.error = String(err);
      this.status = 'error';
      return;
    }
    this.error = null;
    switch (result.kind) {
      case 'disabled':
        this.status = 'disabled';
        this.reconcile([]);
        return;
      case 'machine_label_empty':
        this.status = 'machine_label_empty';
        this.reconcile([]);
        return;
      case 'no_overlap_repos':
        this.status = 'no_overlap_repos';
        this.reconcile([]);
        return;
      case 'rows':
        this.status = 'idle';
        this.reconcile(result.rows);
        return;
    }
  }

  /** Open or close the mirror view. Called by `TeamActivityPanel` row
   *  click and `TitleBar` back button. */
  select(workspaceId: string | null): void {
    this.selectedWorkspaceId = workspaceId;
  }

  private reconcile(rows: TeamActivityRow[]): void {
    // SvelteSet (not plain Set) to satisfy the codebase's
    // svelte/prefer-svelte-reactivity ESLint rule in .svelte.ts files.
    const newIds = new SvelteSet<string>(rows.map((r) => r.workspace_id));
    // Insert / update rows present in the latest response.
    for (const r of rows) this.rows.set(r.workspace_id, r);
    // Remove rows that disappeared.
    for (const id of [...this.rows.keys()]) {
      if (!newIds.has(id)) this.rows.delete(id);
    }
    // Auto-close mirror view if its row vanished (teammate went private
    // or workspace closed).
    if (this.selectedWorkspaceId && !newIds.has(this.selectedWorkspaceId)) {
      this.selectedWorkspaceId = null;
      addToast('Team workspace closed by teammate', 'info', 4000);
    }
  }
}

export const teamActivity = new TeamActivityStore();
