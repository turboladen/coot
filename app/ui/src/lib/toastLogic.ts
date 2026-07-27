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
//
// COALESCING (billz-667) sits in front of both: a message identical to the
// NEWEST one bumps a repeat count in place instead of appending. A failure that
// recurs on a timer — a background refresh against a down box — would otherwise
// push one never-expiring error per tick, so "errors are never evicted" would be
// satisfied by a stack holding N copies of one message and nothing else. The
// count is rendered rather than swallowed: that a failure is RECURRING is
// information, and dropping the repeat silently would hide it.

export type ToastKind = "success" | "error" | "info";

export type Toast = {
  id: string;
  kind: ToastKind;
  text: string;
  /** How many times this exact message has arrived in a row. 1 = shown once. */
  repeat: number;
};

/**
 * How many toasts render at once. Purely a display bound — see the note above.
 * Also caps how many TRANSIENT toasts are retained, since those expire anyway.
 */
export const MAX_VISIBLE = 4;

/** How long a non-sticky toast lives. */
export const TOAST_MS = 4000;

/**
 * Add `t`: either COALESCE it into the newest toast, or append and evict.
 *
 * Coalescing matches the NEWEST entry only, on identical `kind` + `text`, and
 * bumps its `repeat` in place (same `id`, same position). Matching anywhere in
 * the stack was rejected on three counts: a recurrence could merge into a toast
 * already collapsed behind the overflow counter, so nothing visible would happen
 * at all; it would break `partitionToasts`' guarantee that a just-raised toast is
 * on screen; and `A A B A` would claim the two runs of `A` were contiguous when
 * the third arrived *after* `B`. Recurrence is the information this feature
 * exists to surface — misdating it is the same failure as dropping it.
 *
 * Sticky toasts (errors) are never evicted — they leave the stack only when the
 * user dismisses them. A burst of routine success toasts therefore cannot
 * destroy an unread error.
 *
 * Returns `active` and `evicted` alongside the new list *by design*: the store
 * owns a pending `setTimeout` per auto-dismissing toast.
 *   - `evicted` — an evicted toast's timer must be cleared, and handing back the
 *     casualties makes that impossible to forget. Diffing the lists at the call
 *     site is where orphan timers, which later fire against an unrelated toast,
 *     come from.
 *   - `active` — the toast now live *given a sane cap*, i.e. the one the store
 *     must (re)arm a timer against. On a coalesce that is the EXISTING toast,
 *     whose id is *not* the id the store just minted for `t`; keying a timer off
 *     the minted id would arm a timeout for something that isn't in the stack.
 *     The hedge is real but degenerate: with `maxTransient` small enough (0) the
 *     eviction loop below can evict the transient it just appended and still hand
 *     it back here. No caller passes such a cap; billz-01c tracks the arming.
 * Whether a coalesce happened is not returned separately — it is exactly
 * `active.repeat > 1`, since the append path always carries `repeat === 1`.
 */
export function addToast(
  list: Toast[],
  t: Toast,
  maxTransient = MAX_VISIBLE,
): { list: Toast[]; evicted: Toast[]; active: Toast } {
  const newest = list[list.length - 1];
  if (newest !== undefined && newest.kind === t.kind && newest.text === t.text) {
    // Spread into a NEW object rather than mutating: this module is pure, and the
    // list it was handed is the store's `$state` array.
    const bumped = { ...newest, repeat: newest.repeat + 1 };
    // Nothing is appended, so the transient count is unchanged and eviction is
    // structurally impossible here.
    return { list: [...list.slice(0, -1), bumped], evicted: [], active: bumped };
  }

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
  return { list: next, evicted, active: t };
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

/**
 * What the screen reader hears — the message, plus the repeat count once there
 * is one.
 *
 * The count is not decoration here. The announcer regions are single strings, and
 * a live region only speaks when its content CHANGES; a bare repeated message
 * would write the identical string and be silently swallowed. Folding the count
 * in makes each repeat a distinct string, so a recurring failure is announced
 * rather than being information only a sighted user gets from the badge.
 *
 * Lives here, next to `isAssertive`/`autoDismissMs`, so all announcement policy
 * stays in one file. See the `announcer` docstring in toasts.svelte.ts for the
 * repeats this still does NOT reach (billz-b8f).
 */
export function announcementText(text: string, repeat: number): string {
  return repeat > 1 ? `${text} (${repeat} times)` : text;
}
