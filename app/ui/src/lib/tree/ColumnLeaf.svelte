<script lang="ts">
  import type { ColumnInfo } from "../api";
  import { columnLabel } from "./columnLabel";

  let { column }: { column: ColumnInfo } = $props();

  // Pure formatter (bun-tested) → the display pieces. Badges render only when set.
  const label = $derived(columnLabel(column));
</script>

<li class="col">
  <span class="name">{label.name}</span>
  <span class="type">: {label.dataType}</span>
  <span class="null">{label.nullText}</span>
  {#if label.isPrimaryKey}<span class="badge pk">PK</span>{/if}
  {#if label.isForeignKey}<span class="badge fk">FK</span>{/if}
</li>

<style>
  .col {
    display: flex;
    align-items: baseline;
    gap: 0.35rem;
    padding: 0.1rem 0 0.1rem 1.4rem;
    font-size: 0.85rem;
    white-space: nowrap;
  }
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
