import { describe, it, expect, beforeEach } from 'vitest';
import { viewMissing } from './view-missing.svelte';

describe('viewMissing store', () => {
  beforeEach(() => {
    viewMissing.clear();
  });

  it('starts empty', () => {
    expect(viewMissing.entries.size).toBe(0);
  });

  it('records a missing view by repo_id', () => {
    viewMissing.report('repo_x', 'vw_gone');
    expect(viewMissing.entries.get('repo_x')).toBe('vw_gone');
  });

  it('dismiss removes one repo entry', () => {
    viewMissing.report('repo_x', 'vw_gone');
    viewMissing.report('repo_y', 'vw_other');
    viewMissing.dismiss('repo_x');
    expect(viewMissing.entries.has('repo_x')).toBe(false);
    expect(viewMissing.entries.get('repo_y')).toBe('vw_other');
  });

  it('clear wipes all entries', () => {
    viewMissing.report('repo_x', 'vw_gone');
    viewMissing.clear();
    expect(viewMissing.entries.size).toBe(0);
  });
});
