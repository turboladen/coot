<script lang="ts">
  import type { ColumnInfo } from "../api";
  import { columnLabel } from "./columnLabel";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";
  import { elidedTitle } from "./elidedTitle";

  let { column, parentKey }: { column: ColumnInfo; parentKey: string } = $props();

  // Pure formatter (bun-tested) -> the display pieces. Badges render only when set.
  const label = $derived(columnLabel(column));
  const key = $derived(childKey(parentKey, "col", column.name));
</script>

<li>
  <button class="col" class:selected={selection.key === key} onclick={() => selectNode(key)}>
    <span class="name" use:elidedTitle={label.name}>{label.name}</span>
    <span class="type" use:elidedTitle={label.dataType}>: {label.dataType}</span>
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
    gap: var(--sp-1);
    width: 100%;
    /* depth-3 indent (2.1rem) — one step below the table row (billz-a5y.8). */
    padding: 0.2rem 0.3rem 0.2rem 2.1rem;
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
  /* billz-6s0 — see TableNode.svelte's .label for why min-width:0 is required.
     The NAME yields FIRST: nullability and the PK/FK badges are short and carry the
     information you'd scan a column list for, so they hold their size (flex:none)
     and a long name ellipses in front of them rather than shoving them off.
     The lopsided shrink factors below (99999 vs 1) make that a priority, not a
     proportional split: the name absorbs essentially the whole deficit and the
     type only starts to ellipse once the name has nothing left to give. Without a
     shrink path on .type, a long alias/UDT type name (sys.types returns the type's
     own name; `format_sql_type`'s `_ => t` arm passes it through) pushed the row
     past the sidebar, where `.conn-tree { overflow-x: hidden }` now clips it dead —
     no ellipsis, no scrollbar, and the trailing badges gone with no way back. */
  .name {
    color: var(--text);
    min-width: 0;
    flex-shrink: 99999;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .type {
    color: var(--type-tag);
    font-size: 0.8rem;
    min-width: 0;
    flex-shrink: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .null { color: var(--faint); font-size: 0.7rem; flex: none; }
  .badge {
    flex: none;
    font-size: 0.65rem;
    padding: 0 0.25rem;
    border-radius: var(--r-sm);
    border: 1px solid;
    line-height: 1.4;
  }
  .pk { color: var(--warn); border-color: var(--warn); }
  .fk { color: var(--type-tag); border-color: var(--type-tag); }
</style>
