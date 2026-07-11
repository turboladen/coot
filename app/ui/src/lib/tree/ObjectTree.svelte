<script lang="ts">
  import { type DatabaseInfo, listDatabases } from "../api";
  import { conns } from "../connections.svelte";
  import DatabaseNode from "./DatabaseNode.svelte";

  // The active connection is the module-level source of truth (no prop plumbing).
  // App.svelte wraps this in {#key conns.activeId} so switching connections
  // remounts the whole tree — every node returns to idle and databases reload.
  const activeId = conns.activeId;

  let status = $state<"idle" | "loading" | "loaded" | "error">("idle");
  let error = $state<string | null>(null);
  let databases = $state<DatabaseInfo[]>([]);

  async function load() {
    if (!activeId) return;
    // Same double-fetch guard as the nodes: flip out of "idle" before the await.
    status = "loading";
    try {
      databases = await listDatabases(activeId);
      status = "loaded";
    } catch (e) {
      error = String(e);
      status = "error";
    }
  }

  // Root loads eagerly on mount (a connection is active). Node children stay lazy.
  if (activeId) load();
</script>

<div class="tree">
  <div class="header">
    <h2>Objects</h2>
    <!-- TODO(rqb.5): Refresh — invalidate core cache + remount tree (bump a nonce
         in the {#key} so the whole ObjectTree remounts past the node-local memo). -->
  </div>

  {#if !activeId}
    <p class="hint">Select a connection to browse its objects.</p>
  {:else if status === "loading"}
    <p class="hint">Loading…</p>
  {:else if status === "error"}
    <p class="hint err">{error}</p>
  {:else if databases.length === 0}
    <p class="hint">No databases.</p>
  {:else}
    <ul>
      {#each databases as db (db.databaseId)}
        <DatabaseNode id={activeId} database={db} />
      {/each}
    </ul>
  {/if}
</div>

<style>
  .tree { padding: 0.5rem; }
  .header { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 1rem; margin: 0.5rem 0; }
  .hint { color: #888; font-size: 0.9rem; }
  .err { color: #b91c1c; }
  ul { list-style: none; margin: 0; padding: 0; }
</style>
