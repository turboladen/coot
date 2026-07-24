// `bun test` — toastLogic.ts is rune-free plain TS, so it imports cleanly here
// with no Svelte compiler (unlike toasts.svelte.ts). Excluded from svelte-check
// via tsconfig `exclude`, same as tabsLogic.test.ts / savedQueriesLogic.test.ts.
import { describe, expect, test } from "bun:test";
import {
  addToast,
  autoDismissMs,
  dismissToast,
  isAssertive,
  MAX_TOASTS,
  type Toast,
  type ToastKind,
  TOAST_MS,
} from "./toastLogic";

function t(id: string, kind: ToastKind = "info"): Toast {
  return { id, kind, text: `toast ${id}` };
}

describe("addToast", () => {
  test("appends to the end (newest last)", () => {
    const { list } = addToast([t("a")], t("b"));
    expect(list.map((x) => x.id)).toEqual(["a", "b"]);
  });

  test("does not mutate the input list", () => {
    const before = [t("a")];
    addToast(before, t("b"));
    expect(before.map((x) => x.id)).toEqual(["a"]);
  });

  test("nothing evicted below the cap", () => {
    expect(addToast([t("a")], t("b")).evicted).toEqual([]);
  });

  test("evicts the OLDEST once over the cap", () => {
    const full = Array.from({ length: MAX_TOASTS }, (_, i) => t(String(i)));
    const { list, evicted } = addToast(full, t("new"));
    expect(list).toHaveLength(MAX_TOASTS);
    expect(list.map((x) => x.id)).toEqual(["1", "2", "3", "new"]);
    // The store clears timers for whatever comes back here — an evicted toast's
    // pending setTimeout must not survive to dismiss an unrelated later toast.
    expect(evicted.map((x) => x.id)).toEqual(["0"]);
  });

  test("evicts however many are needed when the list starts over the cap", () => {
    const over = Array.from({ length: MAX_TOASTS + 2 }, (_, i) => t(String(i)));
    const { list, evicted } = addToast(over, t("new"));
    expect(list).toHaveLength(MAX_TOASTS);
    expect(evicted.map((x) => x.id)).toEqual(["0", "1", "2"]);
  });

  test("honours an explicit max", () => {
    const { list, evicted } = addToast([t("a"), t("b")], t("c"), 2);
    expect(list.map((x) => x.id)).toEqual(["b", "c"]);
    expect(evicted.map((x) => x.id)).toEqual(["a"]);
  });
});

describe("dismissToast", () => {
  test("removes the matching id", () => {
    expect(dismissToast([t("a"), t("b")], "a").map((x) => x.id)).toEqual(["b"]);
  });

  test("unknown id is a no-op", () => {
    const list = [t("a"), t("b")];
    expect(dismissToast(list, "zzz").map((x) => x.id)).toEqual(["a", "b"]);
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

describe("isAssertive", () => {
  test("only errors interrupt the screen reader", () => {
    expect(isAssertive("error")).toBe(true);
    expect(isAssertive("success")).toBe(false);
    expect(isAssertive("info")).toBe(false);
  });
});
