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
  announcementText,
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
 * Known gap (billz-b8f), narrowed but NOT closed by coalescing: each region is a
 * single string, and writing a string equal to what the region already holds is
 * not a DOM change, so it doesn't re-announce. Coalescing fixes only the case
 * where the previous instance is still the newest toast — the repeat count makes
 * the string differ (see `announcementText`). Silent repeats remain when the
 * previous instance has already LEFT the stack (auto-dismissed, or dismissed via
 * ✕ — `dismiss` deliberately doesn't touch the announcer, only `dismissAll`
 * does), and when two identical messages are separated only by a message of the
 * other politeness, since the two regions are independent fields. A real fix
 * needs a nonce or a clear-then-set on the region; that's billz-b8f, not here.
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
 * Show a toast. Returns the id of the toast now on screen, so a caller can
 * dismiss it early.
 *
 * Errors stay until dismissed and are never evicted; success/info expire.
 *
 * A message identical to the newest one COALESCES into it (billz-667), which
 * makes the returned id worth reading carefully: on a repeat it is the id of the
 * pre-existing toast, not of this call, and dismissing it clears the whole
 * coalesced run rather than one occurrence. Returning the freshly minted id
 * instead would type-check and do nothing, since no such toast is in the stack.
 */
export function pushToast(kind: ToastKind, text: string): string {
  const id = crypto.randomUUID();
  const { list, evicted, active } = addToast(toasts.list, { id, kind, text, repeat: 1 });
  // Clear timers for anything pushed off the stack, so a dead toast's pending
  // timeout can't fire later and dismiss whatever is on screen by then.
  for (const t of evicted) clearTimer(t.id);
  toasts.list = list;

  const announcement = announcementText(active.text, active.repeat);
  if (isAssertive(kind)) announcer.assertive = announcement;
  else announcer.polite = announcement;

  // Re-arm from zero. On a coalesced repeat `active.id` is the EXISTING toast,
  // whose timeout is still pending: clearing first is what makes a repeat reset
  // the countdown instead of leaving the original deadline standing — and what
  // stops `timers.set` overwriting a live handle, orphaning it to fire later
  // against whatever occupies the stack by then. On the append path the id is
  // brand new, so this is a no-op.
  clearTimer(active.id);
  const ms = autoDismissMs(kind);
  if (ms !== null) timers.set(active.id, setTimeout(() => dismiss(active.id), ms));
  return active.id;
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
