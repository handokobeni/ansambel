/** Build a GitHub-shaped "view branch" URL from the canonical
 *  `repo_remote_url` and a branch name. Returns `null` when either input
 *  is empty or the remote URL doesn't match either supported scheme.
 *
 *  Two canonical shapes the Phase 3a-3 publisher writes are accepted:
 *  - `https://github.com/owner/repo` (https clone URL, post-canonicalise
 *    strips `.git` and lowercases host)
 *  - `git@github.com:owner/repo` (ssh clone URL, post-canonicalise same)
 *
 *  Self-hosted (GitLab / Bitbucket / Forgejo) remotes that match one of
 *  these shapes will get a URL that may 404; the link is best-effort.
 *  Returning the URL anyway is preferable to a hidden link — the user
 *  can see what was attempted and the worst case is a 404. */
export function githubBranchUrl(remoteUrl: string, branch: string): string | null {
  if (!remoteUrl || !branch) return null;
  let httpsBase: string;
  if (remoteUrl.startsWith('git@')) {
    const match = remoteUrl.match(/^git@([^:]+):(.+)$/);
    if (!match) return null;
    httpsBase = `https://${match[1]}/${match[2]}`;
  } else if (remoteUrl.startsWith('https://')) {
    httpsBase = remoteUrl;
  } else {
    return null;
  }
  return `${httpsBase}/tree/${encodeURIComponent(branch)}`;
}
