// Shared list of databases for the active connection (cwt.10). Both the object
// tree's root level and the DB picker read this ONE store, so a connection
// switch / Refresh issues a single `list_databases` round-trip instead of two,
// and the two views can never show divergent lists. Mirrors
// connections.svelte.ts / savedQueries.svelte.ts: mutate the exported `$state`
// object's fields in place — never reassign the export.
//
// A `.svelte.ts` module can hold `$state` but not `$effect` (no effect root at
// module scope), so it can't self-trigger on `conns.activeId` — the always-
// mounted App.svelte owns that trigger and calls `load()`; every other consumer
// just reads `dbStore`.
import { type DatabaseInfo, listDatabases } from "./api";

export type DbStatus = "idle" | "loading" | "loaded" | "error";

export const dbStore = $state<{ status: DbStatus; list: DatabaseInfo[]; error: string | null }>({
  status: "idle",
  list: [],
  error: null,
});

// Monotonic token: only the newest load may write the store, so a slow response
// for a prior connection can't clobber a newer connection's list (the same race
// the cwt.9 review flagged, handled once here for both consumers).
let token = 0;

// Load databases for `connectionId` (null clears to idle). Errors are captured
// as `status: "error"` for the tree to surface; the picker treats a non-loaded
// store as an empty list.
export async function load(connectionId: string | null): Promise<void> {
  const mine = ++token;
  if (!connectionId) {
    dbStore.status = "idle";
    dbStore.list = [];
    dbStore.error = null;
    return;
  }
  dbStore.status = "loading";
  dbStore.error = null;
  try {
    const list = await listDatabases(connectionId);
    if (mine !== token) return; // superseded by a newer load — drop this result
    dbStore.list = list;
    dbStore.status = "loaded";
  } catch (e) {
    if (mine !== token) return;
    dbStore.list = [];
    dbStore.error = String(e);
    dbStore.status = "error";
  }
}
