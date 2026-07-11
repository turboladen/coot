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
  .name { color: #333; }
  .type { color: #888; font-size: 0.8rem; }
  .null { color: #aaa; font-size: 0.7rem; }
  .badge {
    font-size: 0.65rem;
    padding: 0 0.25rem;
    border-radius: 3px;
    border: 1px solid;
    line-height: 1.4;
  }
  .pk { color: #b8860b; border-color: #b8860b; }
  .fk { color: #3b82f6; border-color: #3b82f6; }
</style>
