// Pure, rune-free viewport-clamp helper for the connection-row context menu
// (billz-a5y.4). The menu is `position: fixed`; a raw cursor point (right-click)
// or an anchored point (the ⋯ button's rect.bottom) can push the fixed box off
// screen — rows near the sidebar's right edge or the bottom of a long connection
// list. This keeps the whole box on screen. Dimensions are passed in (estimated
// constants at the call site) so this stays a total function `bun test` can drive
// without a DOM. Mirrors the layoutLogic.ts / clampWidth split.

export interface MenuPositionInput {
  /** Preferred left edge (cursor clientX, or the anchor's left). */
  x: number;
  /** Preferred top edge (cursor clientY, or the anchor's bottom for a drop-down). */
  y: number;
  /** Estimated menu width in CSS px. */
  menuW: number;
  /** Estimated menu height in CSS px. */
  menuH: number;
  /** Viewport width (window.innerWidth). */
  viewportW: number;
  /** Viewport height (window.innerHeight). */
  viewportH: number;
  /** Gap kept between the menu and each viewport edge. Defaults to 8. */
  margin?: number;
  /**
   * Alternate top edge to flip UP from when the menu overflows the bottom
   * (billz-a5y.8 nit#4). For a trigger-anchored drop-down (the ⋯ button), `y` is
   * the trigger's BOTTOM; flipping up from `y` would land the menu's bottom at the
   * trigger's bottom, overlapping it. Pass the trigger's TOP here so the flipped
   * menu sits fully above the trigger. Omit for the cursor path (right-click),
   * where flipping up from `y` (the cursor point) is correct.
   */
  anchorTop?: number;
}

// Clamp a preferred top-left so the whole menu stays within the viewport.
//   Horizontal: prefer opening rightward from `x`; if the right edge would
//     overflow, shift the box left to fit; never past the left margin.
//   Vertical: prefer dropping down from `y`; if the bottom edge would overflow,
//     flip the box UP so it opens above the point — anchored at `anchorTop` when
//     given (the trigger's top, so a drop-down clears its button), else at `y`
//     (the cursor point); never past the top margin.
// When the viewport is smaller than the menu, both axes bottom out at `margin`
// (top-left), which is the least-bad degenerate placement.
export function clampMenuPosition({
  x,
  y,
  menuW,
  menuH,
  viewportW,
  viewportH,
  margin = 8,
  anchorTop,
}: MenuPositionInput): { x: number; y: number } {
  let cx = x;
  if (cx + menuW > viewportW - margin) cx = viewportW - margin - menuW;
  if (cx < margin) cx = margin;

  let cy = y;
  if (cy + menuH > viewportH - margin) cy = (anchorTop ?? y) - menuH;
  if (cy < margin) cy = margin;

  return { x: cx, y: cy };
}
