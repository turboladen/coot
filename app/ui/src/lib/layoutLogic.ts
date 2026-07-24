// Pure, rune-free layout helpers (billz-a5y.6) — `bun test`-able in isolation.
// The store (layout.svelte.ts) owns the live `$state` + localStorage; this module
// holds only the total functions that clamp a width and sanitise a persisted blob.
// Mirrors the themeLogic.ts / theme.svelte.ts split.

// Right-panel (saved-query Library) width bounds, in CSS px. The panel can be
// dragged/keyboard-resized between MIN and MAX; DEFAULT is the first-launch width.
// Note: the RENDER width is additionally capped in CSS (`min(--lib-width, 100vw-…)`)
// so a stored value never overflows a small window — these bounds govern the
// stored number and the drag range, not the on-screen fit.
export const MIN_LIBRARY_WIDTH = 220;
export const MAX_LIBRARY_WIDTH = 640;
export const DEFAULT_LIBRARY_WIDTH = 320;

export interface LayoutState {
  libraryOpen: boolean;
  libraryWidth: number;
}

// Clamp a width to [MIN, MAX]. A non-number or non-finite input (NaN / ±Infinity,
// or an untrusted JSON field that isn't a number) falls back to DEFAULT — so
// callers (drag handlers, parseLayout) can pass raw values without pre-checking.
export function clampWidth(px: number): number {
  if (typeof px !== "number" || !Number.isFinite(px)) return DEFAULT_LIBRARY_WIDTH;
  return Math.min(MAX_LIBRARY_WIDTH, Math.max(MIN_LIBRARY_WIDTH, px));
}

// Sanitise a persisted (thus untrusted) blob into a total LayoutState. Catches the
// JSON.parse throw INTERNALLY so null / corrupt / partial / wrong-type / non-object
// all resolve to a valid state (the store's own try/catch is belt-and-suspenders,
// not the guard the unit test exercises). Field-by-field: `libraryOpen` strict
// `=== true` (anything else → false); `libraryWidth` through clampWidth.
export function parseLayout(raw: string | null): LayoutState {
  if (raw === null) return { libraryOpen: false, libraryWidth: DEFAULT_LIBRARY_WIDTH };
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { libraryOpen: false, libraryWidth: DEFAULT_LIBRARY_WIDTH };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { libraryOpen: false, libraryWidth: DEFAULT_LIBRARY_WIDTH };
  }
  const obj = parsed as Record<string, unknown>;
  return {
    libraryOpen: obj.libraryOpen === true,
    libraryWidth: clampWidth(obj.libraryWidth as number),
  };
}
