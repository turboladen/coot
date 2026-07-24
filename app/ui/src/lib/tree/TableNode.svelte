<script lang="ts">
  import { type ColumnInfo, listColumns, type SavedQuery, type TableInfo } from "../api";
  import { ChevronDown, ChevronRight, Table2 } from "../icons";
  import { openScopedQuery } from "../openScopedQuery.svelte";
  import { queriesReferencingTable } from "../paramBarLogic";
  import { library } from "../savedQueries.svelte";
  import { newTabWithContent } from "../tabs.svelte";
  import ColumnLeaf from "./ColumnLeaf.svelte";
  import LoadingNote from "./LoadingNote.svelte";
  import { selectTop1000 } from "./selectTopQuery";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";

  let { id, db, table, parentKey }: { id: string; db: string; table: TableInfo; parentKey: string } = $props();

  const key = $derived(childKey(parentKey, "table", table.schema + "." + table.name));

  // Node-local lazy-load state (see plan §3b). Children are memoized here so a
  // collapse/re-expand is instant with zero round-trip; the core SchemaCache is
  // the backstop for remounts.
  let expanded = $state(false);
  let status = $state<"idle" | "loading" | "loaded" | "error">("idle");
  let error = $state<string | null>(null);
  let columns = $state<ColumnInfo[]>([]);

  async function toggle() {
    selectNode(key);
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
  // any follow-up 2-part query the user types resolves against it. Pass this node's
  // `id` as the tab's connection (billz-a5y.1) so the new tab runs against the tree's
  // connection even if a different tab was active when it was opened.
  function openSelect() {
    newTabWithContent(selectTop1000(db, table.schema, table.name), db, null, id);
  }

  // d28.7: right-click → context menu of saved queries that use @table.
  let menu = $state<{ x: number; y: number } | null>(null);
  const scopedQueries = $derived(queriesReferencingTable(library.list));

  function openMenu(e: MouseEvent) {
    e.preventDefault();
    selectNode(key); // right-click doesn't fire onclick, so move selection here too (billz-ek1)
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
  <button class="row" class:selected={selection.key === key} onclick={toggle} ondblclick={openSelect} oncontextmenu={openMenu} aria-expanded={expanded}>
    <span class="twisty">{#if expanded}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}</span>
    <Table2 size={13} />
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
          <ColumnLeaf column={col} parentKey={key} />
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
    align-items: center;
    /* Reset the global button base's justify-content:center (app.css) — without
       this the chevron+icon+label cluster centers in the full-width row. */
    justify-content: flex-start;
    gap: 0.3rem;
    width: 100%;
    padding: 0.1rem 0.3rem 0.1rem 0.7rem;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    color: var(--muted);
    transition: background var(--dur-fast) var(--ease);
  }
  .row:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .row.selected,
  .row.selected:hover { background: var(--tree-selected-bg); }
  .row.selected .label { color: var(--tree-selected-fg); }
  .twisty {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    width: 0.8rem;
    flex: none;
  }
  .row :global(svg) { color: var(--muted); flex: none; }
  .label { color: var(--text); }
  ul { list-style: none; margin: 0; padding: 0; }
  .note { padding: 0.1rem 0 0.1rem 1.4rem; font-size: 0.8rem; color: var(--muted); }
  .err { color: var(--danger); white-space: normal; }
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
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-md);
    padding: 0.25rem;
    font-size: 0.85rem;
  }
  .ctx-header {
    padding: 0.2rem 0.5rem;
    color: var(--muted);
    font-size: 0.72rem;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .ctx-empty { padding: 0.35rem 0.5rem; color: var(--muted); font-size: 0.8rem; }
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
    border-radius: var(--r-sm);
    color: var(--text);
  }
  .ctx-item:hover { background: color-mix(in srgb, var(--accent) 12%, transparent); }
</style>
