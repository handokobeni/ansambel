<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { SvelteMap } from 'svelte/reactivity';
  import { api } from '$lib/ipc';
  import { parseMention, rankFiles } from '$lib/mentions';
  import MentionAutocomplete from './MentionAutocomplete.svelte';
  import type { AttachmentDraft } from '$lib/types';

  interface Props {
    onSend: (text: string, attachments: AttachmentDraft[]) => void;
    disabled?: boolean;
    /** Workspace whose worktree backs the @-mention autocomplete. When
     *  absent, the mention dropdown stays disabled. */
    workspaceId?: string;
  }

  const { onSend, disabled = false, workspaceId }: Props = $props();

  let value = $state('');
  let attachments = $state<AttachmentDraft[]>([]);
  let textareaEl: HTMLTextAreaElement | undefined;

  // Cap auto-grow at ~12 lines so a runaway paste doesn't swallow the
  // chat history. Beyond this the textarea scrolls internally.
  const MAX_TEXTAREA_HEIGHT_PX = 240;

  // Allowed image extensions — must match what Anthropic's image API accepts
  // (png, jpg, webp, gif). Drives both the OS file picker filter and the
  // mime inference below.
  const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'webp', 'gif'];

  // ── @-mention state ────────────────────────────────────────────────
  let caretPos = $state(0);
  let cachedFiles: string[] | null = $state(null);
  let loadingFiles = $state(false);
  let highlighted = $state(0);
  // Index of the `@` the user has dismissed via Esc. While the
  // current mention is anchored at the same `@`, suppress the
  // dropdown. Cleared automatically when the user types a new
  // mention at a different position.
  let dismissedAt: number | null = $state(null);
  // Component-instance cache so re-renders within the same workspace
  // don't re-fetch. Reactive Map (SvelteMap) — non-reactive Map is
  // flagged by lint rule `svelte/prefer-svelte-reactivity`.
  const fileCache = new SvelteMap<string, string[]>();
  // Current detected mention, or null. Driven by `value` + `caretPos`.
  const mention = $derived(parseMention(value, caretPos));
  // Ranked, top-N candidates. Recomputed cheaply on every keystroke.
  const rankedFiles = $derived(mention && cachedFiles ? rankFiles(cachedFiles, mention.query) : []);
  const mentionOpen = $derived(
    mention !== null && workspaceId !== undefined && dismissedAt !== mention.start
  );

  async function ensureFilesLoaded(): Promise<void> {
    if (!workspaceId) return;
    const cached = fileCache.get(workspaceId);
    if (cached !== undefined) {
      cachedFiles = cached;
      return;
    }
    if (loadingFiles) return;
    loadingFiles = true;
    try {
      const files = await api.workspace.filesRecursive(workspaceId);
      fileCache.set(workspaceId, files);
      cachedFiles = files;
    } catch {
      // Best-effort: empty list lets the empty-state render, the user
      // can still type a path manually.
      cachedFiles = [];
    } finally {
      loadingFiles = false;
    }
  }

  // Clear the dismissal flag when the textarea value changes. Esc
  // dismisses for the duration of the current input — if the user
  // keeps typing, they've changed their mind and the dropdown should
  // re-open naturally on the next detected mention.
  $effect(() => {
    void value;
    dismissedAt = null;
  });

  // Trigger fetch when a mention opens. Cheap — early-returns on cache hit.
  $effect(() => {
    if (mentionOpen) void ensureFilesLoaded();
  });

  // Reset cached files when workspace changes.
  $effect(() => {
    void workspaceId;
    cachedFiles = workspaceId ? (fileCache.get(workspaceId) ?? null) : null;
  });

  // Keep highlighted index in range when the ranked list shrinks below it.
  $effect(() => {
    if (highlighted >= rankedFiles.length) highlighted = 0;
  });

  function syncCaret(): void {
    if (textareaEl) caretPos = textareaEl.selectionStart;
  }

  function selectMention(path: string): void {
    if (!mention || !textareaEl) return;
    const before = value.slice(0, mention.start);
    const after = value.slice(caretPos);
    const replacement = `@${path} `;
    value = before + replacement + after;
    const newCaret = before.length + replacement.length;
    // Defer caret placement until after Svelte writes the new value to
    // the DOM. Without `queueMicrotask`, the textarea's selectionStart
    // gets stomped by the input event.
    queueMicrotask(() => {
      if (textareaEl) {
        textareaEl.selectionStart = newCaret;
        textareaEl.selectionEnd = newCaret;
        caretPos = newCaret;
        textareaEl.focus();
      }
    });
  }

  function dismissMention(): void {
    // Pin the dismissal to the current `@` position. The dropdown
    // stays closed as long as the mention is anchored at this index.
    // A new `@` elsewhere (different position) reopens automatically.
    if (mention) dismissedAt = mention.start;
  }

  function autoResize() {
    if (!textareaEl) return;
    // Reset to auto so shrinking works (scrollHeight only grows otherwise).
    textareaEl.style.height = 'auto';
    const next = Math.min(textareaEl.scrollHeight, MAX_TEXTAREA_HEIGHT_PX);
    textareaEl.style.height = `${next}px`;
    textareaEl.style.overflowY =
      textareaEl.scrollHeight > MAX_TEXTAREA_HEIGHT_PX ? 'auto' : 'hidden';
  }

  // Resize whenever the bound value changes — covers typing, paste, and
  // programmatic resets (after send).
  $effect(() => {
    void value;
    autoResize();
  });

  function basename(path: string): string {
    const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return i >= 0 ? path.slice(i + 1) : path;
  }

  function inferMediaType(path: string): string | null {
    const ext = path.split('.').pop()?.toLowerCase() ?? '';
    if (ext === 'png') return 'image/png';
    if (ext === 'jpg' || ext === 'jpeg') return 'image/jpeg';
    if (ext === 'webp') return 'image/webp';
    if (ext === 'gif') return 'image/gif';
    return null;
  }

  async function pickAttachment() {
    if (disabled) return;
    const picked = await open({
      multiple: true,
      filters: [{ name: 'Image', extensions: IMAGE_EXTS }],
    });
    // Tauri's dialog returns string | string[] | null depending on `multiple`.
    const paths: string[] = Array.isArray(picked) ? picked : picked ? [picked] : [];
    for (const p of paths) {
      const mediaType = inferMediaType(p);
      if (!mediaType) continue;
      attachments = [...attachments, { sourcePath: p, mediaType, filename: basename(p) }];
    }
  }

  function removeAttachment(idx: number) {
    attachments = attachments.filter((_, i) => i !== idx);
  }

  function handleSend() {
    const trimmed = value.trim();
    // Allow attachment-only sends (the user may just want Claude to look at
    // an image without typing anything). Reject only when both are empty.
    if (!trimmed && attachments.length === 0) return;
    onSend(trimmed, attachments);
    value = '';
    attachments = [];
  }

  function handleKeydown(e: KeyboardEvent) {
    // Mention navigation takes precedence — only when a mention is
    // active AND we have results to navigate. Empty-results still
    // intercepts Escape so the user has a way to bail out.
    if (mentionOpen) {
      if (e.key === 'Escape') {
        e.preventDefault();
        dismissMention();
        return;
      }
      if (rankedFiles.length > 0) {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          highlighted = (highlighted + 1) % rankedFiles.length;
          return;
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault();
          highlighted = (highlighted - 1 + rankedFiles.length) % rankedFiles.length;
          return;
        }
        if (e.key === 'Enter' || e.key === 'Tab') {
          e.preventDefault();
          selectMention(rankedFiles[highlighted]);
          return;
        }
      }
    }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<form
  class="flex flex-col gap-2 p-3 border-t border-[var(--border)] bg-[var(--bg-base)]"
  onsubmit={(e) => {
    e.preventDefault();
    handleSend();
  }}
