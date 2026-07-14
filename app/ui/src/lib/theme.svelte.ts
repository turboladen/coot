// Color-theme store (billz-xhv.6) — the user's explicit light/dark/system
// preference, persisted to localStorage and stamped onto <html> as `data-theme`
// so the CSS override hooks in app.css (the `[data-theme="…"]` / `:not(...)`
// blocks) take effect. Default is `system` (follow the OS via
// `prefers-color-scheme`), which is the app's behaviour before this feature.
//
// Mirrors tabs.svelte.ts / databases.svelte.ts: a `.svelte.ts` module can hold
// `$state` but NOT `$effect` (no effect root at module scope), so persistence
// and the DOM side-effect are IMPERATIVE — `setTheme` mutates, stamps, and
// saves in one shot. Mutate the exported `$state` object's field in place; never
// reassign the export.
import { type Choice, parseChoice, resolveAttr } from "./themeLogic";

export type { Choice } from "./themeLogic";

// Versioned key so a future shape change can migrate/reset cleanly.
const STORAGE_KEY = "billz.theme.v1";

// --- localStorage adapter (the swappable persistence seam) -------------------
// Both sides swallow errors and degrade: a corrupt/absent value must never brick
// the app. On failure we fall back to `system`, which the media query still
// handles, so the app stays usable (just unthemed by preference).
function loadChoice(): Choice {
  try {
    return parseChoice(localStorage.getItem(STORAGE_KEY));
  } catch (e) {
    console.warn("billz: failed to load theme from localStorage", e);
    return "system";
  }
}

function saveChoice(choice: Choice): void {
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch (e) {
    console.warn("billz: failed to save theme to localStorage", e);
  }
}

// Stamp (or clear) the `data-theme` attribute on <html>. `system` → remove it so
// the OS preference governs; `light`/`dark` → pin it.
function applyTheme(choice: Choice): void {
  const attr = resolveAttr(choice);
  if (attr === null) {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", attr);
  }
}

export const theme = $state<{ choice: Choice }>({ choice: loadChoice() });

// Flash-kill: stamp the persisted choice at module-eval time (before mount()
// paints in main.ts) so first paint already matches the saved preference — no
// flash-of-wrong-theme even when the choice contradicts the OS. `system` is a
// no-op (removes an attribute that was never set), which is correct.
applyTheme(theme.choice);

// The single mutation entry point (the segmented control's click handler):
// update state, stamp the DOM, persist — all synchronously.
export function setTheme(choice: Choice): void {
  theme.choice = choice;
  applyTheme(choice);
  saveChoice(choice);
}
