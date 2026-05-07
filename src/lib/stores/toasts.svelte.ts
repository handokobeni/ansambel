import { SvelteMap } from 'svelte/reactivity';

export type ToastType = 'error' | 'info' | 'success';

export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  createdAt: number;
}

const toasts = new SvelteMap<string, Toast>();
// `timers` is only consulted from the auto-dismiss path (and on manual
// removal). Nothing reactive reads it, so a plain Map is correct here —
// using SvelteMap would just trigger needless reactive bookkeeping.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const timers = new Map<string, ReturnType<typeof setTimeout>>();

let counter = 0;

/** Default auto-dismiss durations (ms) per toast type. 0 keeps the toast indefinitely. */
const DURATIONS: Record<ToastType, number> = {
  error: 6000,
  info: 4000,
  success: 3000,
};

export function addToast(message: string, type: ToastType = 'error', duration?: number): string {
  const id = `toast-${++counter}`;
  toasts.set(id, { id, message, type, createdAt: Date.now() });

  const ms = duration ?? DURATIONS[type];
  if (ms > 0) {
    timers.set(
      id,
      setTimeout(() => removeToast(id), ms)
    );
  }

  return id;
}

export function removeToast(id: string): void {
  toasts.delete(id);
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
}

export function getToasts(): SvelteMap<string, Toast> {
  return toasts;
}
