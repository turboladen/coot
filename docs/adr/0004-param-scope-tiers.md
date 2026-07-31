# ADR-0004: Parameter scope tiers — Local < Session < Global

- **Status:** Accepted
- **Date:** 2026-07-12
- **Related:** `PLAN.md` §5; [ADR-0005](0005-saved-query-is-a-stable-template.md) (what gets
  persisted); [ADR-0007](0007-design-tokens-and-colorblind-safe-theme.md) (tier badges never rely on
  hue alone); bead `billz-d28.4`

## Context

`PLAN.md` §5 calls for a parameter value to be remembered per-query, for a whole session ("set
`@cust` for the afternoon", shared across every query referencing `@cust`), or as a persisted
default such as `@today`. The first implementation shipped Local-only remembered values
(`Param.last_value` per query), which cannot express the session or default cases.

## Decision

**A parameter value resolves `Local ?? Session ?? Global`, and its scope selects where an edit is
remembered — independent of where the displayed value resolved from.**

- **Precedence and write-target are separate concepts.** `resolve(@name)` layers the three tiers;
  the scope selector chooses the write destination. A value can therefore display as inherited from
  Session while being remembered Locally, or vice versa.
- **Writes happen on Run**, consistent with the existing Local behaviour.
- **Storage differs per tier, deliberately:** Local stays `Param.last_value` in `query_store`;
  Session is in-memory for the app session; Global persists to `localStorage` under a versioned key,
  degrading to `{}` on corrupt data.
- Resolution, source attribution, and write routing live in `paramBarLogic.ts` as pure, unit-tested
  functions (`resolve`, `valueSource`, `routeWrites`). Routing a value to Session or Global also
  **clears** its Local `lastValue`, so `resolve` falls through to the tier the user chose.
- Session and Global stores are read as **untracked snapshots** when pre-filling, so a value set
  elsewhere propagates on the next tab switch rather than disruptively into a field mid-edit.
- Inherited values are badged. Per ADR-0007 the badge carries a text label as well as a hue.

**Accepted subtlety:** `valueSource` is computed from *stored* state, so a field the user has typed
into but not yet Run still shows an inherited badge. For a single-user tool this is acceptable — the
badge means "no Local value stored yet; inheriting."

## Consequences

- **Positive:** the workflow `PLAN.md` §5 describes actually works — set `@cust` once at Session
  scope and every saved query referencing it resolves without prompting.
- **Positive:** the resolution rules are pure functions, so precedence is tested exhaustively
  without a database or a browser.
- **Negative:** a parameter's value can live in any of three stores with three different lifetimes
  (query store on disk, memory, `localStorage`). "Where is this value actually kept?" requires
  knowing the scope.
- **Negative:** the badge can lag user input until Run, which is a deliberate simplification rather
  than a bug, and is documented on `valueSource`.
