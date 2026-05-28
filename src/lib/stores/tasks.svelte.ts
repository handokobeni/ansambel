// src/lib/stores/tasks.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import type { Task, CreateTaskArgs, TaskPatch, KanbanColumn, UnlinkResult } from '$lib/types';

export class TasksStore {
  readonly tasks = new SvelteMap<string, SvelteMap<string, Task>>();
  readonly loadingByRepo = new SvelteMap<string, boolean>();
  selectedTaskId = $state<string | null>(null);
  highlightedTaskId = $state<string | null>(null);

  isLoading(repoId: string): boolean {
    return this.loadingByRepo.get(repoId) === true;
  }

  private getOrCreate(repoId: string): SvelteMap<string, Task> {
    let map = this.tasks.get(repoId);
    if (!map) {
      map = new SvelteMap<string, Task>();
      this.tasks.set(repoId, map);
    }
    return map;
  }

  async loadForRepo(repoId: string): Promise<void> {
    const list = await api.task.list(repoId);
    const map = this.getOrCreate(repoId);
    map.clear();
    for (const task of list) {
      map.set(task.id, task);
    }
  }

  async add(args: CreateTaskArgs): Promise<Task> {
    const task = await api.task.add(args);
    this.getOrCreate(task.repo_id).set(task.id, task);
    return task;
  }

  async update(taskId: string, patch: TaskPatch): Promise<void> {
    await api.task.update(taskId, patch);
    for (const [, map] of this.tasks) {
      const existing = map.get(taskId);
      if (existing) {
        map.set(taskId, { ...existing, ...patch });
        return;
      }
    }
  }

  async move(taskId: string, column: KanbanColumn, order: number): Promise<Task> {
    // Find the task across all repo maps (same pattern as update/remove)
    let repoIdOfTask: string | null = null;
    let prev: Task | undefined;
    for (const [repoId, map] of this.tasks) {
      const existing = map.get(taskId);
      if (existing) {
        repoIdOfTask = repoId;
        prev = existing;
        break;
      }
    }
    if (!prev || !repoIdOfTask) {
      // Fall back — let backend handle missing-id; hydrate the returned task.
      const updated = await api.task.move(taskId, column, order);
      this.getOrCreate(updated.repo_id).set(updated.id, updated);
      return updated;
    }
    // Optimistic write
    this.getOrCreate(repoIdOfTask).set(taskId, { ...prev, column, order });
    try {
      // Use the backend's returned Task so we pick up auto-set workspace_id
      // when moving into InProgress.
      const updated = await api.task.move(taskId, column, order);
      // Reconcile — backend may route task to a different repo map (defensive).
      this.getOrCreate(updated.repo_id).set(updated.id, updated);
      return updated;
    } catch (err) {
      // Revert on error
      this.getOrCreate(repoIdOfTask).set(taskId, prev);
      addToast(`Move failed: ${err instanceof Error ? err.message : String(err)}`, 'error');
      throw err;
    }
  }

  async refresh(repoId?: string): Promise<void> {
    if (repoId !== undefined) {
      this.loadingByRepo.set(repoId, true);
    }
    try {
      const tasks = await api.task.refresh(repoId);
      if (repoId !== undefined) {
        // Clear only the entries for this repo then re-populate
        const map = this.getOrCreate(repoId);
        map.clear();
        for (const task of tasks) {
          this.getOrCreate(task.repo_id).set(task.id, task);
        }
      } else {
        // Global refresh — clear all nested maps then re-populate by repo_id
        for (const [, map] of this.tasks) {
          map.clear();
        }
        for (const task of tasks) {
          this.getOrCreate(task.repo_id).set(task.id, task);
        }
      }
    } finally {
      if (repoId !== undefined) {
        this.loadingByRepo.set(repoId, false);
      }
    }
  }

  async remove(taskId: string, force?: boolean): Promise<void> {
    await api.task.remove(taskId, force);
    for (const [, map] of this.tasks) {
      if (map.has(taskId)) {
        map.delete(taskId);
        if (this.selectedTaskId === taskId) {
          this.selectedTaskId = null;
        }
        return;
      }
    }
  }

  async link(taskId: string, workspaceId: string, repoId: string): Promise<void> {
    await api.task.linkToWorkspace(taskId, workspaceId);
    const fresh = await api.task.list(repoId);
    const map = this.getOrCreate(repoId);
    map.clear();
    for (const task of fresh) {
      map.set(task.id, task);
    }
    await workspaces.loadForRepo(repoId);
  }

  async unlink(taskId: string, force: boolean, repoId: string): Promise<UnlinkResult> {
    const result = await api.task.unlinkFromWorkspace(taskId, force);
    if (force) {
      const fresh = await api.task.list(repoId);
      const map = this.getOrCreate(repoId);
      map.clear();
      for (const task of fresh) {
        map.set(task.id, task);
      }
      await workspaces.loadForRepo(repoId);
    }
    return result;
  }

  listForRepo(repoId: string): Task[] {
    const map = this.tasks.get(repoId);
    if (!map) return [];
    return Array.from(map.values());
  }

  listForColumn(repoId: string, column: KanbanColumn): Task[] {
    return this.listForRepo(repoId)
      .filter((t) => t.column === column)
      .sort((a, b) => a.order - b.order);
  }

  selectTask(id: string | null): void {
    this.selectedTaskId = id;
  }

  // Cross-repo lookup. Linear scan is fine — tasks per repo are dozens at
  // most, and this is only called from sidebar expand/render hot paths.
  byId(id: string): Task | undefined {
    for (const map of this.tasks.values()) {
      const t = map.get(id);
      if (t) return t;
    }
    return undefined;
  }

  // Highlight a task in the kanban (driven from the sidebar's expanded
  // workspace row). KanbanBoard reads `highlightedTaskId` to apply a brief
  // ring style; setting null clears the highlight.
  highlight(id: string | null): void {
    this.highlightedTaskId = id;
  }
}

export const tasks = new TasksStore();
