<script lang="ts">
  import { type ColumnInfo, listColumns, type SavedQuery, type TableInfo } from "../api";
  import { openScopedQuery } from "../openScopedQuery.svelte";
  import { queriesReferencingTable } from "../paramBarLogic";
  import { library } from "../savedQueries.svelte";
  import { newTabWithContent } from "../tabs.svelte";
  import ColumnLeaf from "./ColumnLeaf.svelte";
  import LoadingNote from "./LoadingNote.svelte";
  import { selectTop1000 } from "./selectTopQuery";

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
  // rqb.6: open a new tab pre-seeded with a 3-part SELECT TOP 1000 (not auto-run).
  // Pass `db` as the tab's target so the picker (cwt.9) shows the table's DB and
  // any follow-up 2-part query the user types resolves against it.
  function openSelect() {
    newTabWithContent(selectTop1000(db, table.schema, table.name), db);
  }

  // d28.7: right-click → context menu of saved queries that use @table.
  let menu = $state<{ x: number; y: number } | null>(null);
  const scopedQueries = $derived(queriesReferencingTable(library.list));

  function openMenu(e: MouseEvent) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY };
  }
  function runScoped(q: SavedQuery) {
    menu = null;
    openScopedQuery(id, db, table.schema, table.name, q).catch((e) => {
      console.error("scoped-open failed", e);
    });
  }
</script>

<li>
  <!-- Double-click also fires two onclicks (toggle is idempotent — expanded
       returns to its prior state), harmless for a single-user tool. -->
  <button class="row" onclick={toggle} ondblclick={openSelect} oncontextmenu={openMenu}>
    <span class="twisty">{expanded ? "▼" : "▶"}</span>
    <span class="label">{table.schema}.{table.name}</span>
  </button>
  {#if expanded}
    {#if status === "loading"}
      <LoadingNote text="Loading columns…" />
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

<svelte:window onkeydown={(e) => { if (e.key === "Escape") menu = null; }} />

{#if menu}
  <button class="menu-backdrop" aria-label="Close menu" onclick={() => (menu = null)}></button>
  <div class="ctx-menu" style="left: {menu.x}px; top: {menu.y}px;">
    <div class="ctx-header">Run saved query scoped to {table.schema}.{table.name}</div>
    {#if scopedQueries.length === 0}
      <div class="ctx-empty">No saved queries use @table</div>
    {:else}
      {#each scopedQueries as q (q.id)}
        <button class="ctx-item" onclick={() => runScoped(q)}>{q.name}</button>
      {/each}
    {/if}
  </div>
{/if}

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
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: none;
    border: none;
    padding: 0;
    cursor: default;
  }
  .ctx-menu {
    position: fixed;
    z-index: 41;
    min-width: 12rem;
    background: #fff;
    border: 1px solid #ccc;
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
    padding: 0.25rem;
    font-size: 0.85rem;
  }
  .ctx-header {
    padding: 0.2rem 0.5rem;
    color: #888;
    font-size: 0.72rem;
    border-bottom: 1px solid #eee;
    white-space: nowrap;
  }
  .ctx-empty { padding: 0.35rem 0.5rem; color: #888; font-size: 0.8rem; }
  .ctx-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.25rem 0.5rem;
    background: none;
    border: none;
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    border-radius: 4px;
  }
  .ctx-item:hover { background: rgba(59, 130, 246, 0.1); }
</style>
