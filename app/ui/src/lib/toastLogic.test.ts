// `bun test` — toastLogic.ts is rune-free plain TS, so it imports cleanly here
// with no Svelte compiler (unlike toasts.svelte.ts). Excluded from svelte-check
// via tsconfig `exclude`, same as tabsLogic.test.ts / savedQueriesLogic.test.ts.
import { describe, expect, test } from "bun:test";
import {
  addToast,
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

function t(id: string, kind: ToastKind = "info"): Toast {
  return { id, kind, text: `toast ${id}` };
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
