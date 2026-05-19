<!-- src/lib/components/lark/TeamActivitySettings.svelte -->
<!--
  Team Activity publisher settings.

  Owns the 3-field form (app_token / table_id / machine_label) plus three
  actions: "Setup table schema" (idempotent column provisioner), Save, and
  Disconnect. The Lark `app_id` / `app_secret` come from the global Lark
  credentials panel above this one — this panel only configures *which*
  Bitable / table the workspace-state publisher writes into.

  Notes
    * Changing the config does NOT restart the running publisher — the new
      values take effect on the next app launch (the Save toast says so).
    * Disconnect = `set` with an empty `app_token`. The backend persister's
      empty-token-as-None rule (see commands/team_activity.rs Task 4) makes
      the next `get` return `null`, which our reload routine treats as the
      empty-form state.
    * `machine_label` defaults to `'me@machine'` on first run. TODO:
      derive from OS via a tiny Rust helper (`$USER@$(hostname)`) — a
      follow-up; out of scope for Task 17.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type { TeamActivityConfig } from '$lib/types';

  const DEFAULT_MACHINE_LABEL = 'me@machine';

  let loading = $state(true);
  let busy = $state(false);
  let configured = $state(false);

  let appToken = $state('');
  let tableId = $state('');
  let machineLabel = $state(DEFAULT_MACHINE_LABEL);

  let confirmDisconnect = $state(false);

  const canSave = $derived(!busy && appToken.trim().length > 0 && tableId.trim().length > 0);
  const canSetup = $derived(!busy && appToken.trim().length > 0 && tableId.trim().length > 0);

  onMount(async () => {
    await loadConfig();
  });

  async function loadConfig() {
    loading = true;
    try {
      const cfg = await api.teamActivity.get();
      applyConfig(cfg);
    } catch (e) {
      addToast(`Failed to load team activity config: ${describe(e)}`, 'error');
    } finally {
      loading = false;
    }
  }

  function applyConfig(cfg: TeamActivityConfig | null) {
    if (cfg) {
      appToken = cfg.app_token;
      tableId = cfg.table_id;
      machineLabel = cfg.machine_label || DEFAULT_MACHINE_LABEL;
      configured = true;
    } else {
      appToken = '';
      tableId = '';
      machineLabel = DEFAULT_MACHINE_LABEL;
      configured = false;
    }
  }

  async function handleSave(e: Event) {
    e.preventDefault();
    if (!canSave) return;
    busy = true;
    try {
      const cfg: TeamActivityConfig = {
        app_token: appToken.trim(),
        table_id: tableId.trim(),
        machine_label: machineLabel.trim() || DEFAULT_MACHINE_LABEL,
      };
      await api.teamActivity.set(cfg);
      configured = true;
      addToast('Team activity configured. Restart app to apply changes.', 'success');
    } catch (err) {
      addToast(`Save failed: ${describe(err)}`, 'error');
    } finally {
      busy = false;
    }
  }

  async function handleSetup() {
    if (!canSetup) return;
    busy = true;
    try {
      const created = await api.teamActivity.setupTable(appToken.trim(), tableId.trim());
      addToast(`Table schema created/verified (${created.length} columns).`, 'success');
    } catch (err) {
      addToast(`Setup failed: ${describe(err)}`, 'error');
    } finally {
      busy = false;
    }
  }

  function handleDisconnectClick() {
    confirmDisconnect = true;
  }

  function handleDisconnectCancel() {
    confirmDisconnect = false;
  }

  async function handleDisconnectConfirm() {
    busy = true;
    try {
      // Empty token tells the backend persister to drop the config (see
      // Task 4's empty-token-as-None rule); subsequent `get` returns null.
      await api.teamActivity.set({ app_token: '', table_id: '', machine_label: '' });
      applyConfig(null);
      addToast('Team activity disconnected.', 'success');
    } catch (err) {
      addToast(`Disconnect failed: ${describe(err)}`, 'error');
    } finally {
      busy = false;
      confirmDisconnect = false;
    }
  }

  function describe(e: unknown): string {
    if (e instanceof Error) return e.message;
    if (typeof e === 'string') return e;
    try {
      return JSON.stringify(e);
    } catch {
      return 'unknown error';
    }
  }

  const fieldClass =
    'px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]';