>
  {#if attachments.length > 0}
    <div class="flex flex-wrap gap-2" data-testid="attachment-chips">
      {#each attachments as att, i (att.sourcePath + i)}
        <div
          class="flex items-center gap-2 px-2 py-1 rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-xs"
          data-testid="attachment-chip"
        >
          <!-- filename is always set when the chip is created (basename
               of the picked path); nullable on the wire shape but never
               null in this component. The non-null assertion keeps the
               template free of an unreachable fallback branch. -->
          <img
            src={convertFileSrc(att.sourcePath)}
            alt={att.filename!}
            class="w-8 h-8 object-cover rounded"
          />
          <span class="text-[var(--text-secondary)] max-w-[160px] truncate">
            {att.filename!}
          </span>
          <button
            type="button"
            aria-label="Remove attachment"
            onclick={() => removeAttachment(i)}
            class="text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
          >
            ×
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <label class="sr-only" for="message-input">Message</label>
  <div class="relative">
    <textarea
      bind:this={textareaEl}
      id="message-input"
      rows="1"
      class="w-full px-3 py-2 text-sm rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none min-h-[40px] leading-relaxed"
      placeholder="Ask Claude…"
      bind:value
      onkeydown={handleKeydown}
      oninput={syncCaret}
      onclick={syncCaret}
      onkeyup={syncCaret}
      {disabled}
    ></textarea>
    {#if mentionOpen}
      <MentionAutocomplete
        files={rankedFiles}
        query={mention?.query ?? ''}
        {highlighted}
        loading={loadingFiles && cachedFiles === null}
        onSelect={selectMention}
        onHighlight={(i) => (highlighted = i)}
        onDismiss={dismissMention}
      />
    {/if}
  </div>
  <div class="flex items-center justify-between">
    <button
      type="button"
      onclick={pickAttachment}
      {disabled}
      aria-label="Attach image"
      data-testid="attach-button"
      class="px-2 py-1.5 text-base rounded text-[var(--text-secondary)] hover:bg-[var(--bg-card)] disabled:opacity-50 cursor-pointer"
    >
      📎
    </button>
    <button
      type="submit"
      class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 cursor-pointer"
      disabled={disabled || (!value.trim() && attachments.length === 0)}
      aria-label="Send"
    >
      Send
    </button>
  </div>
</form>
