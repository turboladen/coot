import { expect, test } from "bun:test";
import { databaseLoadTarget } from "./databasesLogic";

// billz-zmw: a locked (session-only, not-yet-unlocked) connection must resolve to
// `null` so the shared dbStore is CLEARED rather than left holding the previous
// connection's databases — otherwise the tree/picker render the prior server's
// schema under the locked connection (a silent wrong-target risk).

test("unlocked connection loads its own id", () => {
  expect(databaseLoadTarget("conn-a", false)).toBe("conn-a");
});

test("locked connection resolves to null so the store is cleared", () => {
  expect(databaseLoadTarget("conn-b", true)).toBeNull();
});

test("no active connection resolves to null", () => {
  expect(databaseLoadTarget(null, false)).toBeNull();
});

test("locked takes precedence even with an active id", () => {
  // The prior-connection list must never survive a switch to a locked connection.
  expect(databaseLoadTarget("conn-a", true)).toBeNull();
});

test("locked with no active connection resolves to null", () => {
  expect(databaseLoadTarget(null, true)).toBeNull();
});
