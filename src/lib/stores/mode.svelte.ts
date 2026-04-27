// src/lib/stores/mode.svelte.ts
import type { Mode } from '$lib/types';

export class ModeStore {
  mode = $state<Mode>('plan');

  set(next: Mode): void {
    this.mode = next;
  }
}

export const modeStore = new ModeStore();
