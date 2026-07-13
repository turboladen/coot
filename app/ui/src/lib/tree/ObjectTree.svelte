<script lang="ts">
  import { refreshSchema } from "../api";
  import { conns } from "../connections.svelte";
  import { dbStore } from "../databases.svelte";
  import { Database, RefreshCw } from "../icons";
  import DatabaseNode from "./DatabaseNode.svelte";
  import LoadingNote from "./LoadingNote.svelte";
  import { bumpRefresh } from "./refresh.svelte";

  // The active connection is the module-level source of truth (no prop plumbing).
  // App.svelte wraps this in {#key conns.activeId} so switching connections
  // remounts the whole tree — every node returns to idle. The databases list
  // itself lives in the shared `dbStore` (cwt.10), loaded once by App.svelte's
  // effect and shared with the DB picker; this component only READS it.
  const activeId = conns.activeId;

  async function refresh() {
    if (!activeId) return;
    // Invalidate core FIRST, then bump: App.svelte's effect re-runs the shared
    // load (missing the just-cleared cache → re-queries SQL) and the {#key}
    // remounts the tree so every node returns to idle. Bumping before invalidate
    // would re-fill from stale.
    await refreshSchema(activeId);
    bumpRefresh();
  }
</script>

<div class="tree">
  <div class="header">
    <h2>Objects</h2>
    <button class="refresh" onclick={refresh} disabled={!activeId} title="Refresh"><RefreshCw size={13} /></button>
  </div>

  {#if !activeId}
    <div class="empty-tree">
      <Database size={20} />
      <p class="hint">Select a connection to browse its objects.</p>
    </div>
  {:else if dbStore.status === "error"}
    <p class="hint err">{dbStore.error}</p>
  {:else if dbStore.status === "loaded" && dbStore.list.length === 0}
    <div class="empty-tree">
      <Database size={20} />
      <p class="hint">No databases.</p>
    </div>
  {:else if dbStore.status === "loaded"}
    <ul>
      {#each dbStore.list as db (db.databaseId)}
        <DatabaseNode id={activeId} database={db} />
      {/each}
    </ul>
  {:else}
    <!-- idle (before App's effect fires) or loading -->
    <LoadingNote text="Loading databases…" />
  {/if}
</div>

<style>
  .tree { padding: 0.5rem; }
  .header { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 1rem; margin: 0.5rem 0; }
  .refresh {
    display: inline-flex;
    align-items: center;
    background: none;
    border: none;
    font: inherit;
    color: var(--muted);
    cursor: pointer;
    padding: 0 0.3rem;
  }
  .refresh:disabled { color: var(--faint); cursor: default; }
  .hint { color: var(--muted); font-size: 0.9rem; }
  .err { color: var(--danger); }
  .empty-tree {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--sp-2);
    padding: var(--sp-5) var(--sp-2);
    text-align: center;
  }
  .empty-tree :global(svg) {
    color: var(--faint);
  }
  .empty-tree .hint {
    margin: 0;
  }
  ul { list-style: none; margin: 0; padding: 0; }
</style>
