import { describe, it, expect } from 'vitest';
import { githubBranchUrl } from './github-url';

describe('githubBranchUrl', () => {
  it('builds https URL from an https remote', () => {
    expect(githubBranchUrl('https://github.com/foo/bar', 'main')).toBe(
      'https://github.com/foo/bar/tree/main'
    );
  });

  it('converts ssh-style git@ remote to https URL', () => {
    expect(githubBranchUrl('git@github.com:foo/bar', 'feat/x')).toBe(
      'https://github.com/foo/bar/tree/feat%2Fx'
    );
  });

  it('returns null when remote URL is empty', () => {
    expect(githubBranchUrl('', 'main')).toBeNull();
  });

  it('returns null when branch is empty', () => {
    expect(githubBranchUrl('https://github.com/foo/bar', '')).toBeNull();
  });

  it('returns null for unknown URL scheme', () => {
    expect(githubBranchUrl('ftp://example.com/foo', 'main')).toBeNull();
    expect(githubBranchUrl('totally-not-a-url', 'main')).toBeNull();
  });

  it('returns null when ssh-style URL lacks colon separator', () => {
    expect(githubBranchUrl('git@github.com/foo/bar', 'main')).toBeNull();
  });

  it('URL-encodes the branch name to handle slashes and special chars', () => {
    expect(githubBranchUrl('https://github.com/foo/bar', 'feat/auth & fix')).toBe(
      'https://github.com/foo/bar/tree/feat%2Fauth%20%26%20fix'
    );
  });
});
