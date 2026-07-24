<script lang="ts">
  import { refreshSchema } from "../api";
  import { conns } from "../connections.svelte";
  import { databasesFor, ensureDatabases, refreshDatabases } from "../databases.svelte";
  import { Database, Lock, RefreshCw } from "../icons";
  import DatabaseNode from "./DatabaseNode.svelte";
  import LoadingNote from "./LoadingNote.svelte";
  import { bumpRefresh } from "./refresh.svelte";

  // billz-a5y.3: one connection's object tree. `id` is the connection this tree roots
  // at (passed by the ConnectionNode that renders it) — multiple trees render at once,
  // each keyed on its own `${id}:${refreshNonce(id)}`. `locked` (from App's lockedIds)
  // means session-only + not-yet-unlocked; `onunlock` bubbles the locked empty-state's
  // "Enter password" up to App to open the prompt for THIS connection (no retarget).
  let { id, locked = false, onunlock }: { id: string; locked?: boolean; onunlock?: (id: string) => void } = $props();

  // The databases list lives in the per-connection store (billz-a5y.2); this component
  // READS this connection's entry via `databasesFor(id)`. The subtree components
  // (DatabaseNode/…) are already keyed via childKey rooted at `id`, so a selection
  // under one connection never false-matches a node under another.
  const entry = $derived(databasesFor(id));

  // Defensive presence gate. `id` comes from iterating conns.list, so it never
  // dangles in practice, but keep the guard so the empty state renders rather than a
  // stuck spinner if a connection vanishes mid-render.
  const known = $derived(conns.list.some((c) => c.id === id));

  // billz-a5y.3: each EXPANDED root self-loads its own databases. Idle-gated +
  // error-terminal inside ensureDatabases (see databases.svelte.ts), so this re-runs
  // harmlessly when the entry status changes and never loops on error; it dedupes
  // against App's active-connection ensure (which feeds the DB picker even when this
  // root is collapsed). Gated on `!locked` — billz-zmw: a locked connection must
  // never hit the DB.
  $effect(() => {
    if (!locked && known) ensureDatabases(id);
  });

  async function refresh() {
    // `!known || locked`: a locked connection must never hit the DB (billz-zmw) —
    // refreshDatabases would bypass the ensure gate and fire list_databases with no
    // session password. The button is also disabled when locked; this is defense in depth.
    if (!known || locked) return;
    // Invalidate core FIRST, then reload + bump: refreshSchema clears the core cache so
    // the re-fetches re-query SQL; refreshDatabases forces this connection's list
    // (bypassing the ensure memo); bumpRefresh remounts its subtree so every node
    // returns to idle. refreshDatabases never rejects (errors land in the entry), so
    // the bump always runs. Order matters — bumping before invalidate re-fills stale.
    await refreshSchema(id);
    await refreshDatabases(id);
    bumpRefresh(id);
  }
</script>

<div class="tree">
  <div class="header">
    <button class="refresh" onclick={refresh} disabled={!known || locked} title="Refresh"><RefreshCw size={13} /></button>
  </div>

  {#if locked}
    <!-- billz-zmw + billz-a5y.5 fold-in: a locked connection never loads, so it must
         show a clear "unlock to browse" body — NOT the misleading perpetual
         "Loading databases…" spinner it showed before (its entry stays idle). The
         button asks App to open the password prompt for THIS connection. -->
    <div class="empty-tree">
      <Lock size={20} />
      <p class="hint">Locked — this connection needs its session password.</p>
      <button class="unlock" onclick={() => onunlock?.(id)}>Enter password</button>
    </div>
  {:else if !known}
    <div class="empty-tree">
      <Database size={20} />
      <p class="hint">Connection unavailable.</p>
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
        <DatabaseNode {id} database={db} />
      {/each}
    </ul>
  {:else}
    <!-- idle (before the ensure effect resolves) or loading -->
    <LoadingNote text="Loading databases…" />
  {/if}
</div>

<style>
  .tree { padding: 0.1rem var(--sp-2) var(--sp-2); }
  .header { display: flex; align-items: center; justify-content: flex-end; }
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
    padding: var(--sp-4) var(--sp-2);
    text-align: center;
  }
  .empty-tree :global(svg) {
    color: var(--faint);
  }
  .empty-tree .hint {
    margin: 0;
  }
  .unlock {
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
    cursor: pointer;
  }
  .unlock:hover { background: color-mix(in srgb, var(--accent) 12%, var(--raised)); }
  ul { list-style: none; margin: 0; padding: 0; }
</style>
