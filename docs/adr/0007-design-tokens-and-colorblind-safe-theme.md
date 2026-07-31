# ADR-0007: Design tokens and a colorblind-safe theme

- **Status:** Accepted
- **Date:** 2026-07-13
- **Related:** `PLAN.md` §2 (locked stack — no CSS framework);
  [ADR-0004](0004-param-scope-tiers.md) (tier badges); epic `billz-xhv`

## Context

The UI worked but looked vanilla: bare `system-ui`, hardcoded `#ccc` borders, no design system, and
no coherent way to add dark mode. Styling decisions were being made per-component, so every new
component was a fresh opportunity to hardcode a colour that would later break theming.

Separately, and non-negotiably: the sole user has a red/green colour deficiency. Any convention that
carries meaning in hue alone is not merely imperfect for this project — it is unreadable.

## Decision

**A single token layer in `app/ui/src/app.css`, and colourblind-safety as a hard constraint on every
colour choice.**

Tokens:

- Light values on `:root`; dark via **both** `@media (prefers-color-scheme: dark)` and
  `:root[data-theme="dark"]`, with the attribute winning so an explicit toggle can override the OS.
- **Components reference `var(--…)` only — no raw hex anywhere.** This is the enforceable rule; a
  grep for `#` inside `.svelte` `<style>` blocks catches violations.
- Self-hosted IBM Plex Sans and IBM Plex Mono via `@fontsource` — **no CDN**, because the Tauri app
  must work offline. Only the used weights are imported.
- Lucide icons at one stroke weight, inheriting `currentColor` so they theme for free.
- No CSS framework and no runtime theming library, per the locked stack: plain CSS custom properties
  plus Svelte scoped styles.
- Motion is subtle (120–150ms) and gated on `prefers-reduced-motion`.

Colourblind-safety rules, enforced in every phase:

- **State is never carried by hue alone** — pair every status with an icon or a shape change.
  Connected is a filled teal dot with a check; locked or offline is a hollow ring, not a red/green
  swap.
- **Primary action is teal, never green.**
- **Type tags are blue, never red** — red on a type tag also falsely reads as "error".
- **The semantic axis is blue/teal ↔ amber/orange**, which separates cleanly for all vision types.
- **Red is reserved for genuinely destructive actions only**, and is always paired with an icon.
- Parameter tiers differ in hue **and** text label; hue is never the only cue.

## Consequences

- **Positive:** theming is free for new components, and dark mode is one token set rather than a
  per-component effort. The manual light/dark/system toggle was deferred at design time and shipped
  later in 0.1.0 using the `data-theme` hook this established — exactly the intended payoff.
- **Positive:** the accessibility constraint is expressed as concrete, checkable rules rather than a
  goal, so it survives contact with future components.
- **Negative:** every new component must use tokens. A hardcoded hex works fine in light mode and
  silently breaks dark mode, which is the failure mode most likely to slip through.
- **Negative:** red is effectively unavailable as a status colour, so the error/warning vocabulary is
  narrower than convention assumes and leans on icons to carry meaning.
