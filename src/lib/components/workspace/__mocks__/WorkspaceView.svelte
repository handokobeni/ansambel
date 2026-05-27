<!-- Stub WorkspaceView used in tests: renders a sentinel div so
     mount-persistence tests can assert the component stays in the DOM.
     Accepts the same props as the real WorkspaceView to avoid type errors. -->
<script lang="ts">
  import type { Workspace } from '$lib/types';

  const { workspace }: { workspace: Workspace; highlightedFile?: string | null } = $props();
</script>

<div data-testid="workspace-view-stub">{workspace.title}</div>
