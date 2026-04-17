/**
 * Tauri IPC shim for Playwright E2E tests.
 *
 * In Vite dev mode (no Tauri binary), window.__TAURI_INTERNALS__ is absent.
 * This module installs it via page.addInitScript so that:
 *  - All Tauri commands are intercepted by a configurable handler map.
 *  - The @tauri-apps/api/core invoke() call finds the shim before app code runs.
 */

import type { Page } from '@playwright/test';
import type { Repo, Workspace } from '../../../src/lib/types';

export interface ShimConfig {
  /** Override fixture path returned by plugin:dialog|open */
  dialogOpenPath?: string;
  /** Initial repos returned by list_repos */
  initialRepos?: Repo[];
  /** Initial workspaces returned by list_workspaces */
  initialWorkspaces?: Workspace[];
}

/**
 * Install a full __TAURI_INTERNALS__ mock on the page before any app code
 * loads. Must be called BEFORE page.goto().
 */
export async function installTauriShim(page: Page, config: ShimConfig): Promise<void> {
  await page.addInitScript(
    ({
      dialogOpenPath,
      initialRepos,
      initialWorkspaces,
    }: {
      dialogOpenPath?: string;
      initialRepos?: unknown[];
      initialWorkspaces?: unknown[];
    }) => {
      // In-memory state shared across invoke calls
      const state = {
        repos: (initialRepos ?? []) as Array<{
          id: string;
          name: string;
          path: string;
          gh_profile: string | null;
          default_branch: string;
          created_at: number;
          updated_at: number;
        }>,
        workspaces: (initialWorkspaces ?? []) as Array<{
          id: string;
          repo_id: string;
          branch: string;
          base_branch: string;
          custom_branch: boolean;
          title: string;
          description: string;
          status: string;
          column: string;
          created_at: number;
          updated_at: number;
        }>,
        nextRepoSeq: 1,
        nextWsSeq: 1,
      };

      function makeRepoId() {
        return `repo_e2e${String(state.nextRepoSeq++).padStart(4, '0')}`;
      }
      function makeWsId() {
        return `ws_e2e${String(state.nextWsSeq++).padStart(4, '0')}`;
      }

      const now = () => Math.floor(Date.now() / 1000);

      async function shimInvoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
        switch (cmd) {
          case 'get_app_version':
            return '0.0.0-e2e';

          case 'list_repos':
            return [...state.repos];

          case 'add_repo': {
            const repoPath = args.path as string;
            const name = repoPath.split('/').pop() ?? repoPath;
            const existing = state.repos.find((r) => r.path === repoPath);
            if (existing) return existing;
            const repo = {
              id: makeRepoId(),
              name,
              path: repoPath,
              gh_profile: null,
              default_branch: 'main',
              created_at: now(),
              updated_at: now(),
            };
            state.repos.push(repo);
            return repo;
          }

          case 'remove_repo': {
            const repoId = args.repoId as string;
            const idx = state.repos.findIndex((r) => r.id === repoId);
            if (idx !== -1) state.repos.splice(idx, 1);
            return undefined;
          }

          case 'update_repo_gh_profile': {
            const r = state.repos.find((r) => r.id === args.repoId);
            if (r) r.gh_profile = args.ghProfile as string | null;
            return undefined;
          }

          case 'list_workspaces': {
            const repoId = args.repoId as string | undefined;
            if (repoId) return state.workspaces.filter((w) => w.repo_id === repoId);
            return [...state.workspaces];
          }

          case 'create_workspace': {
            const id = makeWsId();
            const title = args.title as string;
            const repoId = args.repoId as string;
            const description = (args.description as string | undefined) ?? '';
            const branchName =
              (args.branchName as string | undefined) ??
              `feat/${title.toLowerCase().replace(/\s+/g, '-').slice(0, 40)}`;
            const repo = state.repos.find((r) => r.id === repoId);
            const baseBranch = repo?.default_branch ?? 'main';
            const ws = {
              id,
              repo_id: repoId,
              branch: branchName,
              base_branch: baseBranch,
              custom_branch: !!args.branchName,
              title,
              description,
              status: 'not_started',
              column: 'todo',
              created_at: now(),
              updated_at: now(),
            };
            state.workspaces.push(ws);
            return ws;
          }

          case 'remove_workspace': {
            const wsId = args.workspaceId as string;
            const idx = state.workspaces.findIndex((w) => w.id === wsId);
            if (idx !== -1) state.workspaces.splice(idx, 1);
            return undefined;
          }

          case 'plugin:dialog|open':
            return dialogOpenPath ?? null;

          default:
            // Unknown commands return undefined rather than throwing so the app
            // degrades gracefully
            return undefined;
        }
      }

      // Install shim — must be ready before any import of @tauri-apps/api/core
      (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] = {
        invoke: shimInvoke,
        transformCallback: (cb: (v: unknown) => void) => {
          const id = Math.floor(Math.random() * 1e9);
          (window as unknown as Record<string, unknown>)[`_${id}`] = cb;
          return id;
        },
        unregisterCallback: (id: number) => {
          delete (window as unknown as Record<string, unknown>)[`_${id}`];
        },
        convertFileSrc: (path: string) => path,
      };
    },
    {
      dialogOpenPath: config.dialogOpenPath,
      initialRepos: config.initialRepos,
      initialWorkspaces: config.initialWorkspaces,
    }
  );
}
