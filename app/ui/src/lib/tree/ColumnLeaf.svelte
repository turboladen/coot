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
