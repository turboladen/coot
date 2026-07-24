import { expect, test } from "bun:test";
import { connectionStatus, defaultTabConnection } from "./sidebarLogic";

// billz-a5y.5: the sidebar's per-connection status dot is a HONEST passive signal —
// no background probing. It derives purely from whether the connection is locked
// (session-only password not entered this session) and the state of its
// per-connection databases store (billz-a5y.2). `connectionStatus` is the pure seam:
//
//   locked                    → "locked"   (--warn; session password needed)
//   else loaded               → "loaded"   (--ok;   object tree fetched this session)
//   else error                → "error"    (--danger; a touched connection that failed)
//   else loading              → "loading"  (faint/transient neutral variant)
//   else idle (untouched)     → "neutral"  (--faint hollow)
//
// locked takes precedence: a locked connection never loads its store, but guard it
// explicitly so a stale "loaded" entry can never read as ready under a lock.

test("locked wins over every db status", () => {
  expect(connectionStatus({ locked: true, dbStatus: "idle" })).toBe("locked");
  expect(connectionStatus({ locked: true, dbStatus: "loading" })).toBe("locked");
  expect(connectionStatus({ locked: true, dbStatus: "loaded" })).toBe("locked");
  expect(connectionStatus({ locked: true, dbStatus: "error" })).toBe("locked");
});

test("loaded (unlocked) reads as loaded", () => {
  expect(connectionStatus({ locked: false, dbStatus: "loaded" })).toBe("loaded");
});

test("error (unlocked) reads as error — honest surfacing of a failed load", () => {
  expect(connectionStatus({ locked: false, dbStatus: "error" })).toBe("error");
});

test("loading (unlocked) reads as loading", () => {
  expect(connectionStatus({ locked: false, dbStatus: "loading" })).toBe("loading");
});

test("idle (unlocked, untouched this session) is neutral", () => {
  expect(connectionStatus({ locked: false, dbStatus: "idle" })).toBe("neutral");
});

// billz-a5y.3: a NEW tab defaults to the FOCUSED connection (the last root you
// browsed/retargeted) when it's a live connection, else the active tab's connection
// (mirrored via conns.activeId) when that's live, else null. Focus is decoupled from
// the active tab so expanding Y to browse defaults new tabs to Y WITHOUT retargeting
// the current tab. Both focus and active are validated against the live id set so a
// dangling id (a since-deleted connection) never becomes a new tab's target.

test("focus wins when it is a live connection", () => {
  expect(defaultTabConnection("focus-id", "active-id", ["focus-id", "active-id"])).toBe("focus-id");
});

test("stale focus falls back to the active connection when active is live", () => {
  expect(defaultTabConnection("gone-id", "active-id", ["active-id"])).toBe("active-id");
});

test("stale focus AND stale active → null (never target a dangling id)", () => {
  expect(defaultTabConnection("gone-id", "also-gone", ["other-id"])).toBe(null);
});

test("null focus falls back to the active connection", () => {
  expect(defaultTabConnection(null, "active-id", ["active-id"])).toBe("active-id");
});

test("null focus and null active → null", () => {
  expect(defaultTabConnection(null, null, ["some-id"])).toBe(null);
});
