// `bun test` — toastLogic.ts is rune-free plain TS, so it imports cleanly here
// with no Svelte compiler (unlike toasts.svelte.ts). Excluded from svelte-check
// via tsconfig `exclude`, same as tabsLogic.test.ts / savedQueriesLogic.test.ts.
import { describe, expect, test } from "bun:test";
import {
  addToast,
  announcementText,
  autoDismissMs,
  dismissAllToasts,
  dismissToast,
  isAssertive,
  isSticky,
  MAX_VISIBLE,
  partitionToasts,
  type Toast,
  type ToastKind,
  TOAST_MS,
} from "./toastLogic";

// `text` defaults to something derived from `id`, so distinct ids give distinct
// messages and nothing coalesces by accident. Pass it explicitly to force the
// collision the coalescing tests are about.
function t(id: string, kind: ToastKind = "info", text = `toast ${id}`): Toast {
  return { id, kind, text, repeat: 1 };
}

const ids = (list: Toast[]) => list.map((x) => x.id);

describe("addToast", () => {
  test("appends to the end (newest last)", () => {
    expect(ids(addToast([t("a")], t("b")).list)).toEqual(["a", "b"]);
  });

  test("does not mutate the input list", () => {
    const before = [t("a")];
    addToast(before, t("b"));
    expect(ids(before)).toEqual(["a"]);
  });

  test("nothing evicted below the cap", () => {
    expect(addToast([t("a")], t("b")).evicted).toEqual([]);
  });

  test("evicts the oldest TRANSIENT once over the cap", () => {
    const full = Array.from({ length: MAX_VISIBLE }, (_, i) => t(String(i)));
    const { list, evicted } = addToast(full, t("new"));
    expect(ids(list)).toEqual(["1", "2", "3", "new"]);
    // The store clears timers for whatever comes back here — an evicted toast's
    // pending setTimeout must not survive to dismiss an unrelated later toast.
    expect(ids(evicted)).toEqual(["0"]);
  });

  // The core of the retention rule: an error you never read must not be thrown
  // away to make room for "Saved to library". Errors accumulate without bound;
  // the DISPLAY cap is partitionToasts' job, not eviction's.
  describe("errors are never evicted", () => {
    test("a full stack of errors keeps every one when a success arrives", () => {
      const errs = Array.from({ length: MAX_VISIBLE }, (_, i) => t(`e${i}`, "error"));
      const { list, evicted } = addToast(errs, t("ok", "success"));
      expect(ids(list)).toEqual(["e0", "e1", "e2", "e3", "ok"]);
      expect(evicted).toEqual([]);
    });

    test("errors accumulate past the cap", () => {
      let list: Toast[] = [];
      for (let i = 0; i < 10; i++) list = addToast(list, t(`e${i}`, "error")).list;
      expect(list).toHaveLength(10);
      expect(list.every((x) => x.kind === "error")).toBe(true);
    });

    test("transients are evicted around the errors, oldest transient first", () => {
      // Already at the transient budget (i0..i3), plus an error that predates them.
      const start = [t("e", "error"), t("i0"), t("i1"), t("i2"), t("i3")];
      const { list, evicted } = addToast(start, t("i4"));
      // "e" survives despite being the OLDEST entry; the oldest TRANSIENT goes.
      expect(ids(list)).toEqual(["e", "i1", "i2", "i3", "i4"]);
      expect(ids(evicted)).toEqual(["i0"]);
    });

    // A cap no real caller passes, but the loop must terminate rather than
    // splice(-1) the newest error away and spin.
    test("a cap smaller than the sticky count terminates without eating errors", () => {
      const errs = Array.from({ length: 3 }, (_, i) => t(`e${i}`, "error"));
      const { list, evicted } = addToast(errs, t("e3", "error"), 0);
      expect(list).toHaveLength(4);
      expect(evicted).toEqual([]);
    });

    test("the transient budget counts only transients", () => {
      const errs = Array.from({ length: 6 }, (_, i) => t(`e${i}`, "error"));
      let list = [...errs];
      for (let i = 0; i < MAX_VISIBLE; i++) list = addToast(list, t(`i${i}`)).list;
      expect(list.filter((x) => x.kind === "error")).toHaveLength(6);
      expect(list.filter((x) => x.kind !== "error")).toHaveLength(MAX_VISIBLE);
    });
  });
});

