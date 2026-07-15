<script lang="ts">
  import { type DatabaseInfo, listTables, listViews, type TableInfo, type ViewInfo } from "../api";
  import { ChevronDown, ChevronRight, Database } from "../icons";
  import LoadingNote from "./LoadingNote.svelte";
  import TableNode from "./TableNode.svelte";
  import ViewNode from "./ViewNode.svelte";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";

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
  const key = $derived(childKey(id, "db", database.name));

  async function toggle() {
    selectNode(key);
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
  <button class="row" class:selected={selection.key === key} class:muted={!isOnline} onclick={toggle} disabled={!isOnline}>
    <span class="twisty">
      {#if isOnline}
        {#if expanded}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
      {/if}
    </span>
    <Database size={13} />
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
          <TableNode {id} db={database.name} table={t} parentKey={key} />
        {/each}
      </ul>
      <div class="group">Views</div>
      <ul>
        {#each views as v (v.schema + "." + v.name)}
          <ViewNode view={v} parentKey={key} />
        {/each}
      </ul>
    {/if}
  {/if}
</li>

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
    padding: 0.15rem 0.3rem;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    font: inherit;
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    color: var(--muted);
    transition: background var(--dur-fast) var(--ease);
  }
  .row:not(:disabled):hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
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
  .label { color: var(--text); font-weight: 500; }
  /* B1: greys the NAME too — the .label rule above otherwise wins on the muted row. */
  .row.muted .label { color: var(--faint); }
  .state { color: var(--faint); font-size: 0.75rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  .group {
    padding: 0.1rem 0 0.1rem 0.7rem;
    font-size: 0.75rem;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .note { padding: 0.1rem 0 0.1rem 0.7rem; font-size: 0.8rem; color: var(--muted); }
  .err { color: var(--danger); white-space: normal; }
</style>
