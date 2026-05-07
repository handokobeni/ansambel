import { describe, it, expect, beforeEach } from 'vitest';
import { workspaceTabs } from './workspace-tabs.svelte';

beforeEach(() => {
  workspaceTabs.reset();
});

describe('workspaceTabs store', () => {
  it('defaults to chat for an unseen workspace', () => {
    expect(workspaceTabs.active('ws_a')).toBe('chat');
  });

  it('setActive switches to the requested tab', () => {
    workspaceTabs.setActive('ws_a', 'diff');
    expect(workspaceTabs.active('ws_a')).toBe('diff');
    workspaceTabs.setActive('ws_a', 'files');
    expect(workspaceTabs.active('ws_a')).toBe('files');
  });

  it('setActive is a no-op when the tab is already active', () => {
    workspaceTabs.setActive('ws_a', 'diff');
    // A second call with the same tab shouldn't blow up; the state is
    // still 'diff' afterwards.
    workspaceTabs.setActive('ws_a', 'diff');
    expect(workspaceTabs.active('ws_a')).toBe('diff');
  });

  it('expanded paths persist across active-tab changes', () => {
    workspaceTabs.toggleExpanded('ws_a', 'src');
    workspaceTabs.toggleExpanded('ws_a', 'src/lib');
    workspaceTabs.setActive('ws_a', 'chat');
    workspaceTabs.setActive('ws_a', 'files');
    const set = workspaceTabs.expanded('ws_a');
    expect(set.has('src')).toBe(true);
    expect(set.has('src/lib')).toBe(true);
  });

  it('toggleExpanded round-trips between expanded and collapsed', () => {
    expect(workspaceTabs.toggleExpanded('ws_a', 'src')).toBe('expanded');
    expect(workspaceTabs.expanded('ws_a').has('src')).toBe(true);
    expect(workspaceTabs.toggleExpanded('ws_a', 'src')).toBe('collapsed');
    expect(workspaceTabs.expanded('ws_a').has('src')).toBe(false);
  });

  it('separate workspaces have isolated tab state', () => {
    workspaceTabs.setActive('ws_a', 'diff');
    workspaceTabs.toggleExpanded('ws_a', 'src');
    expect(workspaceTabs.active('ws_b')).toBe('chat');
    expect(workspaceTabs.expanded('ws_b').size).toBe(0);
  });

  it('forget drops both active and expanded state', () => {
    workspaceTabs.setActive('ws_a', 'files');
    workspaceTabs.toggleExpanded('ws_a', 'src');
    workspaceTabs.forget('ws_a');
    // Re-reading recreates a fresh default — proves the entry was deleted.
    expect(workspaceTabs.active('ws_a')).toBe('chat');
    expect(workspaceTabs.expanded('ws_a').size).toBe(0);
  });

  it('reset clears every workspace at once', () => {
    workspaceTabs.setActive('ws_a', 'diff');
    workspaceTabs.setActive('ws_b', 'files');
    workspaceTabs.reset();
    expect(workspaceTabs.active('ws_a')).toBe('chat');
    expect(workspaceTabs.active('ws_b')).toBe('chat');
  });
});
