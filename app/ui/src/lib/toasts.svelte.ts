// Live toast state (Svelte 5 runes module) — billz-086. Mirrors
// savedQueries.svelte.ts: mutate the exported `$state` object's fields in place,
// never reassign the export.
//
// Pure stack logic (addToast/partitionToasts/dismissToast/...) lives in the
// rune-free toastLogic.ts so it's `bun test`-able; this module owns only the live
// state, the pending-timer bookkeeping, and the screen-reader announcement text —
// the parts that can't be pure.
import {
  addToast,
  autoDismissMs,
  dismissAllToasts,
  dismissToast,
  isAssertive,
  type Toast,
  type ToastKind,
} from "./toastLogic";

export const toasts = $state<{ list: Toast[] }>({ list: [] });

/**
 * Text mirrored into the two pre-mounted, visually-hidden live regions in
 * ToastHost.
 *
 * The visual toasts carry NO live-region markup: `role="alert"` on a child of an
 * `aria-live="polite"` container nests live regions, which double-announces on
 * some AT and silently drops the assertive intent on others. Announcing through
 * dedicated regions keeps the visual stack fully operable (its dismiss buttons
 * stay in the a11y tree — `aria-hidden` around focusable controls would be its
 * own violation) while the announcement politeness stays correct.
 *
 * Known gap: pushing the identical string twice in a row is not a DOM change, so
 * the second one doesn't re-announce. billz-667 (coalescing repeats) makes that
 * case render as a repeat count instead, which is the better answer anyway.
 */
export const announcer = $state<{ polite: string; assertive: string }>({
  polite: "",
  assertive: "",
});

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
 * Errors stay until dismissed and are never evicted; success/info expire.
 */
export function pushToast(kind: ToastKind, text: string): string {
  const id = crypto.randomUUID();
  const { list, evicted } = addToast(toasts.list, { id, kind, text });
  // Clear timers for anything pushed off the stack, so a dead toast's pending
  // timeout can't fire later and dismiss whatever is on screen by then.
  for (const t of evicted) clearTimer(t.id);
  toasts.list = list;

  if (isAssertive(kind)) announcer.assertive = text;
  else announcer.polite = text;

  const ms = autoDismissMs(kind);
  if (ms !== null) timers.set(id, setTimeout(() => dismiss(id), ms));
  return id;
}

export function dismiss(id: string): void {
  clearTimer(id);
  toasts.list = dismissToast(toasts.list, id);
}

/** Clear the whole stack — the escape hatch when errors have piled up. */
export function dismissAll(): void {
  for (const t of toasts.list) clearTimer(t.id);
  toasts.list = dismissAllToasts(toasts.list);
  announcer.polite = "";
  announcer.assertive = "";
}