</script>

<section
  class="team-activity-settings flex flex-col gap-4 p-4 text-[var(--text-primary)]"
  aria-labelledby="team-activity-settings-title"
  data-testid="team-activity-settings"
>
  <header class="flex items-center justify-between">
    <h2 id="team-activity-settings-title" class="text-sm font-semibold">Team Activity</h2>
    <span
      class="text-[11px] px-2 py-0.5 rounded-full border"
      class:border-[var(--accent)]={configured}
      class:text-[var(--accent)]={configured}
      class:border-[var(--border-light)]={!configured}
      class:text-[var(--text-muted)]={!configured}
      data-testid="team-activity-status"
    >
      {configured ? '● Active' : '○ Not configured'}
    </span>
  </header>

  <p class="text-[11px] text-[var(--text-muted)]">
    Publish workspace state to a shared Bitable so teammates can see who's working on what.
  </p>

  {#if loading}
    <p class="text-xs text-[var(--text-muted)]" data-testid="team-activity-loading">Loading...</p>
  {:else}
    <form class="flex flex-col gap-3" onsubmit={handleSave}>
      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="team-activity-app-token"
        >
          App Token
        </label>
        <input
          id="team-activity-app-token"
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={appToken}
          placeholder="bascnxxxxxxxxxxxxxxxxxxx"
          class={fieldClass}
          data-testid="team-activity-app-token"
        />
      </div>

      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="team-activity-table-id"
        >
          Table ID
        </label>
        <input
          id="team-activity-table-id"
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={tableId}
          placeholder="tblxxxxxxxxxxxxx"
          class={fieldClass}
          data-testid="team-activity-table-id"
        />
      </div>

      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="team-activity-machine-label"
        >
          Machine label
          <span class="text-[10px] text-[var(--text-muted)]">
            (shown to teammates — defaults to {DEFAULT_MACHINE_LABEL})
          </span>
        </label>
        <input
          id="team-activity-machine-label"
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={machineLabel}
          placeholder={DEFAULT_MACHINE_LABEL}
          class={fieldClass}
          data-testid="team-activity-machine-label"
        />
      </div>

      <div class="flex justify-between items-center gap-2 mt-1">
        <button
          type="button"
          class="px-3 py-1.5 text-xs font-semibold rounded border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          onclick={handleSetup}
          disabled={!canSetup}
          data-testid="team-activity-setup"
        >
          Setup table schema
        </button>

        <div class="flex gap-2">
          {#if configured}
            {#if !confirmDisconnect}
              <button
                type="button"
                class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={handleDisconnectClick}
                disabled={busy}
                data-testid="team-activity-disconnect"
              >
                Disconnect
              </button>
            {/if}
          {/if}
          <button
            type="submit"
            class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            disabled={!canSave}
            data-testid="team-activity-save"
          >
            {busy ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>

      {#if confirmDisconnect}
        <div
          class="flex items-center justify-between gap-2 px-2 py-1.5 rounded border border-[var(--border-light)] text-[11px] text-[var(--text-muted)]"
          role="alertdialog"
          aria-label="Confirm disconnect"
          data-testid="team-activity-disconnect-prompt"
        >
          <span>Disconnect? Existing config will be deleted.</span>
          <div class="flex gap-2">
            <button
              type="button"
              class="px-2 py-0.5 text-[11px] rounded border border-[var(--border-light)] text-[var(--text-dim)] hover:text-[var(--text-primary)] cursor-pointer disabled:opacity-50"
              onclick={handleDisconnectCancel}
              disabled={busy}
              data-testid="team-activity-disconnect-cancel"
            >
              Cancel
            </button>
            <button
              type="button"
              class="px-2 py-0.5 text-[11px] rounded bg-red-500/80 text-white hover:bg-red-500 cursor-pointer disabled:opacity-50"
              onclick={handleDisconnectConfirm}
              disabled={busy}
              data-testid="team-activity-disconnect-confirm"
            >
              {busy ? 'Disconnecting...' : 'Yes, disconnect'}
            </button>
          </div>
        </div>
      {/if}
    </form>
  {/if}
</section>
