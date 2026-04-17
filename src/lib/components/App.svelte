<!-- src/lib/components/App.svelte — stub for Task 10; extended in Task 11 -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { ShortcutRegistry } from '$lib/keyboard';
  import { modeStore } from '$lib/stores/mode.svelte';

  let registry: ShortcutRegistry;

  onMount(() => {
    registry = new ShortcutRegistry();
    registry.register('ctrl+1', () => modeStore.set('plan'));
    registry.register('ctrl+2', () => modeStore.set('work'));
    // ctrl+n, ctrl+, and ctrl+e are no-ops at this phase; registered for
    // future phases to attach handlers without structural changes.
    registry.register('ctrl+n', () => {});
    registry.register('ctrl+,', () => {});
    registry.register('ctrl+e', () => {});
  });

  onDestroy(() => {
    registry?.destroy();
  });
</script>

<div class="app-stub">
  <!-- Full layout wired in Task 11 -->
</div>
