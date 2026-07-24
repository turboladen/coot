// Live toast state (Svelte 5 runes module) — billz-086. Mirrors
// savedQueries.svelte.ts: mutate the exported `$state` object's fields in place,
// never reassign the export.
//
// Pure stack logic (addToast/dismissToast/autoDismissMs) lives in the rune-free
// toastLogic.ts so it's `bun test`-able; this module owns only the live state and
// the pending-timer bookkeeping, which is the part that can't be pure.
import { addToast, autoDismissMs, dismissToast, type Toast, type ToastKind } from "./toastLogic";

export const toasts = $state<{ list: Toast[] }>({ list: [] });

// One pending auto-dismiss timer per non-sticky toast. Kept OUTSIDE the $state
// object deliberately — timer handles aren't UI state and shouldn't be tracked.
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function clearTimer(id: string): void {
  const handle = timers.get(id);
  if (handle !== undefined) {
    clearTimeout(handle);
    timers.delete(id);
  }
}

/**
 * Show a toast. Returns its id so a caller can dismiss it early.
 *
 * Errors stay until dismissed (`autoDismissMs` returns null); success/info expire.
 */
export function pushToast(kind: ToastKind, text: string): string {
  const id = crypto.randomUUID();
  const { list, evicted } = addToast(toasts.list, { id, kind, text });
  // Clear timers for anything pushed off the stack, so a dead toast's pending
  // timeout can't fire later and dismiss whatever is on screen by then.
  for (const t of evicted) clearTimer(t.id);
  toasts.list = list;

  const ms = autoDismissMs(kind);
  if (ms !== null) timers.set(id, setTimeout(() => dismiss(id), ms));
  return id;
}

export function dismiss(id: string): void {
  clearTimer(id);
  toasts.list = dismissToast(toasts.list, id);
}
