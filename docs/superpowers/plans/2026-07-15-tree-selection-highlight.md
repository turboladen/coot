# Tree-row Selection Highlight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the object tree a selected-row highlight — clicking any node (database, table, view, or column) marks it as selected with an accent-tinted background.

**Architecture:** A module-level `$state` selection store (`selection.svelte.ts`) holds the last-clicked node's *path key*; a pure `childKey` helper composes a connection-prefixed hierarchical key per node. Each node computes its key from a `parentKey` prop, sets `class:selected` when it matches the store, and calls `selectNode(key)` on click. The two leaf types (view, column) are converted from inert `<li>`s into `<button>`s so they are clickable and keyboard-accessible. The selected *appearance* is defined once via shared `--tree-selected-*` CSS tokens in `app.css`, applied through a small scoped `.selected` rule in each node component.

**Tech Stack:** Svelte 5 (runes: `$state`, `$props`, `$derived`), TypeScript, Vite, bun test. Styling with the existing CSS custom-property design tokens.

## Global Constraints

- **bun for everything** — never npm/pnpm/yarn/node. Tests run via `just ui-test` (`cd app/ui && bun test`).
- **`cargo fmt` + `cargo clippy` clean, warnings-as-errors.** Frontend gate: `just ui-check` (svelte-check) and `just verify` (full Rust + frontend) must be green before done.
- **Store import paths omit `.ts`/`.svelte` extension conventions:** a `.svelte.ts` module is imported as `"./selection.svelte"` (see `ResultsGrid.svelte` importing `"./columnWidths.svelte"`).
- **Only pure logic files are unit-tested** (`*Logic.ts`, `columnLabel.ts`, `treeKey.ts`). `.svelte` components and `.svelte.ts` state modules are **not** unit-tested in this repo (no component-test harness); they are verified by `just ui-check` (type-check) plus visual confirmation.
- **Visual verification is mandatory for UI work** — screenshot light **and** dark themes after code review (standing order); reviewers cannot see rendering.
- **Selection is ephemeral** — in-memory only, never persisted to localStorage.
- Commit messages end with the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`. Avoid backticks in commit messages.

---

## File Structure

- **Create** `app/ui/src/lib/tree/treeKey.ts` — pure `childKey` path-key builder.
- **Create** `app/ui/src/lib/tree/treeKey.test.ts` — bun unit tests for `childKey`.
- **Create** `app/ui/src/lib/tree/selection.svelte.ts` — `$state` selection store + `selectNode`.
- **Modify** `app/ui/src/app.css` — add `--tree-selected-bg` / `--tree-selected-fg` tokens to `:root`.
- **Modify** `app/ui/src/lib/tree/DatabaseNode.svelte` — selectable row, thread `parentKey` to children, scoped `.selected` style.
- **Modify** `app/ui/src/lib/tree/TableNode.svelte` — selectable row, thread `parentKey` to `ColumnLeaf`, scoped `.selected` style.
- **Modify** `app/ui/src/lib/tree/ViewNode.svelte` — convert leaf `<li>` to selectable `<button>`, add `parentKey` prop, scoped `.selected` style.
- **Modify** `app/ui/src/lib/tree/ColumnLeaf.svelte` — convert leaf `<li>` to selectable `<button>`, add `parentKey` prop, scoped `.selected` style.

`ObjectTree.svelte` is **unchanged**: `DatabaseNode` already receives the connection id as its `id` prop, which is the root of the key path.

---

## Task 1: Pure `childKey` path-key helper (TDD)

**Files:**
- Create: `app/ui/src/lib/tree/treeKey.ts`
- Test: `app/ui/src/lib/tree/treeKey.test.ts`

**Interfaces:**
- Produces: `childKey(parentKey: string, kind: "db" | "table" | "view" | "col", name: string): string` — returns `` `${parentKey}/${kind}:${name}` ``.

- [ ] **Step 1: Write the failing test**

Create `app/ui/src/lib/tree/treeKey.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { childKey } from "./treeKey";

