// Per-connection database lists (billz-a5y.2). Several connections can be expanded
// with live object trees at once, so the databases list is keyed BY CONNECTION in a
// SvelteMap rather than a single shared store. Each entry is the root of one
// connection's object tree; the DB picker + effectiveDb read the ACTIVE
// connection's entry.
//
// CONVENTION DEVIATION (deliberate): sibling stores (connections.svelte.ts,
// savedQueries.svelte.ts) say "mutate the exported $state object's fields in place,
// never reassign." A SvelteMap tracks reactivity PER KEY and per-entry
// *replacement*, not mutation of a nested plain object it holds, so every state
// change replaces the whole entry via `dbStore.set(id, {...})`. The invariant that
// survives: never reassign the `dbStore` Map binding itself — mutate only via
// set/delete. `databasesFor(id)` reads `dbStore.get(id)` inside the reactive access
// so a reader subscribes to a key even before its entry exists (SvelteMap tracks the
// key), and re-runs when a later `set` fills or replaces it.
//
// A `.svelte.ts` module can hold `$state` but not `$effect` (no effect root at
// module scope), so it can't self-trigger on `conns.activeId` — the always-mounted
// App.svelte owns the trigger (via `databaseLoadAction`) and calls
// ensure/clear/refresh; every other consumer just reads through `databasesFor`.
import { SvelteMap } from "svelte/reactivity";
import { type DatabaseInfo, listDatabases } from "./api";

export type DbStatus = "idle" | "loading" | "loaded" | "error";
export type DbEntry = { status: DbStatus; list: DatabaseInfo[]; error: string | null };

// The shared default returned for any connection with no entry yet (null active id,
// cold start, dangling id). Frozen so distinct absent ids can safely share one
// object — loaders NEVER mutate an entry, they always `set` a fresh one.
const IDLE_ENTRY: DbEntry = Object.freeze({ status: "idle", list: [], error: null });

const dbStore = new SvelteMap<string, DbEntry>();

// Per-connection monotonic tokens (plain, non-reactive bookkeeping): only the newest
// load for a given connection may write that connection's entry, so a slow response
// for one connection can't clobber a newer load of the SAME connection. Different
// connections never race — they write different keys.
const tokens = new Map<string, number>();

// Read a connection's entry (the reactive accessor every consumer goes through).
// Reads `dbStore.get(id)` for a non-null id INSIDE the reactive access so the reader
// subscribes to that key and re-runs once its entry is inserted/replaced; falls back
// to the shared frozen idle default when absent or when id is null.
export function databasesFor(id: string | null): DbEntry {
  if (id === null) return IDLE_ENTRY;
  return dbStore.get(id) ?? IDLE_ENTRY;
}

// billz-a5y.5: "loaded this session" derives from whether a connection's object store
// is populated. Kept trivial for that later unit.
export function isDatabasesLoaded(id: string | null): boolean {
  return databasesFor(id).status === "loaded";
}

// Load-once memo: ensure a connection's databases are loaded. Only a never-attempted
// (idle) connection loads — loaded (session cache), in-flight (loading), and errored
// entries all short-circuit, so switching back to a connection is instant and (billz-
// a5y.3) a tree root expanding a connection App already loaded won't double-fetch.
//
// CRITICAL — error is TERMINAL here, not a retry point. The App load effect reads this
// connection's status (via ensure), so it SUBSCRIBES to the entry; runLoad's error
// `set` would re-trigger the effect, which would ensure→runLoad again → an unbounded
// list_databases retry loop against an unreachable/erroring server. Refresh
// (`refreshDatabases`, which bypasses this memo) is the explicit retry. Never rejects
// — errors are captured into the entry.
export async function ensureDatabases(id: string): Promise<void> {
  if (databasesFor(id).status !== "idle") return;
  await runLoad(id);
}

// billz-zmw: clear a connection's entry to empty (never hits the DB). Bumps its token
// so any in-flight load for it is dropped, then removes the entry so `databasesFor`
// falls back to the shared idle default — the tree/picker show nothing for it.
export function clearDatabases(id: string): void {
  tokens.set(id, (tokens.get(id) ?? 0) + 1);
  dbStore.delete(id);
}

// Force a fresh reload for a connection, bypassing the ensure memo (rqb.5 Refresh,
// after the core schema cache is invalidated). Like `ensureDatabases`, captures errors
// into the entry and NEVER rejects — so an awaiting caller's follow-up (bumpRefresh →
// subtree remount) still runs even on a transient reload failure.
export async function refreshDatabases(id: string): Promise<void> {
  await runLoad(id);
}

// billz-a5y.2: drop a connection's cached entry entirely (called when the connection
// is deleted) so a since-removed id can't linger as a stale "loaded this session".
export function dropDatabases(id: string): void {
  tokens.delete(id);
  dbStore.delete(id);
}

// The shared loader for ensure/refresh: token-guarded so only the newest load for
// this connection writes its entry. Never throws — failures land in the entry as
// `status: "error"`.
async function runLoad(id: string): Promise<void> {
  const mine = (tokens.get(id) ?? 0) + 1;
  tokens.set(id, mine);
  dbStore.set(id, { status: "loading", list: [], error: null });
  try {
    const list = await listDatabases(id);
    if (mine !== tokens.get(id)) return; // superseded by a newer load — drop this result
    dbStore.set(id, { status: "loaded", list, error: null });
  } catch (e) {
    if (mine !== tokens.get(id)) return;
    dbStore.set(id, { status: "error", list: [], error: String(e) });
  }
}
