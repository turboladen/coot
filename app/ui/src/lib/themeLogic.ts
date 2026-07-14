// Pure, rune-free theme helpers (billz-xhv.6) — `bun test`-able in isolation.
// The store (theme.svelte.ts) owns the live `$state` + localStorage + DOM
// stamping; this module holds only the two total functions that decide WHAT to
// stamp and how to sanitise a persisted value. Mirrors the tabsLogic.ts /
// tabs.svelte.ts split.

// The user's explicit preference. `system` defers to the OS (`prefers-color-scheme`).
export type Choice = "system" | "light" | "dark";

const CHOICES: readonly Choice[] = ["system", "light", "dark"];

// Map a choice to the `data-theme` attribute value the CSS override hooks expect
// (app.css:40-74). `system` → null means REMOVE the attribute so the
// `@media (prefers-color-scheme)` block governs; `light`/`dark` pin it.
export function resolveAttr(choice: Choice): string | null {
  return choice === "system" ? null : choice;
}

// Sanitise a persisted (thus untrusted) value: return it only if it's exactly
// one of the known choices, else fall back to `system`. Guards against a stale
// blob, a hand-edited value, or a future shape change.
export function parseChoice(raw: string | null): Choice {
  return CHOICES.includes(raw as Choice) ? (raw as Choice) : "system";
}
