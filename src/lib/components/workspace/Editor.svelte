<script lang="ts">
  import { onMount, onDestroy, untrack, tick } from 'svelte';
  import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
  import { EditorState, Compartment, type Extension } from '@codemirror/state';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { bracketMatching, indentOnInput } from '@codemirror/language';
  import { oneDark } from '@codemirror/theme-one-dark';
  import { api } from '$lib/ipc';
  import { editorTabs, type OpenFile } from '$lib/stores/editor-tabs.svelte';
  import { addToast } from '$lib/stores/toasts.svelte';
  import { langForPath, type LangId } from '$lib/lang';

  interface Props {
    workspaceId: string;
  }

  const { workspaceId }: Props = $props();

  const activeFile = $derived(editorTabs.activeFile(workspaceId));

  let containerRef: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  /** CodeMirror compartment that swaps language extension on file change. */
  let langCompartment: Compartment | undefined;
  /** Path the editor's current document was loaded for. Tracks
   *  activeFile.path so we know when to dispatch a doc swap. */
  let mountedPath: string | null = null;
  /** Set true while we're applying a programmatic doc swap so the
   *  update listener doesn't loop the change back into the store. */
  let suppressUpdates = false;
  let saving = $state(false);

  onMount(async () => {
    // Imperatively wait until the panel is unhidden — when the
    // surrounding TabStrip flips display, the container has zero
    // dimensions and CodeMirror would render with bad layout. The
    // ResizeObserver fires when display changes from none → block.
    await tick();
    if (!containerRef) return;
    langCompartment = new Compartment();
    view = new EditorView({
      parent: containerRef,
      state: EditorState.create({
        doc: '',
        extensions: baseExtensions(langCompartment, []),
      }),
    });
  });

  $effect(() => {
    // Track activeFile so the effect re-runs when the file switches.
    // Mutate via untrack so the dispatch we send into CodeMirror doesn't
    // re-trigger this effect.
    const file = activeFile;
    if (!view || !langCompartment) return;
    untrack(() => {
      if (!file) {
        if (mountedPath !== null) {
          swapDoc('', null);
          mountedPath = null;
        }
        return;
      }
      if (file.path === mountedPath && file.content === currentDoc()) return;
      swapDoc(file.content, langForPath(file.path));
      mountedPath = file.path;
    });
  });

  onDestroy(() => {
    view?.destroy();
    view = undefined;
  });

  function baseExtensions(compartment: Compartment, langExtensions: Extension[]): Extension[] {
    return [
      keymap.of([
        {
          key: 'Mod-s',
          preventDefault: true,
          run: () => {
            void save();
            return true;
          },
        },
        ...defaultKeymap,
        ...historyKeymap,
      ]),
      lineNumbers(),
      highlightActiveLine(),
      history(),
      bracketMatching(),
      indentOnInput(),
      oneDark,
      compartment.of(langExtensions),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged || suppressUpdates) return;
        const path = mountedPath;
        if (!path) return;
        editorTabs.updateContent(workspaceId, path, update.state.doc.toString());
      }),
    ];
  }

  function swapDoc(content: string, langId: LangId): void {
    if (!view || !langCompartment) return;
    suppressUpdates = true;
    try {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: content,
        },
        effects: langCompartment.reconfigure(langExtensionsFor(langId)),
      });
    } finally {
      suppressUpdates = false;
    }
  }

  function currentDoc(): string {
    return view?.state.doc.toString() ?? '';
  }

  /** Lazy-loads the right CodeMirror language package — keeps the
   *  initial editor bundle tight and the Editor mount synchronous. The
   *  switch returns an empty extension array for unknown languages. */
  async function loadLang(langId: LangId): Promise<Extension[]> {
    if (!langId) return [];
    switch (langId) {
      case 'javascript':
        return [(await import('@codemirror/lang-javascript')).javascript()];
      case 'rust':
        return [(await import('@codemirror/lang-rust')).rust()];
      case 'python':
        return [(await import('@codemirror/lang-python')).python()];
      case 'json':
        return [(await import('@codemirror/lang-json')).json()];
      case 'markdown':
        return [(await import('@codemirror/lang-markdown')).markdown()];
      case 'html':
        return [(await import('@codemirror/lang-html')).html()];
      case 'css':
        return [(await import('@codemirror/lang-css')).css()];
      case 'php':
        return [(await import('@codemirror/lang-php')).php()];
    }
  }

  function langExtensionsFor(langId: LangId): Extension[] {
    // Synchronous best-effort — the `loadLang` call kicks off the
    // import promise and re-dispatches when it resolves. For now
    // return empty so the swap is non-blocking.
    void langId;
    void loadLang(langId).then((exts) => {
      if (!view || !langCompartment) return;
      view.dispatch({
        effects: langCompartment.reconfigure(exts),
      });
    });
    return [];
  }

  async function save(): Promise<void> {
    const file = activeFile;
    if (!file || file.isBinary || saving) return;
    saving = true;
    try {
      const content = currentDoc();
      const resp = await api.file.write(workspaceId, file.path, content, file.diskSha1);
      editorTabs.markSaved(workspaceId, file.path, resp.sha1);
      addToast(`Saved ${file.path}`, 'success', 1500);
    } catch (err) {
      const msg = String(err);
      if (msg.includes('FileChangedOnDisk')) {
        addToast(
          `${file.path} changed on disk — reload before saving (your edits are still in the buffer).`,
          'error',
          8000
        );
      } else {
        addToast(`Save failed: ${msg}`, 'error');
      }
    } finally {
      saving = false;
    }
  }

  function fileLabel(file: OpenFile): string {
    return file.dirty ? `${file.path} •` : file.path;
  }
</script>

<div class="flex flex-col h-full bg-[var(--bg-base)]" data-testid="editor-view">
  <div
    class="flex items-center justify-between px-3 py-1.5 border-b border-[var(--border)] text-xs text-[var(--text-secondary)]"
  >
    {#if activeFile}
      <span data-testid="editor-active-path">{fileLabel(activeFile)}</span>
      <button
        type="button"
        onclick={save}
        disabled={!activeFile || activeFile.isBinary || saving}
        data-testid="editor-save"
        class="px-2 py-0.5 rounded border border-[var(--border)] hover:bg-[var(--bg-card)] text-[var(--text-primary)] disabled:opacity-50"
      >
        {saving ? 'Saving…' : 'Save (⌃S)'}
      </button>
    {:else}
      <span data-testid="editor-empty-label">No file open</span>
    {/if}
  </div>
  <div class="flex-1 overflow-hidden relative">
    {#if !activeFile}
      <div
        class="absolute inset-0 flex items-center justify-center text-sm text-[var(--text-muted)]"
        data-testid="editor-empty"
      >
        Click a file in the Files tab to open it here.
      </div>
    {:else if activeFile.isBinary}
      <div
        class="absolute inset-0 flex items-center justify-center text-sm text-[var(--text-muted)]"
        data-testid="editor-binary"
      >
        Binary file — preview not available.
      </div>
    {/if}
    <div
      bind:this={containerRef}
      data-testid="editor-codemirror"
      class:hidden={!activeFile || activeFile.isBinary}
      class="h-full overflow-auto font-mono text-xs"
    ></div>
  </div>
</div>
