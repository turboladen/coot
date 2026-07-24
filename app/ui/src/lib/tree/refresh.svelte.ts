// Per-connection bump counters that force a connection's object subtree to remount
// (rqb.5 Refresh). App.svelte keys the tree on `${connId}:${refreshNonce(connId)}`,
// so bumping a connection's nonce tears down that connection's node memos → re-fetch
// (and the just-invalidated core cache re-queries sys.*). Keyed per connection
// (billz-a5y.2) so a Refresh resets only that connection's subtree — when several
// connections render their own trees (billz-a5y.3), one refresh never collapses the
// others. Mutate via set/delete; never reassign the Map binding.
import { SvelteMap } from "svelte/reactivity";

const nonces = new SvelteMap<string, number>();

export function refreshNonce(id: string | null): number {
  if (id === null) return 0;
  return nonces.get(id) ?? 0;
}

export function bumpRefresh(id: string): void {
  nonces.set(id, (nonces.get(id) ?? 0) + 1);
}