// billz-667. A failure that recurs on a timer used to push one never-expiring
// error per tick; the user then had to click ✕ once per copy.
describe("addToast coalescing", () => {
  /** Same text every time, so pushes collide. Ids stay distinct to prove which survives. */
  const same = (id: string, kind: ToastKind = "info") => t(id, kind, "same message");

  test("an identical message bumps the newest toast instead of appending", () => {
    const { list, active } = addToast([same("a")], same("b"));
    expect(list).toHaveLength(1);
    expect(list[0].repeat).toBe(2);
    expect(active.repeat).toBe(2);
  });

  test("N identical pushes occupy one slot carrying repeat N", () => {
    let list: Toast[] = [];
    for (let i = 0; i < 5; i++) list = addToast(list, same(`i${i}`)).list;
    expect(list).toHaveLength(1);
    expect(list[0].repeat).toBe(5);
  });

  // The literal bug: errors never expire and are never evicted, so ten ticks of
  // one failing background refresh meant ten undismissable copies of one message.
  test("ten identical errors occupy one slot, not ten", () => {
    let list: Toast[] = [];
    for (let i = 0; i < 10; i++) list = addToast(list, same(`e${i}`, "error")).list;
    expect(list).toHaveLength(1);
    expect(list[0].repeat).toBe(10);
  });

  // Load-bearing twice over: the store keys its pending timer by id, and the
  // each-block in ToastHost is keyed by id (a new one would remount and replay
  // the entrance animation on every tick of a recurring failure).
  test("keeps the EXISTING toast's id", () => {
    const { list, active } = addToast([same("first")], same("second"));
    expect(list[0].id).toBe("first");
    expect(active.id).toBe("first");
  });

  test("does not mutate the existing toast object", () => {
    const original = same("a");
    const input = [original];
    const { list } = addToast(input, same("b"));
    // The bump landed on a NEW object; the one the caller still holds — and the
    // array it sits in, which is the store's $state list — are untouched.
    expect(original.repeat).toBe(1);
    expect(list[0]).not.toBe(original);
    expect(input).toHaveLength(1);
    expect(input[0]).toBe(original);
  });

  test("coalescing evicts nothing, even with the transient budget full", () => {
    let list = Array.from({ length: MAX_VISIBLE - 1 }, (_, i) => t(String(i)));
    list = addToast(list, same("newest")).list;
    expect(list).toHaveLength(MAX_VISIBLE);

    // Nothing is appended, so the transient count can't rise past the cap.
    const { list: after, evicted } = addToast(list, same("again"));
    expect(evicted).toEqual([]);
    expect(after).toHaveLength(MAX_VISIBLE);
    expect(after[MAX_VISIBLE - 1].repeat).toBe(2);
  });

  // The seam between the two branches: a coalesce leaves the stack AT the budget
  // (a repeat count doesn't make one slot count as several), so the next distinct
  // transient must evict exactly one — no more, and not zero.
  test("a distinct transient after a coalesce evicts exactly one", () => {
    let list = Array.from({ length: MAX_VISIBLE - 1 }, (_, i) => t(String(i)));
    list = addToast(list, same("newest")).list;
    list = addToast(list, same("again")).list;

    const { list: after, evicted } = addToast(list, t("z"));
    expect(ids(evicted)).toEqual(["0"]);
    expect(after).toHaveLength(MAX_VISIBLE);
    expect(ids(after)).toEqual(["1", "2", "newest", "z"]);
    // The bumped slot rode out the eviction with its count intact.
    expect(after[2].repeat).toBe(2);
  });

  test("a different message still stacks normally", () => {
    const { list } = addToast([same("a")], t("b"));
    expect(list).toHaveLength(2);
    expect(list.every((x) => x.repeat === 1)).toBe(true);
  });

  test("the same text under a different kind is a different event", () => {
    const { list } = addToast([same("a", "error")], same("b", "success"));
    expect(ids(list)).toEqual(["a", "b"]);
  });

  test("matches the NEWEST only, not any toast in the stack", () => {
    const { list } = addToast([same("a"), t("b")], same("c"));
    expect(list).toHaveLength(3);
    expect(list.map((x) => x.repeat)).toEqual([1, 1, 1]);
  });

  // A A B A is three slots: the trailing A recurred AFTER B, and folding it into
  // the earlier run would claim the two runs were contiguous.
  test("a repeat separated by another message starts a new run", () => {
    let list = addToast([], same("a1")).list;
    list = addToast(list, same("a2")).list;
    list = addToast(list, t("b")).list;
    list = addToast(list, same("a3")).list;
    expect(ids(list)).toEqual(["a1", "b", "a3"]);
    expect(list.map((x) => x.repeat)).toEqual([2, 1, 1]);
  });

  // Matching the newest is what keeps partitionToasts' promise that a just-raised
  // toast is on screen — a match deeper in the stack could bump a toast already
  // collapsed behind the overflow counter, so nothing visible would happen at all.
  test("the bumped toast holds its position and stays visible", () => {
    const older = Array.from({ length: MAX_VISIBLE }, (_, i) => t(`e${i}`, "error"));
    const seeded = addToast(older, same("newest", "error")).list;
    const { list } = addToast(seeded, same("again", "error"));
    expect(list[list.length - 1].id).toBe("newest");
    expect(ids(partitionToasts(list).visible)).toContain("newest");
  });

  // `active` is the store's timer key, so both branches get an explicit assertion.
  test("active is the bumped toast on coalesce", () => {
    const { list, active } = addToast([same("a")], same("b"));
    expect(active).toBe(list[list.length - 1]);
  });

  test("active is the new toast on append", () => {
    const fresh = t("b");
    expect(addToast([t("a")], fresh).active).toBe(fresh);
  });

  test("the first push into an empty stack appends, with repeat 1", () => {
    const fresh = same("a");
    const { list, evicted, active } = addToast([], fresh);
    expect(list).toEqual([fresh]);
    expect(evicted).toEqual([]);
    expect(active).toBe(fresh);
    expect(active.repeat).toBe(1);
  });

  test("a coalesced error is still sticky — coalescing doesn't touch dismissal policy", () => {
    const { active } = addToast([same("e", "error")], same("e2", "error"));
    expect(active.repeat).toBe(2);
    expect(isSticky(active.kind)).toBe(true);
    expect(autoDismissMs(active.kind)).toBeNull();
  });

  // The user-visible point of the whole bead.
  test("one dismiss clears the entire coalesced run", () => {
    let list = addToast([], same("a")).list;
    list = addToast(list, same("b")).list;
    list = addToast(list, same("c")).list;
    expect(list[0].repeat).toBe(3);
    expect(dismissToast(list, "a")).toEqual([]);
  });
});

