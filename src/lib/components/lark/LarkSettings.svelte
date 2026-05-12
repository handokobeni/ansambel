<!-- src/lib/components/lark/LarkSettings.svelte -->
<!--
  Lark Open Platform credential form.

  Owns the 4-field form (app_id / app_secret / app_token / table_id +
  optional base_url) plus three actions: Save, Test connection, Clear.

  Security notes:
    * `appSecret` is bound to a password-typed input and never echoed
      back from the backend. We pre-fill the other three from the
      current status; the secret field stays blank, and saving an empty
      string is rejected by the backend.
    * Errors surface in the in-component banner — no console.log calls.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';
  import type { LarkStatus } from '$lib/types';

  type Banner =
    | { kind: 'idle' }
    | { kind: 'info'; message: string }
    | { kind: 'success'; message: string }
    | { kind: 'error'; message: string };

  let loading = $state(true);
  let saving = $state(false);
  let testing = $state(false);
  let status = $state<LarkStatus | null>(null);
  let banner = $state<Banner>({ kind: 'idle' });

  let appId = $state('');
  let appSecret = $state('');
  let appToken = $state('');
  let tableId = $state('');
  let baseUrl = $state('');

  const canSave = $derived(
    !saving &&
      appId.trim().length > 0 &&
      appSecret.trim().length > 0 &&
      appToken.trim().length > 0 &&
      tableId.trim().length > 0
  );

  const canTest = $derived(!testing && status?.configured === true);

  onMount(async () => {
    await loadStatus();
  });

  async function loadStatus() {
    loading = true;
    try {
      const next = await api.lark.getStatus();
      applyStatus(next);
    } catch (e) {
      banner = { kind: 'error', message: `Failed to load status: ${describe(e)}` };
    } finally {
      loading = false;
    }
  }

  function applyStatus(next: LarkStatus) {
    status = next;
    appId = next.app_id ?? '';
    appToken = next.app_token ?? '';
    tableId = next.table_id ?? '';
    // Default to the canonical international URL when unset so the
    // input never shows an empty placeholder; the backend treats blank
    // as "use default" anyway.
    baseUrl = next.base_url;
    // Never repopulate the secret — leave it blank for the user to
    // re-enter when changing credentials.
    appSecret = '';
  }

  async function handleSave(e: Event) {
    e.preventDefault();
    if (!canSave) return;
    saving = true;
    banner = { kind: 'info', message: 'Saving credentials...' };
    try {
      const next = await api.lark.setCredentials({
        appId: appId.trim(),
        appSecret: appSecret.trim(),
        appToken: appToken.trim(),
        tableId: tableId.trim(),
        baseUrl: baseUrl.trim() ? baseUrl.trim() : undefined,
      });
      applyStatus(next);
      banner = { kind: 'success', message: 'Credentials saved.' };
    } catch (e) {
      banner = { kind: 'error', message: `Save failed: ${describe(e)}` };
    } finally {
      saving = false;
    }
  }

  async function handleTest() {
    if (!canTest) return;
    testing = true;
    banner = { kind: 'info', message: 'Testing connection...' };
    try {
      await api.lark.testConnection();
      banner = { kind: 'success', message: 'Connection OK.' };
    } catch (e) {
      banner = { kind: 'error', message: `Connection failed: ${describe(e)}` };
    } finally {
      testing = false;
    }
  }

  async function handleClear() {
    saving = true;
    banner = { kind: 'info', message: 'Clearing credentials...' };
    try {
      await api.lark.clear();
      await loadStatus();
      banner = { kind: 'success', message: 'Credentials cleared.' };
    } catch (e) {
      banner = { kind: 'error', message: `Clear failed: ${describe(e)}` };
    } finally {
      saving = false;
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
</script>

<section
  class="lark-settings flex flex-col gap-4 p-4 text-[var(--text-primary)]"
  aria-labelledby="lark-settings-title"
  data-testid="lark-settings"
>
  <header class="flex items-center justify-between">
    <h2 id="lark-settings-title" class="text-sm font-semibold">Lark Open Platform</h2>
    {#if status}
      <span
        class="text-[11px] px-2 py-0.5 rounded-full border"
        class:border-[var(--accent)]={status.configured}
        class:text-[var(--accent)]={status.configured}
        class:border-[var(--border-light)]={!status.configured}
        class:text-[var(--text-muted)]={!status.configured}
        data-testid="lark-status-pill"
      >
        {status.configured ? 'Configured' : 'Not configured'}
      </span>
    {/if}
  </header>

  {#if loading}
    <p class="text-xs text-[var(--text-muted)]" data-testid="lark-loading">Loading...</p>
  {:else}
    <form class="flex flex-col gap-3" onsubmit={handleSave}>
      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="lark-app-id"
        >
          App ID
        </label>
        <input
          id="lark-app-id"
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={appId}
          placeholder="cli_xxxxxxxxxxxxxxxx"
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        />
      </div>

      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="lark-app-secret"
        >
          App secret
          {#if status?.has_secret}
            <span class="text-[10px] text-[var(--text-muted)]" data-testid="lark-secret-stored">
              (stored — type to replace)
            </span>
          {/if}
        </label>
        <input
          id="lark-app-secret"
          type="password"
          autocomplete="off"
          spellcheck="false"
          bind:value={appSecret}
          placeholder={status?.has_secret ? '••••••••••' : 'app secret'}
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        />
      </div>

      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="lark-app-token"
        >
          App token (Bitable)
        </label>
        <input
          id="lark-app-token"
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={appToken}
          placeholder="bascnxxxxxxxxxxxxxx"
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        />
      </div>

      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="lark-table-id"
        >
          Table ID
        </label>
        <input
          id="lark-table-id"
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={tableId}
          placeholder="tblxxxxxxxxxxxxxx"
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        />
      </div>

      <div class="flex flex-col gap-1">
        <label
          class="text-[11px] uppercase tracking-wider text-[var(--text-muted)]"
          for="lark-base-url"
        >
          Base URL
          <span class="text-[10px] text-[var(--text-muted)]">
            (defaults to open.larksuite.com; use open.feishu.cn for China)
          </span>
        </label>
        <input
          id="lark-base-url"
          type="url"
          autocomplete="off"
          spellcheck="false"
          bind:value={baseUrl}
          placeholder="https://open.larksuite.com"
          class="px-2 py-1.5 text-xs rounded bg-[var(--bg-base)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        />
      </div>

      {#if banner.kind !== 'idle'}
        <div
          role="status"
          aria-live="polite"
          class="text-[11px] px-2 py-1.5 rounded border"
          class:border-[var(--accent)]={banner.kind === 'success'}
          class:text-[var(--accent)]={banner.kind === 'success'}
          class:border-[var(--border-light)]={banner.kind !== 'success' && banner.kind !== 'error'}
          class:text-[var(--text-muted)]={banner.kind === 'info'}
          class:border-red-500={banner.kind === 'error'}
          class:text-red-400={banner.kind === 'error'}
          data-testid="lark-banner"
        >
          {banner.message}
        </div>
      {/if}

      <div class="flex justify-end gap-2 mt-1">
        <button
          type="button"
          class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          onclick={handleClear}
          disabled={saving}
          data-testid="lark-clear"
        >
          Clear
        </button>
        <button
          type="button"
          class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          onclick={handleTest}
          disabled={!canTest}
          data-testid="lark-test"
        >
          {testing ? 'Testing...' : 'Test connection'}
        </button>
        <button
          type="submit"
          class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
          disabled={!canSave}
          data-testid="lark-save"
        >
          {saving ? 'Saving...' : 'Save'}
        </button>
      </div>
    </form>
  {/if}
</section>
