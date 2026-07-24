import { expect, test } from "bun:test";
import { databaseLoadAction } from "./databasesLogic";

// billz-a5y.2: the object store is now keyed PER CONNECTION (a SvelteMap), so the
// load decision must name the connection it targets — a locked connection clears
// ITS OWN entry, not a single shared store. `databaseLoadAction` is the pure seam:
//
//   present & unlocked → { kind: "load",  connectionId }  → ensureDatabases(id)
//   present & locked   → { kind: "clear", connectionId }  → clearDatabases(id)  (billz-zmw)
//   absent / null      → { kind: "noop" }                 → nothing
//
// billz-zmw: a locked (session-only, not-yet-unlocked) connection must never hit
// the DB, but must still clear its own entry to empty so the tree/picker don't
// render the previous server's schema under it (silent wrong-target risk). Clear
// targets the connection id so a slow in-flight load for it is dropped (token bump).
//
// billz-a5y.1: `exists` presence-gates loading — the active id is mirrored from a
// persisted tab BEFORE the connection list loads, and can also linger as a dangling
// id after its connection is deleted. Both resolve to `noop`: there is no shared
// store to clear (each reader reads its own connection's entry, which is idle-empty
// when absent), so we neither load nor clear. `locked` is null for BOTH an absent
// and a present-ready connection, so presence is the only signal distinguishing them.

test("present, unlocked connection loads its own id", () => {
  expect(databaseLoadAction("conn-a", false, true)).toEqual({ kind: "load", connectionId: "conn-a" });
});

test("present, locked connection clears its own entry (never hits DB)", () => {
  expect(databaseLoadAction("conn-b", true, true)).toEqual({ kind: "clear", connectionId: "conn-b" });
});

test("no active connection is a noop", () => {
  expect(databaseLoadAction(null, false, false)).toEqual({ kind: "noop" });
});

test("locked takes precedence over present — clears rather than loads", () => {
  // The prior-connection list must never survive a switch to a locked connection;
  // clearing its own entry (not loading) is how billz-zmw enforces that.
  expect(databaseLoadAction("conn-a", true, true)).toEqual({ kind: "clear", connectionId: "conn-a" });
});

test("locked with no active connection is a noop", () => {
  expect(databaseLoadAction(null, true, false)).toEqual({ kind: "noop" });
});

test("absent (not-yet-loaded / dangling) connection is a noop — no premature load", () => {
  // The id points at a connection not in the list (cold-start ordering, or a
  // dangling id after the connection was deleted). Per-connection entries mean
  // there's nothing stale to clear, so neither load nor clear — just noop.
  expect(databaseLoadAction("conn-x", false, false)).toEqual({ kind: "noop" });
});
