<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';

  let version = $state<string>('loading…');

  onMount(async () => {
    try {
      version = await api.system.getAppVersion();
    } catch (e) {
      version = `error: ${e}`;
    }
  });
</script>

<main class="min-h-screen bg-[var(--bg-base)] text-[var(--text-primary)] p-4">
  <h1 class="text-xl">Ansambel {version}</h1>
</main>
