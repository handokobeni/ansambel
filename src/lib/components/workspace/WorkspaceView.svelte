<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, agentChannel } from '$lib/ipc';
  import { messages } from '$lib/stores/messages.svelte';
  import { workspaceTabs } from '$lib/stores/workspace-tabs.svelte';
  import { editorTabs } from '$lib/stores/editor-tabs.svelte';
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import ChatPanel from '$lib/components/chat/ChatPanel.svelte';
  import TabStrip from './TabStrip.svelte';
  import DiffView from './DiffView.svelte';
  import FileBrowser from './FileBrowser.svelte';
  import Editor from './Editor.svelte';
  import EditorTabBar from './EditorTabBar.svelte';
  import Terminal from './Terminal.svelte';
  import ScriptPicker from './ScriptPicker.svelte';
  import type { AgentEvent, Attachment, AttachmentDraft, WorkspaceInfo } from '$lib/types';

  interface Props {
    workspace: WorkspaceInfo;
    /** Path (relative to the worktree) the FileBrowser should highlight.
     *  Stamped from the App-level SearchModal jump callback. */
    highlightedFile?: string | null;
  }

  const { workspace, highlightedFile = null }: Props = $props();

  const status = $derived(messages.statusFor(workspace.id) ?? workspace.status);
  const activeTab = $derived(workspaceTabs.active(workspace.id));

  let channel: ReturnType<typeof agentChannel> | undefined;

  onMount(async () => {
    // Hydrate persisted history first so previous turns appear immediately
    // on workspace open. Failures here are non-fatal — we still want to
    // spawn or reattach so the user can start a fresh turn.
    try {
      const history = await api.messages.list(workspace.id);
      messages.hydrate(workspace.id, history);
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    }

    channel = agentChannel();
    channel.onmessage = (ev: AgentEvent) => {
      messages.apply(ev, workspace.id);
    };
    // Read from the messages store first — it persists across tab-switch
    // remounts within the same app session, while `workspace.status` (the
    // prop) reflects the value at first open and can be stale.
    //
    // Note: the spawn_agent Tauri command is idempotent on the backend
    // (it silently downgrades to reattach when the agent is already alive
    // for this workspace), so taking the spawn path on a stale read here
    // is no longer fatal — the backend won't error with
    // "agent already running". The messages-store check is still the
    // preferred branch because reattach skips the (small) overhead of
    // re-checking process state on the backend.
    const currentStatus = messages.statusFor(workspace.id) ?? workspace.status;
    try {
      if (currentStatus === 'not_started' || currentStatus === 'waiting') {
        await api.agent.spawn(workspace.id, channel);
      } else {
        // Agent is alive on the backend but our Channel handler was GC'd
        // on the previous unmount. Re-subscribe to the broadcaster so live
        // events resume.
        await api.agent.reattach(workspace.id, channel);
      }
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    }
  });

  onDestroy(() => {
    // Leave the agent running on workspace switch — Phase 1d may revisit.
    // The Channel's onmessage handler is GC'd when the component unmounts.
  });

  async function loadEarlier(beforeId: string) {
    return await api.messages.list(workspace.id, { beforeId });
  }

  let stopping = $state(false);
  async function handleStop() {
    if (stopping) return;
    stopping = true;
    try {
      await api.agent.stop(workspace.id);
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    } finally {
      stopping = false;
    }
  }

  async function handleSend(text: string, drafts: AttachmentDraft[] = []) {
    // Echo the user's own message into the store immediately so the bubble
    // renders without waiting for the backend. The backend's send_message
    // command writes the user message to disk; the agent Channel only
    // streams Claude's responses back, so the user message must be added
    // here on the frontend.
    //
    // For attachments we synthesize a preview Attachment from the draft
    // (path = sourcePath until the backend copies the file). The bubble
    // rendering uses convertFileSrc on `path`, which works with both the
    // user's source path and the eventual data-dir path.
    const previewAttachments: Attachment[] = drafts.map((d) => ({
      kind: 'image',
      media_type: d.mediaType,
      path: d.sourcePath,
      filename: d.filename,
    }));
    const echoId = `msg_user_${Date.now()}`;
    messages.apply(
      {
        type: 'message',
        id: echoId,
        role: 'user',
        text,
        is_partial: false,
      },
      workspace.id
    );
    if (previewAttachments.length > 0) {
      // The 'message' event handler in the store ignores attachments because
      // the backend's AgentEvent::Message has no such field. Stamp them on
      // the echoed Message directly so the user's bubble shows their
      // upload immediately.
      messages.attachToMessage(workspace.id, echoId, previewAttachments);
    }
    try {
      // After Stop the agent process is dead, but the user can still type
      // their next prompt. Re-spawn before sending so the conversation
      // continues seamlessly — same UX as Claude/ChatGPT web.
      const current = messages.statusFor(workspace.id) ?? workspace.status;
      if (current === 'stopped' || current === 'not_started') {
        // Detach the previous channel and use a fresh one. The old reader
        // thread on the backend can still emit a trailing Status::Stopped
        // through its broadcaster after respawn; routing it to a dead
        // channel keeps the new agent's status from flapping back to
        // "stopped" the moment we set it to "running".
        if (channel) {
          channel.onmessage = () => {};
        }
        channel = agentChannel();
        channel.onmessage = (ev: AgentEvent) => {
          messages.apply(ev, workspace.id);
        };
        await api.agent.spawn(workspace.id, channel);
      }
      await api.agent.send(workspace.id, text, drafts);
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    }
  }

  async function handleFileOpen(path: string): Promise<void> {
    try {
      const r = await api.file.read(workspace.id, path);
      editorTabs.openFile(workspace.id, path, r.content, r.sha1, r.is_binary);
      workspaceTabs.setActive(workspace.id, 'editor');
    } catch (err) {
      addToast(`Open failed: ${String(err)}`, 'error');
    }
  }

  function statusLabel(s: string): string {
    if (s === 'running') return 'Running';
    if (s === 'waiting') return 'Waiting';
    if (s === 'error') return 'Error';
    if (s === 'stopped') return 'Stopped';
    return 'Idle';
  }

  // ── Phase 3a-3 Task 18: team-activity privacy toggle ───────────────
  //
  // Source-of-truth lives on the workspaces store (which the optimistic
  // setter mutates in place). We fall back to the prop value when the
  // store hasn't loaded this workspace yet (e.g. component mounted from
  // a deep link before the sidebar populated).
  const isPrivate = $derived(
    workspaces.byRepo.get(workspace.repo_id)?.get(workspace.id)?.team_activity_private ??
      workspace.team_activity_private ??
      false
  );
  let togglingPrivacy = $state(false);
  async function handleTogglePrivacy() {
    if (togglingPrivacy) return;
    togglingPrivacy = true;
    try {
      await workspaces.setTeamActivityPrivate(workspace.id, workspace.repo_id, !isPrivate);
    } finally {
      togglingPrivacy = false;
    }
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
    <div class="flex items-center gap-2">
      <button
        type="button"
        onclick={handleTogglePrivacy}
        disabled={togglingPrivacy}
        data-testid="team-activity-privacy-toggle"
        data-private={isPrivate ? 'true' : 'false'}
        aria-pressed={isPrivate}
        aria-label={isPrivate
          ? 'Team Activity is private — click to make public'
          : 'Team Activity is public — click to make private'}
        title={isPrivate
          ? 'Team Activity is private (publisher suppresses sensitive columns). Click to make public.'
          : 'Team Activity is public. Click to make private and clear sensitive columns.'}
        class="text-xs px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text-primary)] hover:bg-[var(--bg-card)] disabled:opacity-50"
      >
        {isPrivate ? 'Team Activity: ◌ Private' : 'Team Activity: ● Public'}
      </button>
      {#if status === 'running'}
        <button
          type="button"
          onclick={handleStop}
          disabled={stopping}
          class="text-xs px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text-primary)] hover:bg-[var(--bg-card)] disabled:opacity-50"
        >
          Stop
        </button>
      {/if}
      <span
        class="text-xs px-2 py-0.5 rounded bg-[var(--bg-card)] text-[var(--text-secondary)]"
        data-status={status}
        aria-label="Agent status"
      >
        {statusLabel(status)}
      </span>
    </div>
  </header>

  <TabStrip active={activeTab} onSelect={(t) => workspaceTabs.setActive(workspace.id, t)} />

  <!-- Hidden-mount: every tab stays in the DOM and just toggles `display`
       so ChatPanel keeps its scroll position + xterm-style hard-rule
       components don't have to remount. DiffView and FileBrowser are
       cheap enough to remount, but the same shape future-proofs 2b's
       editor + terminal panels. -->
  <div class="flex-1 overflow-hidden relative">
    <div
      id="tabpanel-chat"
      class:hidden={activeTab !== 'chat'}
      role="tabpanel"
      aria-labelledby="tab-chat"
      class="absolute inset-0"
    >
      <ChatPanel workspaceId={workspace.id} onSend={handleSend} onLoadEarlier={loadEarlier} />
    </div>
    <div
      id="tabpanel-diff"
      class:hidden={activeTab !== 'diff'}
      role="tabpanel"
      aria-labelledby="tab-diff"
      class="absolute inset-0"
    >
      <DiffView workspaceId={workspace.id} />
    </div>
    <div
      id="tabpanel-files"
      class:hidden={activeTab !== 'files'}
      role="tabpanel"
      aria-labelledby="tab-files"
      class="absolute inset-0"
    >
      <FileBrowser
        workspaceId={workspace.id}
        selectedPath={highlightedFile}
        onOpen={handleFileOpen}
      />
    </div>
    <div
      id="tabpanel-editor"
      class:hidden={activeTab !== 'editor'}
      role="tabpanel"
      aria-labelledby="tab-editor"
      class="absolute inset-0 flex flex-col"
    >
      <EditorTabBar workspaceId={workspace.id} />
      <div class="flex-1 overflow-hidden">
        <Editor workspaceId={workspace.id} />
      </div>
    </div>
    <div
      id="tabpanel-terminal"
      class:hidden={activeTab !== 'terminal'}
      role="tabpanel"
      aria-labelledby="tab-terminal"
      class="absolute inset-0 flex flex-col"
    >
      <ScriptPicker repoId={workspace.repo_id} workspaceId={workspace.id} />
      <div class="flex-1 overflow-hidden">
        <Terminal workspaceId={workspace.id} />
      </div>
    </div>
  </div>
</section>
