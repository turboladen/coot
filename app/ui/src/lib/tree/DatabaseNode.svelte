<script lang="ts">
  import { type DatabaseInfo, listTables, listViews, type TableInfo, type ViewInfo } from "../api";
  import LoadingNote from "./LoadingNote.svelte";
  import TableNode from "./TableNode.svelte";
  import ViewNode from "./ViewNode.svelte";

  let { id, database }: { id: string; database: DatabaseInfo } = $props();

  // Node-local lazy-load state (see plan §3b). Tables + views are memoized here.
  let expanded = $state(false);
  let status = $state<"idle" | "loading" | "loaded" | "error">("idle");
  let error = $state<string | null>(null);
  let tables = $state<TableInfo[]>([]);
  let views = $state<ViewInfo[]>([]);

  // rqb.4: only ONLINE dbs are expandable — enumerating tables/views in a
  // RESTORING/OFFLINE db errors, so those rows grey out and stay collapsed.
  const isOnline = $derived(database.stateDesc === "ONLINE");

  async function toggle() {
    if (!isOnline) return; // rqb.4: don't enumerate a RESTORING/OFFLINE db
    expanded = !expanded;
    if (!expanded || status !== "idle") return; // re-expand = memo, no refetch
    // CRITICAL (double-fetch guard): flip out of "idle" SYNCHRONOUSLY, before any
    // await, so a rapid second click sees status !== "idle" and does nothing.
    status = "loading";
    try {
      // Load both lists in parallel. Promise.all rejects with the FIRST failure
      // reason — the node just shows that one message (single-user tool; no
      // allSettled/partial-render over-build).
      const [t, v] = await Promise.all([listTables(id, database.name), listViews(id, database.name)]);
      tables = t;
      views = v;
      status = "loaded";
    } catch (e) {
      error = String(e);
      status = "error";
    }
  }
</script>

<li>
  <button class="row" class:muted={!isOnline} onclick={toggle} disabled={!isOnline}>
    <span class="twisty">{isOnline ? (expanded ? "▼" : "▶") : ""}</span>
    <span class="label">{database.name}</span>
    {#if !isOnline}<span class="state">({database.stateDesc})</span>{/if}
  </button>
  {#if expanded}
    {#if status === "loading"}
      <LoadingNote text="Loading tables & views…" />
    {:else if status === "error"}
      <div class="note err">{error}</div>
    {:else}
      <!-- "Tables"/"Views" are static grouping labels (plan note b) — plain rows,
           no twisty, no expand state. They present the two lists, they are not
           SSMS folder node types. -->
      <div class="group">Tables</div>
      <ul>
        {#each tables as t (t.schema + "." + t.name)}
          <TableNode {id} db={database.name} table={t} />
        {/each}
      </ul>
      <div class="group">Views</div>
      <ul>
        {#each views as v (v.schema + "." + v.name)}
          <ViewNode view={v} />
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
    padding: 0.15rem 0;
    background: none;
    border: none;
    font: inherit;
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }
  .twisty { color: #888; font-size: 0.7rem; width: 0.8rem; }
  .label { color: #333; font-weight: 500; }
  /* B1: greys the NAME too — the .label rule above otherwise wins on the muted row. */
  .row.muted .label { color: #aaa; }
  .state { color: #aaa; font-size: 0.75rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  .group {
    padding: 0.1rem 0 0.1rem 0.7rem;
    font-size: 0.75rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .note { padding: 0.1rem 0 0.1rem 0.7rem; font-size: 0.8rem; color: #888; }
  .err { color: #b91c1c; white-space: normal; }
</style>