describe("childKey", () => {
  test("composes parent + kind + name", () => {
    expect(childKey("conn1", "db", "Sales")).toBe("conn1/db:Sales");
  });

  test("nests hierarchically", () => {
    const db = childKey("conn1", "db", "Sales");
    const tbl = childKey(db, "table", "dbo.Orders");
    expect(tbl).toBe("conn1/db:Sales/table:dbo.Orders");
  });

  test("distinguishes kinds at the same level", () => {
    const db = childKey("conn1", "db", "Sales");
    expect(childKey(db, "view", "dbo.X")).not.toBe(childKey(db, "table", "dbo.X"));
  });

  test("same table name under different DBs yields distinct keys", () => {
    const a = childKey(childKey("c", "db", "A"), "table", "dbo.T");
    const b = childKey(childKey("c", "db", "B"), "table", "dbo.T");
    expect(a).not.toBe(b);
  });

  test("same node name under different connections yields distinct keys", () => {
    expect(childKey("conn1", "db", "Sales")).not.toBe(childKey("conn2", "db", "Sales"));
  });

  test("is deterministic", () => {
    expect(childKey("c", "col", "id")).toBe(childKey("c", "col", "id"));
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/ui && bun test treeKey`
Expected: FAIL — cannot resolve module `./treeKey` / `childKey` is not defined.

- [ ] **Step 3: Write minimal implementation**

Create `app/ui/src/lib/tree/treeKey.ts`:

```ts
// Pure hierarchical path-key builder for object-tree node selection (billz-a8a).
// No Svelte / DOM — bun-testable. Each node composes its key from its parent's key
// plus its own segment, so uniqueness follows the tree hierarchy. The root parent
// is the connection id, so a selection made under one connection never matches a
// node rendered under another (no cross-connection false highlight).
export function childKey(
  parentKey: string,
  kind: "db" | "table" | "view" | "col",
  name: string,
): string {
  return `${parentKey}/${kind}:${name}`;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/ui && bun test treeKey`
Expected: PASS — 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add app/ui/src/lib/tree/treeKey.ts app/ui/src/lib/tree/treeKey.test.ts
git commit -m "feat(ui): pure childKey tree path-key helper (billz-a8a)"
```

---

## Task 2: Selection store + shared selection tokens

**Files:**
- Create: `app/ui/src/lib/tree/selection.svelte.ts`
- Modify: `app/ui/src/app.css` (inside the `:root` token block, after the `/* brand + action */` line)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `selection` — a `$state` object with a reactive `key: string | null` property (read `selection.key` in markup).
  - `selectNode(key: string): void` — sets `selection.key = key`.
  - CSS tokens `--tree-selected-bg` and `--tree-selected-fg`, resolvable on any element.

No unit test — this is a `$state` module + CSS tokens; verified by `just ui-check` and by its consumers in Tasks 3–4.

- [ ] **Step 1: Create the selection store**

Create `app/ui/src/lib/tree/selection.svelte.ts`:

```ts
// Ephemeral (in-memory) object-tree selection: the path key of the last-clicked
// node (billz-a8a). NOT persisted — selection is transient focus, unlike column
// widths. A module-level $state store avoids prop-drilling a setter through the
// deeply nested tree (ObjectTree -> DatabaseNode -> TableNode -> ColumnLeaf),
// matching the store pattern in columnWidths.svelte.ts / globalParams.svelte.ts.
const state = $state<{ key: string | null }>({ key: null });

export const selection = state;

export function selectNode(key: string): void {
  state.key = key;
}
```

- [ ] **Step 2: Add the shared selection tokens**

In `app/ui/src/app.css`, the `:root` block has this line (light theme):

```css
  --brand:#7c3aed; --accent:#0d9488; --accent-press:#0f766e; --accent-fg:#ffffff;
```

Add the two tokens on the line immediately after it:

```css
  --brand:#7c3aed; --accent:#0d9488; --accent-press:#0f766e; --accent-fg:#ffffff;
  /* tree-row selection (billz-a8a) — reference --accent/--accent-press so they
     re-resolve per theme via lazy custom-property substitution; define once. */
  --tree-selected-bg: color-mix(in srgb, var(--accent) 14%, transparent);
  --tree-selected-fg: var(--accent-press);
```

Do **not** duplicate these into the dark blocks — because they are defined in terms of `--accent` / `--accent-press`, they automatically pick up the dark values wherever they are used.

- [ ] **Step 3: Type-check**

Run: `just ui-check`
Expected: `0 ERRORS 0 WARNINGS`. (The store is not yet imported anywhere; that is fine — it just must compile.)

- [ ] **Step 4: Commit**

```bash
git add app/ui/src/lib/tree/selection.svelte.ts app/ui/src/app.css
git commit -m "feat(ui): tree selection store + shared selection tokens (billz-a8a)"
```

---

## Task 3: Make all four node types selectable

**All four node edits land in ONE task and ONE commit.** They cannot be split into
independently green-gated tasks: the `parentKey` prop *declaration* on `ViewNode` /
`ColumnLeaf` and the *threading* of `parentKey={key}` from `DatabaseNode` /
`TableNode` are mutually dependent — declare without pass (or pass without declare)
and `just ui-check` errors on a missing/unknown required prop. So the single green
`just ui-check` checkpoint comes only after every node is wired.

The db/table rows are already `<button class="row">` — same-shaped change each:
import the store + helper, derive this node's `key`, add `class:selected`, call
`selectNode(key)` on click, thread `parentKey` to children, add a scoped `.selected`
style. The view/column leaves are inert `<li>`s — wrap their content in a `<button>`
(carrying the global-button reset so layout is unchanged), add the `parentKey` prop,
derive the key, wire `selectNode` on click, add scoped hover + selected styles. Each
leaf keeps its own layout (view: `align-items: center`; column: `align-items:
baseline`, to preserve badge alignment) — only the *selected appearance* is shared
via the tokens.

**Files:**
- Modify: `app/ui/src/lib/tree/DatabaseNode.svelte`
- Modify: `app/ui/src/lib/tree/TableNode.svelte`
- Modify: `app/ui/src/lib/tree/ViewNode.svelte`
- Modify: `app/ui/src/lib/tree/ColumnLeaf.svelte`

**Interfaces:**
- Consumes: `childKey` (Task 1); `selection`, `selectNode` (Task 2); tokens `--tree-selected-bg/-fg` (Task 2).
- Produces: `TableNode`, `ViewNode`, `ColumnLeaf` each gain a required `parentKey: string` prop; `DatabaseNode` passes it to `TableNode`/`ViewNode`, `TableNode` passes it to `ColumnLeaf`.

- [ ] **Step 1: DatabaseNode — imports**

In `app/ui/src/lib/tree/DatabaseNode.svelte`, the script imports currently start with:

```ts
  import { type DatabaseInfo, listTables, listViews, type TableInfo, type ViewInfo } from "../api";
  import { ChevronDown, ChevronRight, Database } from "../icons";
  import LoadingNote from "./LoadingNote.svelte";
  import TableNode from "./TableNode.svelte";
  import ViewNode from "./ViewNode.svelte";
```

Add two imports after the `ViewNode` import:

```ts
  import ViewNode from "./ViewNode.svelte";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";
```

- [ ] **Step 2: DatabaseNode — derive the key and select on toggle**

Just below the existing `const isOnline = $derived(...)` line, add:

```ts
  const isOnline = $derived(database.stateDesc === "ONLINE");
  const key = $derived(childKey(id, "db", database.name));
```

In `toggle()`, add `selectNode(key)` as the first statement (the button is `disabled` when offline, so this only runs for online DBs, which is the desired behavior):

```ts
  async function toggle() {
    selectNode(key);
    if (!isOnline) return; // rqb.4: don't enumerate a RESTORING/OFFLINE db
    expanded = !expanded;
```

- [ ] **Step 3: DatabaseNode — markup (selected class + thread parentKey)**

Change the button opening tag to add `class:selected`:

```svelte
  <button class="row" class:selected={selection.key === key} class:muted={!isOnline} onclick={toggle} disabled={!isOnline}>
```

Change the two child render sites to pass `parentKey={key}`:

```svelte
      {#each tables as t (t.schema + "." + t.name)}
        <TableNode {id} db={database.name} table={t} parentKey={key} />
      {/each}
```

```svelte
      {#each views as v (v.schema + "." + v.name)}
        <ViewNode view={v} parentKey={key} />
      {/each}
```

- [ ] **Step 4: DatabaseNode — scoped selected style**

In the `<style>` block, immediately after the existing hover rule:

```css
  .row:not(:disabled):hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
```

add (placing after hover so it wins on hover via source order at equal specificity):

```css
  .row.selected,
  .row.selected:hover { background: var(--tree-selected-bg); }
  .row.selected .label { color: var(--tree-selected-fg); }
```

- [ ] **Step 5: TableNode — imports**

In `app/ui/src/lib/tree/TableNode.svelte`, after the existing `selectTop1000` import:

```ts
  import { selectTop1000 } from "./selectTopQuery";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";
```

- [ ] **Step 6: TableNode — add parentKey prop + derive key + select on toggle**

Change the props line to add `parentKey`:

```ts
  let { id, db, table, parentKey }: { id: string; db: string; table: TableInfo; parentKey: string } = $props();

  const key = $derived(childKey(parentKey, "table", table.schema + "." + table.name));
```

In `toggle()`, add `selectNode(key)` as the first statement:

```ts
  async function toggle() {
    selectNode(key);
    expanded = !expanded;
    if (!expanded || status !== "idle") return; // re-expand = memo, no refetch
```

- [ ] **Step 7: TableNode — markup (selected class + thread parentKey to columns)**

Change the row button opening tag:

```svelte
  <button class="row" class:selected={selection.key === key} onclick={toggle} ondblclick={openSelect} oncontextmenu={openMenu}>
```

Change the column render site:

```svelte
        {#each columns as col (col.ordinal)}
          <ColumnLeaf column={col} parentKey={key} />
        {/each}
```

- [ ] **Step 8: TableNode — scoped selected style**

After the existing hover rule:

```css
  .row:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
```

add:

```css
  .row.selected,
  .row.selected:hover { background: var(--tree-selected-bg); }
  .row.selected .label { color: var(--tree-selected-fg); }
```

(No gate/commit yet — `just ui-check` will still error until the leaves declare
`parentKey`, next. Continue.)

- [ ] **Step 9: ViewNode — full rewrite**

Replace the entire contents of `app/ui/src/lib/tree/ViewNode.svelte` with:

```svelte
<script lang="ts">
  import type { ViewInfo } from "../api";
  import { Eye } from "../icons";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";

  // v1: a view is a leaf. The AC requires table->columns, not view->columns.
  // (`list_columns` already works on views since it queries sys.columns/sys.objects
  // — leave that as a seam, don't build view expansion here.)
  let { view, parentKey }: { view: ViewInfo; parentKey: string } = $props();

  const key = $derived(childKey(parentKey, "view", view.schema + "." + view.name));
</script>

<li>
  <button class="view" class:selected={selection.key === key} onclick={() => selectNode(key)}>
    <Eye size={13} />
    <span class="label">{view.schema}.{view.name}</span>
  </button>
</li>

<style>
  li { list-style: none; }
  .view {
    display: flex;
    align-items: center;
    /* Reset the global button base's justify-content:center (app.css). */
    justify-content: flex-start;
    gap: 0.3rem;
    width: 100%;
    padding: 0.1rem 0.3rem 0.1rem 1.4rem;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    color: var(--text);
    transition: background var(--dur-fast) var(--ease);
  }
  .view :global(svg) { color: var(--muted); flex: none; }
  .view:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .view.selected,
  .view.selected:hover { background: var(--tree-selected-bg); }
  .view.selected .label { color: var(--tree-selected-fg); }
  .label { color: var(--text); }
</style>
```

- [ ] **Step 10: ColumnLeaf — full rewrite**

Replace the entire contents of `app/ui/src/lib/tree/ColumnLeaf.svelte` with:

```svelte
<script lang="ts">
  import type { ColumnInfo } from "../api";
  import { columnLabel } from "./columnLabel";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";

  let { column, parentKey }: { column: ColumnInfo; parentKey: string } = $props();

  // Pure formatter (bun-tested) -> the display pieces. Badges render only when set.
  const label = $derived(columnLabel(column));
  const key = $derived(childKey(parentKey, "col", column.name));
</script>

<li>
  <button class="col" class:selected={selection.key === key} onclick={() => selectNode(key)}>
    <span class="name">{label.name}</span>
    <span class="type">: {label.dataType}</span>
    <span class="null">{label.nullText}</span>
    {#if label.isPrimaryKey}<span class="badge pk">PK</span>{/if}
    {#if label.isForeignKey}<span class="badge fk">FK</span>{/if}
  </button>
</li>

<style>
  li { list-style: none; }
  .col {
    display: flex;
    align-items: baseline;
    /* Reset the global button base's justify-content:center (app.css). */
    justify-content: flex-start;
    gap: 0.35rem;
    width: 100%;
    padding: 0.1rem 0.3rem 0.1rem 1.4rem;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--dur-fast) var(--ease);
  }
  .col:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .col.selected,
  .col.selected:hover { background: var(--tree-selected-bg); }
  .col.selected .name { color: var(--tree-selected-fg); }
  .name { color: var(--text); }
  .type { color: var(--type-tag); font-size: 0.8rem; }
  .null { color: var(--faint); font-size: 0.7rem; }
  .badge {
    font-size: 0.65rem;
    padding: 0 0.25rem;
    border-radius: var(--r-sm);
    border: 1px solid;
    line-height: 1.4;
  }
  .pk { color: var(--warn); border-color: var(--warn); }
  .fk { color: var(--type-tag); border-color: var(--type-tag); }
</style>
```

- [ ] **Step 11: Type-check (single green checkpoint for all four nodes)**

Run: `just ui-check`
Expected: `0 ERRORS 0 WARNINGS`. Now that all four nodes declare/thread `parentKey`, the mutually-dependent prop wiring resolves cleanly.

- [ ] **Step 12: Run the full frontend test suite (guard against regressions)**

Run: `just ui-test`
Expected: all pass, including the 6 new `childKey` tests and the unchanged `columnLabel` tests.

- [ ] **Step 13: Commit (all four node files together)**

```bash
git add app/ui/src/lib/tree/DatabaseNode.svelte app/ui/src/lib/tree/TableNode.svelte app/ui/src/lib/tree/ViewNode.svelte app/ui/src/lib/tree/ColumnLeaf.svelte
git commit -m "feat(ui): selectable object-tree rows for all node types (billz-a8a)"
```

---

## Task 4: Full verification, visual pass, and close-out

**Files:** none (verification + review + PR).

- [ ] **Step 1: Full gate**

Run: `just verify`
Expected: `all checks passed` (Rust fmt/clippy/test + frontend check/test/build all green).

- [ ] **Step 2: Code review**

Run the review gate on the diff: `/code-review high` (or the repo's review pipeline). Address any real findings; re-run gates after fixes.

- [ ] **Step 3: Visual verification (mandatory — light AND dark)**

Launch the app (`just dev`, Vite on `:1420`) and drive it with Playwright (or Chrome DevTools MCP). With a connection selected and the tree populated:
- Click a **database** row, a **table** row, a **view** leaf, and a **column** leaf.
- For each, confirm the accent-tinted background + accent-press text appears on exactly the clicked row, and that clicking a different row moves the highlight (single selection).
- Confirm the highlight persists on hover (does not get replaced by the hover tint).
- Screenshot **light** and **dark** themes. Verify the tint reads clearly against the panel background and against a hovered neighbor in both themes.

If a connection/DEV box is unavailable, note it and verify as much as possible against whatever tree state renders; do not claim visual confirmation that was not performed.

- [ ] **Step 4: Close the bead**

```bash
bd close billz-a8a --reason="Tree-row selection highlight: module $state selection store keyed by connection-prefixed childKey path; all four node types selectable (leaves converted to buttons for a11y); shared --tree-selected-* tokens + scoped .selected styles. Verified light+dark."
```

- [ ] **Step 5: Push branch + open PR**

```bash
git push -u origin billz-a8a-tree-selection
gh pr create --title "feat(ui): object-tree row selection highlight (billz-a8a)" --body "<summary + testing + screenshots>"
```

Then sync beads per standing order: `bd dolt push`.

---

## Self-Review

**Spec coverage:**
- Selection store (module `$state`, ephemeral) → Task 2. ✓
- Pure `childKey` helper with connection-prefixed hierarchical keys → Task 1. ✓
- All four node types selectable; leaves converted to buttons → Task 3 (all four nodes in one commit — the `parentKey` declaration/threading interdependence forbids splitting into independently green-gated tasks). ✓
- `parentKey` threading (root = connection id) → Task 3 (render sites + prop declarations land together). ✓
- Shared selection appearance via tokens; per-component scoped `.selected` (deviation from spec's literal "one global rule," required because a bare global class cannot beat Svelte's hash-boosted scoped rules — CLAUDE.md permits deviation with a stated reason; the *values* remain single-sourced in `app.css`) → Task 2 tokens + Task 3 scoped rules. ✓
- Out of scope (arrow-nav, multi-select, persistence, view→column expansion) → none built. ✓
- Unit test `childKey`; visual light+dark → Task 1 tests, Task 4 visual. ✓

**Placeholder scan:** No TBD/TODO; every code step shows complete code; the PR body `<summary>` in Task 4 Step 5 is intentionally author-filled at PR time.

**Type consistency:** `childKey(parentKey, kind, name)` signature identical across Tasks 1 and 3. `selectNode(key)` / `selection.key` identical across Tasks 2 and 3. `parentKey: string` prop declared on `TableNode`/`ViewNode`/`ColumnLeaf` and passed from the `DatabaseNode`/`TableNode` render sites — all within Task 3, so `just ui-check` is only asserted green after every side is wired (Step 11). `key` derived identically (`$derived(childKey(...))`) in every node.
