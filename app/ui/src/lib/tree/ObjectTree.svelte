<script lang="ts">
  import { refreshSchema } from "../api";
  import { conns } from "../connections.svelte";
  import { databasesFor, refreshDatabases } from "../databases.svelte";
  import { Database, RefreshCw } from "../icons";
  import DatabaseNode from "./DatabaseNode.svelte";
  import LoadingNote from "./LoadingNote.svelte";
  import { bumpRefresh } from "./refresh.svelte";

  // billz-a5y.2: App owns `lockedConn`; it passes whether the active connection is
  // locked so Refresh can honor the billz-zmw "locked never hits DB" invariant.
  let { locked = false }: { locked?: boolean } = $props();

  // The active connection is the module-level source of truth (no prop plumbing).
  // App.svelte wraps this in {#key conns.activeId} so switching connections
  // remounts the whole tree — every node returns to idle. The databases list
  // itself lives in the per-connection store (billz-a5y.2), loaded by App.svelte's
  // effect and shared with the DB picker; this component READS the active
  // connection's entry via `databasesFor`.
  const activeId = conns.activeId;
  const entry = $derived(databasesFor(activeId));

  // billz-a5y.1: `conns.activeId` now mirrors the active tab's connection, which
  // can be a DANGLING id — its connection was deleted while another tab still
  // referenced it, and a later tab switch re-mirrors that stale id here. Presence-
  // gate the tree the same way run(), the DB picker, and `databaseLoadAction` do,
  // so a dangling id lands on the empty state instead of a permanent "Loading…"
  // spinner (App's load effect resolves it to a noop → this connection's entry
  // stays idle, which the final `else` would otherwise render as a stuck spinner).
  // `$derived` (not a
  // mount-captured const) so it flips true once `conns.list` finishes loading at
  // cold start — `activeId` itself stays mount-fixed under the {#key} remount.
  const known = $derived(activeId != null && conns.list.some((c) => c.id === activeId));

  async function refresh() {
    // `|| !activeId` narrows activeId to string below. `|| locked`: billz-zmw — a
    // locked connection must never hit the DB, and refreshDatabases below would
    // bypass App's load-effect gate and fire list_databases with no session
    // password. The button is also disabled for locked, this is defense in depth.
    if (!activeId || !known || locked) return;
    // Invalidate core FIRST, then reload + bump: refreshSchema clears the core cache
    // so the re-fetches re-query SQL; refreshDatabases forces this connection's list
    // (bypassing the ensure memo); bumpRefresh remounts its subtree so every node
    // returns to idle. refreshDatabases never rejects (errors land in the entry), so
    // the bump always runs. Order matters — bumping before invalidate re-fills stale.
    await refreshSchema(activeId);
    await refreshDatabases(activeId);
    bumpRefresh(activeId);
  }
</script>

<div class="tree">
  <div class="header">
    <h2>Objects</h2>
    <button class="refresh" onclick={refresh} disabled={!known || locked} title="Refresh"><RefreshCw size={13} /></button>
  </div>

  {#if !activeId || !known}
    <!-- `!activeId ||` is logically redundant (known ⇒ activeId != null) but narrows
         activeId to `string` in the else branches below (DatabaseNode's `id`). -->
    <div class="empty-tree">
      <Database size={20} />
      <p class="hint">Select a connection to browse its objects.</p>
    </div>
  {:else if entry.status === "error"}
    <p class="hint err">{entry.error}</p>
  {:else if entry.status === "loaded" && entry.list.length === 0}
    <div class="empty-tree">
      <Database size={20} />
      <p class="hint">No databases.</p>
    </div>
  {:else if entry.status === "loaded"}
    <ul>
      {#each entry.list as db (db.databaseId)}
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
