# Variables Library — design

**Date:** 2026-07-17
**Status:** approved (brainstorm) → ready for implementation plan
**Scope:** frontend + localStorage only. No Rust / Tauri / driver changes.

## Problem

The existing `@param` feature is powerful but **undiscoverable and inconsistent**:

- The param bar renders **only when the current tab is a linked saved query**
  (`App.svelte`: `curParams = curSavedQuery ? deriveParams(...) : []`). Typing `@foo`
  in a scratch tab does nothing — the query runs the plain `run_sql` path. This is the
  entire "sometimes `@foo` is recognized, sometimes not" mystery.
- Reusable values exist (the **Global** tier, persisted in `localStorage`, keyed by
  `@name`) but are hidden behind a per-param **scope dropdown** (`Local / Session /
  Global`) on a bar you rarely see. Easy to forget the feature exists at all.
- There is no place to *see, edit, or browse* reusable values.

The user thinks in terms of **named reusable values** ("benchmark_user = 12345", a
vendor name that is annoying to retype), not per-param scope tiers. The goal is a
first-class, browsable **Variables Library** that resolves anywhere.

## Goals (this spec)

**V2 — Variables Library.** A persisted, named, browsable, click-to-insert set of
reusable values that resolve into `@name` references in **any** tab. This is the
headline deliverable.

## Non-goals (explicitly deferred — file as beads, do not build)

- **Autocomplete:** `@`-triggered CodeMirror completion over library variables +
  in-scope tokens. The single biggest discoverability lever, cleanly separable.
- **V3 — local script variables:** inline `@x = 123` declarations scoped to one script.
- **V1 — context-matrix variables:** one label whose value differs per
  database/server, auto-resolved by execution context. Shelved until the user has
  lived with V2; the data model below must not preclude it (a variable stays a
  first-class entity that could later grow a context→value map).
- **V4 — dynamic values:** splice in the result of another saved query.
- **Per-query override** of a library value ("shadow to local for this run"). See the
  deliberate behavior change below.

## Concepts & data model

Two first-class concepts replace today's one-bar-three-scopes model.

### Variable (library entry)

```ts
type Variable = {
  name: string;             // identity == token minus @; identifier-safe, unique. "benchmark_user"
  value: string;
  sqlType: SqlType | null;  // bind type; null = raw-text (flagged unsafe)
  note: string;             // optional memory aid ("the QA benchmark account"); decoration only, not identity
};
```

- Stored as an **ordered array** under a new localStorage key `coot.variables.v1`,
  with a `name → Variable` index built at load for O(1) resolution.
