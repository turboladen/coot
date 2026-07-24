// Multi-root sidebar UI state (billz-a5y.3): which connection roots are EXPANDED
// (each showing its own live object tree) and which one is FOCUSED (the new-tab
// default). Both are DISTINCT from `conns.activeId` (the active tab's connection,
// from billz-a5y.1) — expanding/browsing a root must NEVER retarget the active tab.
// That decoupling is the whole point of the epic (kill the silent wrong-server).
//
// A `.svelte.ts` module holds $state but has no effect root, so it only exposes
// plain mutators; every reader subscribes through the exported `sidebar` object.
// Imports only `svelte/reactivity` — no `conns`/`tabs` — so there is no import cycle
// (tabs.svelte.ts reads focusedId; connections.svelte.ts calls pruneRoot on delete).
//
// Ephemeral (session-only, not persisted): matches "loaded this session" — a restart
// starts with nothing expanded, focus null, and App re-expands the active connection.
import { SvelteSet } from "svelte/reactivity";

const state = $state<{ expanded: SvelteSet<string>; focusedId: string | null }>({
  expanded: new SvelteSet<string>(),
  focusedId: null,
});

export const sidebar = state;

// BROWSE gesture (chevron/twisty): toggle a root's tree. Focus follows only when
// EXPANDING — collapsing a root must not make it the new-tab default (a mild oddity
// the reviewer flagged). Never touches conns.activeId.
export function toggleRoot(id: string): void {
  if (state.expanded.has(id)) {
    state.expanded.delete(id);
  } else {
    state.expanded.add(id);
    state.focusedId = id;
  }
}

// Ensure a root is expanded + focused. Used by the RETARGET gesture (connection-name
// click, alongside setActiveConnection) and by App's launch auto-expand of the active
// connection. Idempotent.
export function expandRoot(id: string): void {
  state.expanded.add(id);
  state.focusedId = id;
}

// Drop a deleted connection's id from the sidebar state (called from remove()). Stale
// ids are otherwise inert (the roots iterate conns.list, so a removed id has no row,
// and defaultTabConnection guards focus against the live set) — this is tidy cleanup.
export function pruneRoot(id: string): void {
  state.expanded.delete(id);
  if (state.focusedId === id) state.focusedId = null;
}
