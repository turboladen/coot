// Pure toast-stack logic (billz-086). Rune-free plain TS so `bun test` imports it
// without the Svelte compiler; the runes store + timers live in toasts.svelte.ts.
// Same split as tabsLogic.ts / savedQueriesLogic.ts.
//
// Scope boundary: toasts are TRANSIENT APP-LEVEL events (saved to library,
// connection failed, background refresh error). Query execution output — row
// counts, batch results, SQL errors — stays in the Messages pane, which is a
// durable record you re-read. Don't reroute those here.
//
// RETENTION vs DISPLAY are two different limits, deliberately separated:
//   - Errors are never auto-dismissed AND never evicted. They accumulate.
//   - Only MAX_VISIBLE toasts are RENDERED; the older ones collapse behind a
//     counter (partitionToasts) rather than being destroyed.
// Conflating the two is how "errors stay until you dismiss them" quietly stops
// being true in exactly the session that's generating errors.

export type ToastKind = "success" | "error" | "info";

export type Toast = {
  id: string;
  kind: ToastKind;
  text: string;
};

/**
 * How many toasts render at once. Purely a display bound — see the note above.
 * Also caps how many TRANSIENT toasts are retained, since those expire anyway.
 */
export const MAX_VISIBLE = 4;

/** How long a non-sticky toast lives. */
export const TOAST_MS = 4000;

/**
 * Append `t`, evicting only TRANSIENT toasts to stay within `maxTransient`.
 *
 * Sticky toasts (errors) are never evicted — they leave the stack only when the
 * user dismisses them. A burst of routine success toasts therefore cannot
 * destroy an unread error.
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
  maxTransient = MAX_VISIBLE,
): { list: Toast[]; evicted: Toast[] } {
  const next = [...list, t];
  const evicted: Toast[] = [];
  while (next.filter((x) => !isSticky(x.kind)).length > maxTransient) {
    const oldest = next.findIndex((x) => !isSticky(x.kind));
    // Structural guard, not defensive noise: `splice(-1, 1)` would delete the
    // NEWEST entry (a sticky error) and leave the transient count unchanged, so
    // the loop would drain every error and then spin forever on an empty array.
    // No caller passes a cap that reaches this, but the failure mode is silent
    // data loss plus a hang, which is not a thing to leave one typo away.
    if (oldest === -1) break;
    evicted.push(...next.splice(oldest, 1));
  }
  return { list: next, evicted };
}

/**
 * Split the stack into what renders and what collapses behind a counter.
 *
 * The newest `maxVisible` are shown, so a just-raised toast is always on screen;
 * anything older stays in `hidden`, still readable once expanded, never dropped.
 */
export function partitionToasts(
  list: Toast[],
  maxVisible = MAX_VISIBLE,
): { visible: Toast[]; hidden: Toast[] } {
  if (list.length <= maxVisible) return { visible: list, hidden: [] };
  const cut = list.length - maxVisible;
  return { visible: list.slice(cut), hidden: list.slice(0, cut) };
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

/** Clear the stack. Errors pile up unbounded, so "dismiss all" is a real need. */
export function dismissAllToasts(_list: Toast[]): Toast[] {
  return [];
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
 * Does this kind stay until dismissed? Derived from `autoDismissMs` rather than
 * re-testing the kind, so the retention rule and the timer rule cannot drift.
 */
export function isSticky(kind: ToastKind): boolean {
  return autoDismissMs(kind) === null;
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
