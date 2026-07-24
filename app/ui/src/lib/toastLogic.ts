// Pure toast-stack logic (billz-086). Rune-free plain TS so `bun test` imports it
// without the Svelte compiler; the runes store + timers live in toasts.svelte.ts.
// Same split as tabsLogic.ts / savedQueriesLogic.ts.
//
// Scope boundary: toasts are TRANSIENT APP-LEVEL events (saved to library,
// connection failed, background refresh error). Query execution output — row
// counts, batch results, SQL errors — stays in the Messages pane, which is a
// durable record you re-read. Don't reroute those here.

export type ToastKind = "success" | "error" | "info";

export type Toast = {
  id: string;
  kind: ToastKind;
  text: string;
};

/** Most toasts on screen at once; older ones are evicted to make room. */
export const MAX_TOASTS = 4;

/** How long a non-sticky toast lives. */
export const TOAST_MS = 4000;

/**
 * Append `t`, evicting the oldest entries until the stack fits `max`.
 *
 * Returns the evicted toasts alongside the new list *by design*: the store owns
 * a pending `setTimeout` per auto-dismissing toast, and an evicted toast's timer
 * must be cleared. Handing back the casualties makes that impossible to forget —
 * the alternative (diffing the lists at the call site) is where orphan timers,
 * which later fire against an unrelated toast, come from.
 */
export function addToast(
  list: Toast[],
  t: Toast,
  max = MAX_TOASTS,
): { list: Toast[]; evicted: Toast[] } {
  const next = [...list, t];
  const overflow = Math.max(0, next.length - max);
  return { list: next.slice(overflow), evicted: next.slice(0, overflow) };
}

/**
 * Remove one toast by id. Unknown ids are a no-op (a double-click on ✕ races the
 * timer) — and a *true* no-op: the input list is handed straight back when nothing
 * matched, so the store's `toasts.list = dismissToast(...)` doesn't invalidate the
 * `$state` field (and re-run the each-block) for a dismissal that did nothing.
 */
export function dismissToast(list: Toast[], id: string): Toast[] {
  const next = list.filter((t) => t.id !== id);
  return next.length === list.length ? list : next;
}

/**
 * How long this kind stays up, or `null` for "until dismissed".
 *
 * Errors are sticky: an error that vanished before you read it is precisely the
 * failure mode a toast system is supposed to fix.
 */
export function autoDismissMs(kind: ToastKind): number | null {
  return kind === "error" ? null : TOAST_MS;
}

/**
 * Does this kind interrupt the screen reader, or wait its turn?
 *
 * Lives here rather than in ToastHost so ALL per-kind policy is in one file next
 * to `autoDismissMs` — otherwise "errors are the special kind" is encoded in two
 * places that can drift apart.
 */
export function isAssertive(kind: ToastKind): boolean {
  return kind === "error";
}
