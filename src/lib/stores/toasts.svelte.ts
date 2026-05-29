import { SvelteMap } from 'svelte/reactivity';

export type ToastType = 'error' | 'info' | 'success';

export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
}

export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  createdAt: number;
  /** Optional inline action buttons (rendered next to the dismiss control). */
  actions?: ToastAction[];
  /** Effective auto-dismiss duration in ms (0 = sticky). Stored for tests
   *  and rendering; the timer is set up immediately on add. */
  timeoutMs?: number;
}

export interface ToastOptions {
  /** Inline action buttons. */
  actions?: ToastAction[];
  /** Override the default auto-dismiss for this toast (ms). 0 = sticky. */
  timeoutMs?: number;
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

/**
 * Add a toast.
 *
 * The third argument is overloaded for backward-compat:
 *   - `number` — explicit auto-dismiss in ms (0 = sticky).
 *   - `ToastOptions` — `{ actions?, timeoutMs? }` for richer toasts (e.g. the
 *     auto-create undo toast in `App.svelte:handleMove`).
 *   - `undefined` — use `DURATIONS[type]` as the default.
 */
export function addToast(
  message: string,
  type: ToastType = 'error',
  opts?: number | ToastOptions
): string {
  const id = `toast-${++counter}`;

  let durationMs: number;
  let actions: ToastAction[] | undefined;

  if (typeof opts === 'number') {
    durationMs = opts;
  } else if (opts) {
    durationMs = opts.timeoutMs ?? DURATIONS[type];
    actions = opts.actions;
  } else {
    durationMs = DURATIONS[type];
  }

  toasts.set(id, {
    id,
    message,
    type,
    createdAt: Date.now(),
    actions,
    timeoutMs: durationMs,
  });

  if (durationMs > 0) {
    timers.set(
      id,
      setTimeout(() => removeToast(id), durationMs)
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
