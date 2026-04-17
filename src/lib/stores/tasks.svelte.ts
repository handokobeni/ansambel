// src/lib/stores/tasks.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import type { Task, CreateTaskArgs, TaskPatch, KanbanColumn } from '$lib/types';

export class TasksStore {
  readonly tasks = new SvelteMap<string, SvelteMap<string, Task>>();
  selectedTaskId = $state<string | null>(null);

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

  async move(taskId: string, column: KanbanColumn, order: number): Promise<void> {
    await api.task.move(taskId, column, order);
    for (const [, map] of this.tasks) {
      const existing = map.get(taskId);
      if (existing) {
        map.set(taskId, { ...existing, column, order });
        return;
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
}

export const tasks = new TasksStore();
