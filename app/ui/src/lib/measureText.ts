// Canvas text measurement + autofit width arithmetic (billz-kh3). The canvas ctx
// is created LAZILY on first measure so this module is import-safe in bun tests
// (which only exercise the pure `fitColumnWidth`, never the DOM path).

// Cell chrome added to measured content to get the column width. .th/.td use
// `padding: 4px 8px` (8 + 8 = 16) and `border-right: 1px`, box-sizing border-box.
const CELL_PADDING_X = 16;
const CELL_BORDER = 1;

let ctx: CanvasRenderingContext2D | null = null;
let ctxTried = false;

function measureCtx(): CanvasRenderingContext2D | null {
  if (ctxTried) return ctx;
  ctxTried = true;
  ctx = document.createElement("canvas").getContext("2d");
  return ctx;
}

// Width in px of `text` rendered with the CSS `font` shorthand (e.g.
// "600 13px 'IBM Plex Sans', sans-serif"). Returns 0 when canvas is unavailable
// (headless) — callers then fall back to the min width via fitColumnWidth.
export function measureTextWidth(text: string, font: string): number {
  const c = measureCtx();
  if (!c) return 0;
  c.font = font;
  return c.measureText(text).width;
}

// Pure autofit arithmetic (bun-tested): widest content + cell chrome, clamped to
// [minSize, maxCap]. maxCap keeps one huge cell from producing an absurd column;
// only autofit is capped (drag stays uncapped — the columnDef has no maxSize).
export function fitColumnWidth(contentPxs: number[], minSize: number, maxCap: number): number {
  const widest = Math.max(...contentPxs, 0);
  return Math.min(maxCap, Math.max(minSize, Math.ceil(widest + CELL_PADDING_X + CELL_BORDER)));
}
