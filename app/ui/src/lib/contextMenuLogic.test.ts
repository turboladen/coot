import { expect, test } from "bun:test";
import { clampMenuPosition } from "./contextMenuLogic";

// A comfortably large viewport where an unclamped menu fits with room to spare.
const BIG = { menuW: 176, menuH: 88, viewportW: 1000, viewportH: 800, margin: 8 };

test("clampMenuPosition: a menu that fits passes through unchanged", () => {
  expect(clampMenuPosition({ x: 100, y: 100, ...BIG })).toEqual({ x: 100, y: 100 });
});

test("clampMenuPosition: overflowing the right edge shifts left to fit", () => {
  // 950 + 176 = 1126 > 1000 - 8 → left edge pulled to (992 - 176).
  expect(clampMenuPosition({ x: 950, y: 100, ...BIG })).toEqual({ x: 992 - 176, y: 100 });
});

test("clampMenuPosition: overflowing the bottom edge flips the menu up above y", () => {
  // 780 + 88 = 868 > 800 - 8 → top flips to (y - menuH), opening upward.
  expect(clampMenuPosition({ x: 100, y: 780, ...BIG })).toEqual({ x: 100, y: 780 - 88 });
});

test("clampMenuPosition: an anchor past the left margin is pushed back to it", () => {
  // A negative preferred x (anchor's right - menuW went off the left edge) clamps
  // to the left margin rather than off-screen.
  expect(clampMenuPosition({ x: -40, y: 100, ...BIG })).toEqual({ x: 8, y: 100 });
});

test("clampMenuPosition: a viewport smaller than the menu degrades to the top-left margin", () => {
  // Both axes overflow AND the flip/shift lands past the near edge → clamped to margin.
  expect(
    clampMenuPosition({ x: 50, y: 50, menuW: 176, menuH: 88, viewportW: 100, viewportH: 100, margin: 8 }),
  ).toEqual({ x: 8, y: 8 });
});

test("clampMenuPosition: margin defaults to 8 when omitted", () => {
  expect(
    clampMenuPosition({ x: 999, y: 100, menuW: 176, menuH: 88, viewportW: 1000, viewportH: 800 }),
  ).toEqual({ x: 1000 - 8 - 176, y: 100 });
});

// billz-a5y.8 nit#4: when the menu is anchored to a trigger button (the ⋯ button),
// a bottom overflow must flip the menu ABOVE the trigger — its bottom edge at the
// trigger's TOP — not above y (the trigger's bottom), which would overlap the button.
test("clampMenuPosition: a bottom-overflow flip anchors above anchorTop when given", () => {
  // Trigger rect top=760, bottom=780. y=780 (r.bottom) overflows → flip using
  // anchorTop=760 so the menu's bottom lands at 760 (the trigger's top), clearing it.
  expect(
    clampMenuPosition({ x: 100, y: 780, anchorTop: 760, ...BIG }),
  ).toEqual({ x: 100, y: 760 - 88 });
});

test("clampMenuPosition: anchorTop is ignored when the menu fits below y", () => {
  // No bottom overflow → drops down from y regardless of anchorTop.
  expect(clampMenuPosition({ x: 100, y: 100, anchorTop: 80, ...BIG })).toEqual({ x: 100, y: 100 });
});

test("clampMenuPosition: without anchorTop a bottom-overflow flip still uses y - menuH", () => {
  // Regression guard for the cursor path (right-click), which passes no anchorTop.
  expect(clampMenuPosition({ x: 100, y: 780, ...BIG })).toEqual({ x: 100, y: 780 - 88 });
});
