import { expect, test } from "bun:test";
import { databaseLoadTarget } from "./databasesLogic";

// billz-zmw: a locked (session-only, not-yet-unlocked) connection must resolve to
// `null` so the shared dbStore is CLEARED rather than left holding the previous
// connection's databases — otherwise the tree/picker render the prior server's
// schema under the locked connection (a silent wrong-target risk).
//
// billz-a5y.1: the `exists` presence flag gates loading on the connection actually
// being in the list. At cold start the active id is mirrored from a persisted tab
// BEFORE the connection list loads (and a deleted/dangling id can also linger), so
// without this gate we'd fire a premature `list_databases` against an absent
// connection. `locked` is null for BOTH absent and present-ready, so presence is
// the only way to distinguish them.

test("present, unlocked connection loads its own id", () => {
  expect(databaseLoadTarget("conn-a", false, true)).toBe("conn-a");
});

test("present, locked connection resolves to null so the store is cleared", () => {
  expect(databaseLoadTarget("conn-b", true, true)).toBeNull();
});

test("no active connection resolves to null", () => {
  expect(databaseLoadTarget(null, false, false)).toBeNull();
});

test("locked takes precedence even with a present active id", () => {
  // The prior-connection list must never survive a switch to a locked connection.
  expect(databaseLoadTarget("conn-a", true, true)).toBeNull();
});

test("locked with no active connection resolves to null", () => {
  expect(databaseLoadTarget(null, true, false)).toBeNull();
});

test("absent (not-yet-loaded / deleted) connection resolves to null — no premature load", () => {
  // The id points at a connection not in the list (cold-start ordering, or a
  // dangling id after the connection was deleted). Must clear, not hit the backend.
  expect(databaseLoadTarget("conn-x", false, false)).toBeNull();
});
