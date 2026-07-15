# Tree-row selection highlight (billz-a8a)

**Date:** 2026-07-15
**Bead:** billz-a8a (P3, follow-up to the UI refresh billz-xhv)

## Context

The UI-refresh design (billz-xhv) called for a "selected row" highlight in the object tree — an
accent-tinted background on the active node, per the capstone mockup — but the tree has no selection
concept today. It supports only expand/collapse. This was deferred from the refresh (a
presentational pass) because it needs new state, which was out of scope then.

Two facts discovered while scoping, both correcting the bead's framing:

1. The four node types are **not** uniformly interactive. `DatabaseNode` and `TableNode` render
   `<button class="row">` whose click toggles expand/collapse; `ViewNode` and `ColumnLeaf` render an
   inert `<li>` with **no** click handler.
2. There is **no `.selected` CSS anywhere yet** — the bead says "Phase B already anticipated" it, but
   only the accent tokens (`--accent`, `--accent-press`) exist. This is net-new state *and* net-new
   styling.

**Outcome:** clicking any tree row (all four node types) highlights it as the selected node, giving
the tree the visual "where am I" feedback the refresh mockup intended.

## Scope decision

Selection applies to **all four node types** (databases, tables, views, columns) — chosen over a
minimal "DB + table only" variant, because a highlight that silently skips leaves feels arbitrary.
The cost is converting `ViewNode` / `ColumnLeaf` from inert `<li>`s into interactive elements, which
also gives them keyboard access (a genuine a11y improvement).

## Architecture

### 1. Selection store — `app/ui/src/lib/tree/selection.svelte.ts` (new)

A module-level `$state` holding the selected node's path key. **Ephemeral only — not persisted to
localStorage** (selection is transient focus, unlike column widths). A module store is the idiomatic
choice here: the tree is deeply nested (`ObjectTree → DatabaseNode → TableNode → ColumnLeaf`) and
`ObjectTree` already avoids prop-drilling, so threading a setter through every level is the pattern
we explicitly want to avoid. Mirrors `columnWidths.svelte.ts` / `globalParams.svelte.ts`.

```ts
const state = $state<{ key: string | null }>({ key: null });
export const selection = state;
export function selectNode(key: string): void {
  state.key = key;
}
```

### 2. Pure key helper — `app/ui/src/lib/tree/treeKey.ts` (new, bun-testable)

Composes a hierarchical path key from the parent's key plus this node's segment. Mirrors the pure
helper convention (`signatureOf`, `columnLabel`) — no Svelte / DOM, so it is unit-testable.

```ts
export function childKey(parentKey: string, kind: "db" | "table" | "view" | "col", name: string): string {
  return `${parentKey}/${kind}:${name}`;
}
```

Key composition per node type (the root parent is the **connection id**, already available as the
`id` prop on `DatabaseNode` / `TableNode`):

| Node          | Key                                                        |
| ------------- | ---------------------------------------------------------- |
| `DatabaseNode`| `childKey(id, "db", database.name)`                        |
| `TableNode`   | `childKey(parentKey, "table", schema + "." + name)`        |
| `ViewNode`    | `childKey(parentKey, "view", schema + "." + name)`         |
| `ColumnLeaf`  | `childKey(parentKey, "col", column.name)`                  |

Prefixing every key with the connection id means a stale selection from another connection **never
matches** a rendered row — so no explicit "clear selection on connection switch" lifecycle wiring is
needed; the highlight simply doesn't appear. (Switching back to the original connection re-highlights
the remembered node, which is acceptable / mildly nice.)

### 3. Node wiring

Each node receives its parent's key as a `parentKey` prop and computes its own key once.

- **`DatabaseNode` / `TableNode`** (already `<button class="row">`): add
  `class:tree-selected={selection.key === key}`, and have the existing `onclick` **also** call
  `selectNode(key)`. Clicking an expandable row therefore **selects *and* toggles** on the same click
  (approved) — no existing behavior removed. Each threads `parentKey={key}` to its children. Disabled
  offline DB rows remain non-selectable (they are already `disabled`).
- **`ViewNode` / `ColumnLeaf`** (currently inert `<li>`): wrap the leaf content in a
  `<button class="leaf">` inside the `<li>`, wired to `selectNode(key)`, carrying the same global
  button reset (`justify-content: flex-start`, etc.) the `.row` buttons use so layout is unchanged.
  `ViewNode` gains a `parentKey` prop (it already receives `view`); `ColumnLeaf` gains `parentKey`
  (it already receives `column`). This is the a11y upgrade from the all-nodes scope — leaves become
  Tab-focusable and Enter/Space-activatable.

### 4. Styling — one global rule in `app.css`

A single `.tree-selected` class defines the selected appearance so all four node types share one
definition and cannot drift, applied to each node's root row/leaf element:

```css
.tree-selected,
.tree-selected:hover {
  background: color-mix(in srgb, var(--accent) 14%, transparent);
}
/* accent-press on the row/leaf label text (exact child selectors settled in the plan) */
```

Accent-tinted background + accent-press text per the bead. The global focus-visible outline already
covers keyboard focus, so a focused-but-not-selected row still reads correctly.

## Out of scope (YAGNI)

- Arrow-key tree navigation (up/down/left/right). Selection is click / Enter-driven only. File a
  follow-up bead if wanted later.
- Multi-select.
- Selection persistence across app restarts.
- View → column expansion (unchanged; a pre-existing v1 seam).

## Testing / verification

- **Unit** (`treeKey.test.ts`, bun): `childKey` composition, uniqueness across kinds and across
  parents (two DBs with the same `schema.table` produce distinct keys), stability.
- **Gates**: `just ui-test`, `just ui-check`, `just verify` all green (warnings-as-errors).
- **Visual** (standing order — reviewers cannot see rendering): drive the app (`just dev`), expand
  the tree, click each node type, and screenshot the highlight in **both light and dark** themes;
  confirm the accent tint reads correctly against hover and against the row's normal state.