describe("partitionToasts", () => {
  test("everything is visible below the cap", () => {
    const list = [t("a"), t("b")];
    const { visible, hidden } = partitionToasts(list);
    expect(ids(visible)).toEqual(["a", "b"]);
    expect(hidden).toEqual([]);
  });

  test("shows the NEWEST maxVisible; older ones collapse", () => {
    const list = Array.from({ length: 7 }, (_, i) => t(String(i), "error"));
    const { visible, hidden } = partitionToasts(list);
    expect(ids(visible)).toEqual(["3", "4", "5", "6"]);
    expect(ids(hidden)).toEqual(["0", "1", "2"]);
  });

  test("hidden preserves order (oldest first)", () => {
    const list = Array.from({ length: 6 }, (_, i) => t(String(i), "error"));
    expect(ids(partitionToasts(list).hidden)).toEqual(["0", "1"]);
  });

  test("honours an explicit maxVisible", () => {
    const list = [t("a"), t("b"), t("c")];
    expect(ids(partitionToasts(list, 1).visible)).toEqual(["c"]);
  });

  test("empty list", () => {
    expect(partitionToasts([])).toEqual({ visible: [], hidden: [] });
  });
});

describe("dismissToast", () => {
  test("removes the matching id", () => {
    expect(ids(dismissToast([t("a"), t("b")], "a"))).toEqual(["b"]);
  });

  test("unknown id is a no-op", () => {
    expect(ids(dismissToast([t("a"), t("b")], "zzz"))).toEqual(["a", "b"]);
  });

  test("unknown id hands the SAME list back (no needless $state invalidation)", () => {
    const list = [t("a"), t("b")];
    expect(dismissToast(list, "zzz")).toBe(list);
  });

  test("does not mutate the input list", () => {
    const before = [t("a"), t("b")];
    dismissToast(before, "a");
    expect(before).toHaveLength(2);
  });

  test("dismissing from an empty list is a no-op", () => {
    expect(dismissToast([], "a")).toEqual([]);
  });
});

// Needed once errors can pile up unbounded — clicking ✕ twelve times is not a
// dismissal strategy.
describe("dismissAllToasts", () => {
  test("clears everything", () => {
    expect(dismissAllToasts([t("a"), t("b", "error")])).toEqual([]);
  });

  test("empty list is already clear", () => {
    expect(dismissAllToasts([])).toEqual([]);
  });
});

describe("autoDismissMs", () => {
  test("success and info expire", () => {
    expect(autoDismissMs("success")).toBe(TOAST_MS);
    expect(autoDismissMs("info")).toBe(TOAST_MS);
  });

  // The whole point of the system: an error you never saw is the failure mode
  // toasts exist to fix, so errors stay until explicitly dismissed.
  test("errors are sticky", () => {
    expect(autoDismissMs("error")).toBeNull();
  });
});

describe("isSticky", () => {
  test("tracks autoDismissMs so the two can't drift", () => {
    expect(isSticky("error")).toBe(true);
    expect(isSticky("success")).toBe(false);
    expect(isSticky("info")).toBe(false);
  });
});

describe("isAssertive", () => {
  test("only errors interrupt the screen reader", () => {
    expect(isAssertive("error")).toBe(true);
    expect(isAssertive("success")).toBe(false);
    expect(isAssertive("info")).toBe(false);
  });
});

describe("announcementText", () => {
  test("a single occurrence announces the bare message", () => {
    expect(announcementText("Saved to the library.", 1)).toBe("Saved to the library.");
  });

  // Each repeat must produce a string DIFFERENT from the one before it: a live
  // region handed the text it already holds is not a DOM change and says nothing.
  test("repeats carry the count, so consecutive announcements differ", () => {
    expect(announcementText("boom", 2)).toBe("boom (2 times)");
    expect(announcementText("boom", 3)).toBe("boom (3 times)");
  });
});
