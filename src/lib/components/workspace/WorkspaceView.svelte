<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, agentChannel } from '$lib/ipc';
  import { messages } from '$lib/stores/messages.svelte';
  import ChatPanel from '$lib/components/chat/ChatPanel.svelte';
  import type { AgentEvent, WorkspaceInfo } from '$lib/types';

  interface Props {
    workspace: WorkspaceInfo;
  }

  const { workspace }: Props = $props();

  const status = $derived(messages.statusFor(workspace.id) ?? workspace.status);

  let channel: ReturnType<typeof agentChannel> | undefined;

  onMount(async () => {
    if (workspace.status === 'not_started' || workspace.status === 'waiting') {
      channel = agentChannel();
      channel.onmessage = (ev: AgentEvent) => {
        messages.apply(ev, workspace.id);
      };
      try {
        await api.agent.spawn(workspace.id, channel);
      } catch (err) {
        messages.apply({ type: 'error', message: String(err) }, workspace.id);
      }
    }
  });

  onDestroy(() => {
    // Leave the agent running on workspace switch — Phase 1d may revisit.
    // The Channel's onmessage handler is GC'd when the component unmounts.
  });

  async function handleSend(text: string) {
    // Echo the user's own message into the store immediately so the bubble
    // renders without waiting for the backend. The backend's send_message
    // command writes the user message to disk; the agent Channel only
    // streams Claude's responses back, so the user message must be added
    // here on the frontend.
    messages.apply(
      {
        type: 'message',
        id: `msg_user_${Date.now()}`,
        role: 'user',
        text,
        is_partial: false,
      },
      workspace.id
    );
    try {
      await api.agent.send(workspace.id, text);
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    }
  }

  function statusLabel(s: string): string {
    if (s === 'running') return 'Running';
    if (s === 'waiting') return 'Waiting';
    if (s === 'error') return 'Error';
    if (s === 'stopped') return 'Stopped';
    return 'Idle';
  }
</script>

<section class="flex flex-col h-full">
  <header
    class="flex items-center justify-between px-4 py-2 border-b border-[var(--border)] bg-[var(--bg-sidebar)]"
  >
    <div class="flex flex-col">
      <h2 class="text-sm font-semibold text-[var(--text-primary)]">
        {workspace.title}
      </h2>
      <code class="text-xs text-[var(--text-muted)]">{workspace.branch}</code>
    </div>
    <span
      class="text-xs px-2 py-0.5 rounded bg-[var(--bg-card)] text-[var(--text-secondary)]"
      data-status={status}
      aria-label="Agent status"
    >
      {statusLabel(status)}
    </span>
  </header>

  <div class="flex-1 overflow-hidden">
    <ChatPanel workspaceId={workspace.id} onSend={handleSend} />
  </div>
</section>