- **Name is the identity** (mirrors today's Global map). Renaming is delete + recreate;
  you update `@references` yourself, same as renaming a SQL column. No separate stable
  id — YAGNI for a single-user tool.
- `name` must match `^[A-Za-z_]\w*$` and be unique; collisions and invalid names are
  rejected at save time.

### Query input

A `@name` in the current query that is **not** a library variable. A value the query
needs, supplied per-run; for a saved query its last value is remembered. This is
today's `Param` with `lastValue`, minus the scope machinery.

### The one rule

> **A `@name` resolves from the library if a variable of that name exists; otherwise it
> is a query input.**

Scope is **implied by name**, never chosen. The `Local / Session / Global` dropdown and
the **Session tier** are removed. `Param.scope` remains in the stored JSON as an
ignored, vestigial field (backward-compatible), deletable in a later cleanup.

## Resolution

For every `@name` the editor finds in the current tab — in **any** tab now; the
saved-query gate is removed:

1. **Library match** → binds automatically to the variable's value. Shown in the param
   bar as a read-only chip: `@vendor → "The best…"  ᴸᴵᴮ`. Value is edited in the
   Variables panel, not inline.
2. **No match** → an editable query-input field (+ bind-type control). Remembers its
   last value if the tab is a saved query; ephemeral (this-run-only) in a scratch tab.

### Deliberate behavior change: library always wins

Today a `Local` value can override a `Global` of the same name for one query. In the
new model a library `@name` **always wins** — no per-query shadowing in v1. Approved as
the right simplicity trade (the user had forgotten the tiers existed and was not relying
on override). "Override a library value to a one-off local for this run" is a clean
follow-on bead if it is ever missed.

## UI surfaces

### A. Variables panel

A third sidebar mode: **Objects | Library | Variables** (`sidebarMode` gains
`"variables"`; new `VariablesLibrary.svelte` mirrors `SavedQueryLibrary.svelte`). Each
row:

```
 benchmark_user   12345         [↧ insert] [✎] [🗑]
 vendor           "The best…"   [↧ insert] [✎] [🗑]
```

- **↧ insert** drops `@benchmark_user` at the editor cursor in the active tab.
- **✎** edits name / value / bind-type / note inline (same reveal pattern as
  `SavedQueryLibrary`'s promote row / `ConnectionForm` fields — `window.prompt` is
  unreliable in the Tauri v2 WKWebView).
- **＋ New variable** row at top; name validated identifier-safe, collisions rejected.

### B. Param bar reframe (`ParamBar.svelte`)

Rendered in **every** tab (gate removed). Per `@name`:

- **library-matched** → read-only chip `@vendor → "The best…" ᴸᴵᴮ`; click opens its
  Variables row. No value field, no scope dropdown.
- **query input** → editable field + bind-type control (today's field, minus the scope
  `<select>`).
- No `@names` (or none unmatched) → bar collapses, as today.

## Safety & bind types

Unchanged engine, reused end-to-end. A variable's `sqlType` defaults by **inference on
save**: value parses as an integer → `int`, else → `nvarchar`, so it rides the safe
`sp_executesql` bind path by default. `raw-text` (`null`) stays the explicit,
`raw!`-flagged opt-in for "splice an identifier/snippet" cases (e.g. a table name).
`param_bind.rs` / `run_params` are untouched — `run_params` already accepts
`ResolvedParam[] {name, sqlType, value}`; V2 only changes where `value` comes from.

## Migration (additive, rollback-safe)

- **`coot.globalParams.v1`** (`Record<string,string>`) → one-time seed into
  `coot.variables.v1`: each `@name→value` becomes `{name, value, sqlType: inferred,
  note: ""}`. Old key left untouched (rollback-safe).
- **Session values** — in-memory only; nothing to migrate (they evaporate on restart
  today).
- **Saved-query `Param.scope`** — becomes ignored. `global`-scoped params are covered by
  the migrated library; `local`/`session` become plain query inputs (`local` keeps its
  `lastValue`). The field stays in stored JSON, unread, removable later.

## Affected files (indicative, not binding)

- **new** `app/ui/src/lib/variables.svelte.ts` — `coot.variables.v1` store (load / add /
  update / remove / persist), migration from `globalParams`.
- **new** `app/ui/src/lib/variablesLogic.ts` — pure, rune-free: name validation, type
  inference, migration mapping, library-vs-input classification, resolution.
- **new** `app/ui/src/lib/variablesLogic.test.ts` — unit tests (the
  `paramBarLogic.test.ts` pattern).
- **new** `app/ui/src/lib/VariablesLibrary.svelte` — the panel.
- **edit** `app/ui/src/lib/ParamBar.svelte` — chip vs input rendering; drop scope
  `<select>`.
- **edit** `app/ui/src/lib/paramBarLogic.ts` — resolution/classification against the
  library; retire `resolve`/`valueSource`/`routeWrites` scope-tier logic (or reduce to
  library-vs-input).
- **edit** `app/ui/src/App.svelte` — remove the `curSavedQuery` gate on `curParams`
  (derive in every tab); add `"variables"` sidebar mode; wire click-to-insert;
  build `ResolvedParam[]` from library + query inputs.
- **retire** `globalParams.svelte.ts` / `sessionParams.svelte.ts` usage (kept only for
  the one-time migration read of `globalParams`).

## Testing

- Pure-logic units in `*.test.ts`: resolution (library-match vs query-input), migration
  mapping, name validation, type inference, insert-token building.
- Visual verify **light + dark** (vite dev + Playwright screenshots): the new Variables
  panel and the reframed param bar (chip vs input, collapse when empty).
- `just verify` green (Rust untouched but the gate still runs).

## Follow-on beads to file

1. `@`-triggered CodeMirror autocomplete over library variables + in-scope tokens.
2. V3 — inline `@x = 123` local script variables.
3. V1 — context-matrix variables (per-database/server values).
4. Per-query override of a library value ("shadow to local for this run"), if missed.
5. Cleanup — delete the vestigial `Param.scope` field and `sessionParams` module once
   migration has settled.
