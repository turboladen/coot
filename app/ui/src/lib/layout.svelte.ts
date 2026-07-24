// Workspace-layout store (billz-a5y.6) — the saved-query Library's right-panel
// open/closed state + width, persisted to localStorage so both survive a relaunch.
//
// Mirrors theme.svelte.ts: a `.svelte.ts` module can hold `$state` but NOT
// `$effect` (no effect root at module scope), so persistence is IMPERATIVE — the
// setters mutate the exported `$state` object in place and save synchronously.
// Eager `load()` at module eval means the panel paints in its persisted state on
// first render (no flash, no restore effect). Never reassign the export.
import { type LayoutState, clampWidth, parseLayout } from "./layoutLogic";

// Versioned key so a future shape change can migrate/reset cleanly.
const STORAGE_KEY = "coot.layout.v1";

// A floor the workspace + sidebar must keep when the panel is dragged wide, so a
// persisted width can't crowd them out. Mirrors the CSS cap's reserve
// (`calc(100vw - 20rem - 3rem)` = 368px at a 16px root: 20rem sidebar + 3rem
// workspace floor). The CSS `min(--lib-width, …)` is what actually prevents
// overflow at render time; this is only the persist-time guard on the drag input.
// setLibraryWidth applies clampWidth AFTER this reserve, so the stored width stays
// within [MIN, MAX] regardless of window size.
const VIEWPORT_RESERVE = 368;

function load(): LayoutState {
  try {
    return parseLayout(localStorage.getItem(STORAGE_KEY));
  } catch (e) {
    console.warn("coot: failed to load layout from localStorage", e);
    return parseLayout(null);
  }
}

function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
  } catch (e) {
    console.warn("coot: failed to persist layout to localStorage", e);
  }
}

export const layout = $state<LayoutState>(load());

// Open/collapse the Library panel and persist — the collapse/expand button's
// click handler.
export function setLibraryOpen(open: boolean): void {
  layout.libraryOpen = open;
  persist();
}

// Set the panel width and persist — the drag/keyboard-resize handlers. The
// viewport reserve (Math.min) trims a request that would crowd the sidebar +
// workspace, then clampWidth runs LAST as the final [MIN, MAX] floor/ceiling —
// so the STORED width is always ≥ MIN (a sub-MIN value is never persisted, even
// on a narrow window) and no caller can bypass the bounds with a huge value.
export function setLibraryWidth(px: number): void {
  const capped = Math.min(px, window.innerWidth - VIEWPORT_RESERVE);
  layout.libraryWidth = clampWidth(capped);
  persist();
}
