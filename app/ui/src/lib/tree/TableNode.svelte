<script lang="ts">
  import { type ColumnInfo, listColumns, type TableInfo } from "../api";
  import ColumnLeaf from "./ColumnLeaf.svelte";

  let { id, db, table }: { id: string; db: string; table: TableInfo } = $props();

  // Node-local lazy-load state (see plan §3b). Children are memoized here so a
  // collapse/re-expand is instant with zero round-trip; the core SchemaCache is
  // the backstop for remounts.
  let expanded = $state(false);
  let status = $state<"idle" | "loading" | "loaded" | "error">("idle");
  let error = $state<string | null>(null);
  let columns = $state<ColumnInfo[]>([]);

  async function toggle() {
    expanded = !expanded;
    if (!expanded || status !== "idle") return; // re-expand = memo, no refetch
    // CRITICAL (double-fetch guard): flip out of "idle" SYNCHRONOUSLY, before any
    // await, so a rapid second click sees status !== "idle" and does nothing.
    status = "loading";
    try {
      // Backend returns columns in ordinal order (ORDER BY column_id) — preserve it.
      columns = await listColumns(id, db, table.schema, table.name);
      status = "loaded";
    } catch (e) {
      error = String(e);
      status = "error";
    }
  }
  // TODO(rqb.6): ondblclick → open a new editor tab with SELECT TOP 1000 FROM [schema].[table]
  // TODO(d28.7): context menu — run saved query scoped to this table
</script>

<li>
  <button class="row" onclick={toggle}>
    <span class="twisty">{expanded ? "▼" : "▶"}</span>
    <span class="label">{table.schema}.{table.name}</span>
  </button>
  {#if expanded}
    {#if status === "loading"}
      <div class="note">Loading…</div>
    {:else if status === "error"}
      <div class="note err">{error}</div>
    {:else}
      <ul>
        {#each columns as col (col.ordinal)}
          <ColumnLeaf column={col} />
        {/each}
      </ul>
    {/if}
  {/if}
</li>

<style>
  li { list-style: none; }
  .row {
    display: flex;
    align-items: baseline;
    gap: 0.3rem;
    width: 100%;
    padding: 0.1rem 0 0.1rem 0.7rem;
    background: none;
    border: none;
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }
  .twisty { color: #888; font-size: 0.7rem; width: 0.8rem; }
  .label { color: #333; }
  ul { list-style: none; margin: 0; padding: 0; }
  .note { padding: 0.1rem 0 0.1rem 1.4rem; font-size: 0.8rem; color: #888; }
  .err { color: #b91c1c; white-space: normal; }
</style>
